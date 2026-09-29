use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::deps::{self, SvgKind, TexTool, Tools};

/// Does this fenced code block describe a TikZ picture we should render?
pub fn is_tikz_block(info: &str, content: &str) -> bool {
    let lang = info.split_whitespace().next().unwrap_or("");
    let lang_ok = lang.eq_ignore_ascii_case("tikz") || lang.eq_ignore_ascii_case("latex");
    if !lang_ok {
        return false;
    }
    content.contains("\\begin{tikzpicture}")
        || content.contains("\\begin{axis}")
        || content.contains("\\begin{tikzcd}")
        || content.contains("\\begin{circuitikz}")
}

/// Constructs TyLax's CeTZ transpiler silently drops (producing an empty
/// canvas). Blocks containing these are routed to the SVG backend.
pub fn looks_unsupported_for_cetz(src: &str) -> bool {
    const MARKERS: &[&str] = &[
        "circuitikz",
        "pgfplots",
        "\\addplot",
        "\\begin{axis}",
        "\\begin{semilogxaxis}",
        "\\begin{semilogyaxis}",
        "\\begin{loglogaxis}",
        "tikzcd",
        "\\tikzset",
        "\\pgfmath",
        "\\pgfplotsset",
        "\\newcommand",
        "\\def\\",
    ];
    MARKERS.iter().any(|marker| src.contains(marker))
}

/// Transpile a TikZ snippet to CeTZ, or `None` when nothing usable is produced.
pub fn cetz_from_tikz(src: &str) -> Option<String> {
    let output = tylax::tikz::convert_tikz_to_cetz(src);
    if cetz_is_usable(&output) {
        Some(output.trim().to_string())
    } else {
        None
    }
}

/// True when a TyLax CeTZ snippet contains at least one drawing command.
fn cetz_is_usable(output: &str) -> bool {
    output.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with("#import")
            && !trimmed.starts_with("#canvas")
            && trimmed != "import draw: *"
            && !trimmed.starts_with("})")
    })
}

/// Split preamble-only commands out of a TikZ snippet so they can be hoisted
/// above `\begin{document}` while being removed from the body.
fn split_preamble(src: &str) -> (String, String) {
    let mut preamble = String::new();
    let mut body = String::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        let is_preamble = [
            "\\usepackage",
            "\\usetikzlibrary",
            "\\pgfplotsset",
            "\\tikzset",
        ]
        .iter()
        .any(|p| trimmed.starts_with(p));
        if is_preamble {
            preamble.push_str(line.trim());
            preamble.push('\n');
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    (preamble, body)
}

/// Split a snippet into (preamble text, document body).
///
/// Handles snippets that are a bare document body wrapped in
/// `\begin{document}...\end{document}` but have no `\documentclass`, which is
/// common in notes copied from a full LaTeX file.
fn split_document(content: &str) -> (String, String) {
    const BEGIN: &str = "\\begin{document}";
    const END: &str = "\\end{document}";
    match content.find(BEGIN) {
        Some(index) => {
            let pre = &content[..index];
            let rest = &content[index + BEGIN.len()..];
            let mid = match rest.find(END) {
                Some(end) => &rest[..end],
                None => rest,
            };
            (pre.to_string(), mid.to_string())
        }
        None => (String::new(), content.to_string()),
    }
}

fn assemble(declared: &str, extra: Option<&str>, body: &str) -> String {
    let extra = extra.unwrap_or("");
    let mut all = String::new();
    all.push_str(declared);
    all.push_str(extra);
    let auto = auto_preamble(body, &all);
    format!(
        "\\documentclass[tikz,border=2pt]{{standalone}}\n\
         \\usepackage{{tikz}}\n\
         \\usetikzlibrary{{arrows.meta,positioning,calc}}\n\
         {all}{auto}\n\
         \\begin{{document}}\n{body}\\end{{document}}\n"
    )
}

/// Packages implied by a snippet's content but not declared inside it.
fn auto_preamble(content: &str, already: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    if content.contains("circuitikz") {
        lines.push("\\usepackage{circuitikz}");
    }
    if content.contains("\\begin{axis}")
        || content.contains("\\addplot")
        || content.contains("pgfplots")
    {
        lines.push("\\usepackage{pgfplots}");
        lines.push("\\pgfplotsset{compat=1.18}");
    }
    if content.contains("tikzcd") {
        lines.push("\\usepackage{tikz-cd}");
        lines.push("\\usetikzlibrary{cd}");
    }
    let mut out = String::new();
    for line in lines {
        if !already.contains(line) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Build a standalone LaTeX document around a TikZ snippet.
fn build_document(content: &str, extra_preamble: Option<&str>) -> String {
    // A full document is used untouched.
    if content.contains("\\documentclass") {
        return content.to_string();
    }

    let (pre, mid) = split_document(content);

    if pre.trim().is_empty() {
        // Bare snippet: hoist any preamble commands sitting at the top.
        let (hoisted, body) = split_preamble(&mid);
        return assemble(&hoisted, extra_preamble, &body);
    }

    // Explicit document body: everything before `\begin{document}` is preamble.
    let mut declared = String::new();
    for line in pre.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            declared.push_str(trimmed);
            declared.push('\n');
        }
    }
    assemble(&declared, extra_preamble, &mid)
}

fn tex_args(tex: &TexTool, dir: &Path, tex_file: &Path) -> Vec<String> {
    match tex.name.as_str() {
        "tectonic" => vec![
            "--outdir".into(),
            dir.display().to_string(),
            tex_file.display().to_string(),
        ],
        _ => vec![
            "-no-shell-escape".into(),
            "-interaction=nonstopmode".into(),
            "-halt-on-error".into(),
            "-file-line-error".into(),
            format!("-output-directory={}", dir.display()),
            tex_file.display().to_string(),
        ],
    }
}

fn svg_args(kind: SvgKind, pdf: &Path, svg: &Path) -> Vec<String> {
    match kind {
        SvgKind::Dvisvgm => vec![
            "--pdf".into(),
            format!("--output={}", svg.display()),
            pdf.display().to_string(),
        ],
        SvgKind::Pdftocairo => vec![
            "-svg".into(),
            pdf.display().to_string(),
            svg.display().to_string(),
        ],
        SvgKind::Pdf2svg => vec![pdf.display().to_string(), svg.display().to_string()],
        SvgKind::Inkscape => vec![
            pdf.display().to_string(),
            "--export-type=svg".into(),
            format!("--export-filename={}", svg.display()),
        ],
    }
}

/// Compile a TikZ snippet into SVG bytes using the detected toolchain.
pub fn compile(content: &str, tools: &Tools, extra_preamble: Option<&str>) -> Result<Vec<u8>> {
    let (tex, svg_tool) = tools.require()?;
    let document = build_document(content, extra_preamble);

    let tmp = tempfile::Builder::new()
        .prefix("re-typ-tikz-")
        .tempdir()
        .context("creating temporary build directory")?;
    let dir = tmp.path();

    let tex_file = dir.join("diagram.tex");
    std::fs::write(&tex_file, document).context("writing temporary LaTeX document")?;

    let mut tex_cmd = std::process::Command::new(&tex.path);
    tex_cmd.args(tex_args(tex, dir, &tex_file));
    deps::run_with_timeout(&mut tex_cmd, dir, "tex")
        .with_context(|| format!("LaTeX engine `{}` failed", tex.name))?;

    let pdf = find_first(dir, Path::new("diagram.pdf"), "pdf")
        .ok_or_else(|| anyhow!("LaTeX engine `{}` produced no PDF", tex.name))?;

    let svg_path = dir.join("diagram.svg");
    let mut svg_cmd = std::process::Command::new(&svg_tool.path);
    svg_cmd.args(svg_args(svg_tool.kind, &pdf, &svg_path));
    deps::run_with_timeout(&mut svg_cmd, dir, "svg")
        .with_context(|| format!("SVG backend `{}` failed", svg_tool.kind.name()))?;

    let svg = find_first(dir, &svg_path, "svg")
        .ok_or_else(|| anyhow!("SVG backend `{}` produced no SVG", svg_tool.kind.name()))?;
    std::fs::read(&svg).with_context(|| format!("reading {}", svg.display()))
}

/// Locate a produced file; some backends add numeric suffixes for extra pages.
fn find_first(dir: &Path, expected: &Path, ext: &str) -> Option<PathBuf> {
    if expected.exists() {
        return Some(expected.to_path_buf());
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == ext).unwrap_or(false))
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(snippet: &str) -> String {
        build_document(snippet, None)
    }

    #[test]
    fn bare_snippet_is_wrapped_once() {
        let doc = document("\\draw (0,0) circle (1cm);");
        assert_eq!(doc.matches("\\begin{document}").count(), 1);
        assert_eq!(doc.matches("\\end{document}").count(), 1);
        assert!(doc.contains("\\draw (0,0) circle (1cm);"));
    }

    #[test]
    fn snippet_with_preamble_commands_is_hoisted() {
        let doc = document("\\usepackage{tikz}\n\\usetikzlibrary{3d}\n\\draw (0,0) circle (1cm);");
        // Library declared before \begin{document}.
        let begin = doc.find("\\begin{document}").unwrap();
        assert!(doc[..begin].contains("\\usetikzlibrary{3d}"));
        // Body does not repeat the preamble command.
        let body = &doc[begin..];
        assert!(!body.contains("\\usetikzlibrary{3d}"));
    }

    #[test]
    fn wrapped_document_body_is_unwrapped() {
        // Regression: notes often paste a full body with \begin{document} but
        // no \documentclass. We must not emit a nested document environment.
        let snippet = "\\usepackage{tikz}\n\
                       \\usetikzlibrary{3d}\n\
                       \\begin{document}\n\
                       \\begin{tikzpicture}\n\
                       \\draw (0,0) -- (1,1);\n\
                       \\end{tikzpicture}\n\
                       \\end{document}";
        let doc = document(snippet);
        assert_eq!(doc.matches("\\begin{document}").count(), 1);
        assert_eq!(doc.matches("\\end{document}").count(), 1);
        assert_eq!(doc.matches("\\begin{tikzpicture}").count(), 1);
        assert!(doc.contains("\\usetikzlibrary{3d}"));
    }

    #[test]
    fn full_document_is_left_untouched() {
        let snippet = "\\documentclass{article}\n\\begin{document}x\\end{document}";
        assert_eq!(document(snippet), snippet);
    }

    #[test]
    fn auto_preamble_adds_missing_packages() {
        let doc = document("\\begin{circuitikz}\n\\draw (0,0) to[R] (2,0);\n\\end{circuitikz}");
        assert!(doc.contains("\\usepackage{circuitikz}"));
        let doc = document("\\begin{axis}\\addplot {x^2};\\end{axis}");
        assert!(doc.contains("\\usepackage{pgfplots}"));
        let doc = document("\\begin{tikzcd} A \\arrow[r] & B \\end{tikzcd}");
        assert!(doc.contains("\\usepackage{tikz-cd}"));
    }

    #[test]
    fn auto_preamble_does_not_duplicate_declared_packages() {
        let doc = document("\\usepackage{circuitikz}\n\\begin{circuitikz}\\end{circuitikz}");
        assert_eq!(doc.matches("\\usepackage{circuitikz}").count(), 1);
    }
}
