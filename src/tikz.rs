use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::cli::SvgBackground;
use crate::deps::{self, SvgKind, TexTool, Tools};

/// Collapse whitespace between a control word and a following `{` or `[`.
///
/// LaTeX treats `\begin {circuitikz}`, `\usepackage {tikz}` and
/// `\draw [red]` exactly like their unspaced forms, but the string checks
/// below do not. Notes copied out of editors often contain the spaced form,
/// which previously caused diagrams to be missed or wrapped in a second
/// `document` environment that TeX rejects.
pub fn normalize_control_spacing(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();

    while let Some(c) = chars.next() {
        out.push(c);
        if c != '\\' {
            continue;
        }

        // Copy the control word (letters only; `\\` and `\{` are left alone).
        while let Some(&next) = chars.peek() {
            if next.is_ascii_alphabetic() {
                out.push(next);
                chars.next();
            } else {
                break;
            }
        }

        // Look ahead for spaces/tabs before a `{` or `[`.
        let mut lookahead = chars.clone();
        let mut spaces = 0usize;
        while let Some(&next) = lookahead.peek() {
            if next == ' ' || next == '\t' {
                spaces += 1;
                lookahead.next();
            } else {
                break;
            }
        }
        if spaces > 0 && matches!(lookahead.peek(), Some('{' | '[')) {
            for _ in 0..spaces {
                chars.next();
            }
        }
    }

    out
}

/// Does this fenced code block describe a TikZ picture we should render?
pub fn is_tikz_block(info: &str, content: &str) -> bool {
    let lang = info.split_whitespace().next().unwrap_or("");
    let lang_ok = lang.eq_ignore_ascii_case("tikz") || lang.eq_ignore_ascii_case("latex");
    if !lang_ok {
        return false;
    }
    let content = normalize_control_spacing(content);
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
    let src = normalize_control_spacing(src);
    MARKERS.iter().any(|marker| src.contains(marker))
}

/// Transpile a TikZ snippet to CeTZ, or `None` when nothing usable is produced.
pub fn cetz_from_tikz(src: &str) -> Option<String> {
    let src = normalize_control_spacing(src);
    let output = tylax::tikz::convert_tikz_to_cetz(&src);
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
    // `standalone`'s `tikz` option wraps bare drawing commands in a
    // `tikzpicture`. When the snippet already contains its own picture
    // environment (notably circuitikz), that wrapper breaks the bounding box
    // and TeX emits a full letter page instead of a cropped diagram.
    let class_options = if contains_picture_environment(body) {
        "border=2pt"
    } else {
        "tikz,border=2pt"
    };
    format!(
        "\\documentclass[{class_options}]{{standalone}}\n\
         \\usepackage{{tikz}}\n\
         \\usetikzlibrary{{arrows.meta,positioning,calc}}\n\
         {all}{auto}\n\
         \\begin{{document}}\n{body}\\end{{document}}\n"
    )
}

/// Does the snippet already contain an environment that draws its own picture?
fn contains_picture_environment(body: &str) -> bool {
    [
        "\\begin{tikzpicture}",
        "\\begin{circuitikz}",
        "\\begin{axis}",
        "\\begin{tikzcd}",
    ]
    .iter()
    .any(|marker| body.contains(marker))
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
    // Normalize `\begin {x}` and friends so the structural splits below work,
    // then a full document is used untouched.
    let content = normalize_control_spacing(content);
    let content = content.as_str();
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
    compile_with_background(content, tools, extra_preamble, SvgBackground::Auto)
}

/// Compile a TikZ snippet and paint the requested background behind it.
pub fn compile_with_background(
    content: &str,
    tools: &Tools,
    extra_preamble: Option<&str>,
    background: SvgBackground,
) -> Result<Vec<u8>> {
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
    let bytes = std::fs::read(&svg).with_context(|| format!("reading {}", svg.display()))?;
    Ok(add_background(&bytes, background))
}

/// Insert a background rectangle as the first element of an SVG.
///
/// SVG renderers leave the canvas transparent, so a diagram drawn in black
/// disappears against a dark note theme and one drawn in white disappears
/// against a light theme. `Auto` inspects the paint colours and picks the
/// contrasting background.
pub fn add_background(svg: &[u8], background: SvgBackground) -> Vec<u8> {
    if background == SvgBackground::None {
        return svg.to_vec();
    }
    let Ok(text) = std::str::from_utf8(svg) else {
        return svg.to_vec();
    };
    // Do not stack rectangles if this SVG was already processed.
    if text.contains("data-retyp-background") {
        return svg.to_vec();
    }
    let Some(svg_start) = text.find("<svg") else {
        return svg.to_vec();
    };
    let Some(rel_end) = text[svg_start..].find('>') else {
        return svg.to_vec();
    };
    let tag_end = svg_start + rel_end + 1;
    let tag = &text[svg_start..tag_end];

    let fill = match background {
        SvgBackground::Light => "#ffffff",
        SvgBackground::Dark => "#1e1e1e",
        SvgBackground::Auto => {
            if prefers_dark_background(text) {
                "#1e1e1e"
            } else {
                "#ffffff"
            }
        }
        SvgBackground::None => return svg.to_vec(),
    };

    let (x, y, width, height) = match view_box(tag) {
        Some((x, y, w, h)) if w > 0.0 && h > 0.0 => {
            (x.to_string(), y.to_string(), w.to_string(), h.to_string())
        }
        _ => ("0".into(), "0".into(), "100%".into(), "100%".into()),
    };

    let rect = format!(
        "<rect data-retyp-background=\"true\" x=\"{x}\" y=\"{y}\" \
         width=\"{width}\" height=\"{height}\" fill=\"{fill}\"/>"
    );
    let mut out = String::with_capacity(text.len() + rect.len());
    out.push_str(&text[..tag_end]);
    out.push_str(&rect);
    out.push_str(&text[tag_end..]);
    out.into_bytes()
}

/// Read the four numbers from the root `<svg>` tag's `viewBox` attribute.
fn view_box(tag: &str) -> Option<(f32, f32, f32, f32)> {
    let index = tag.find("viewBox").or_else(|| tag.find("viewbox"))?;
    let rest = tag[index + "viewBox".len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let end = rest[1..].find(quote)?;
    let value = &rest[1..1 + end];
    let numbers: Vec<f32> = value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    match numbers.as_slice() {
        [x, y, w, h] => Some((*x, *y, *w, *h)),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaintKind {
    Fill,
    Stroke,
}

/// True when most of the diagram's ink is light, i.e. it was drawn for a dark
/// canvas and needs a dark background.
fn prefers_dark_background(text: &str) -> bool {
    let mut weighted_luminance = 0.0f32;
    let mut weight = 0.0f32;
    let mut saw_white = false;

    for (kind, value) in paints(text) {
        let Some((r, g, b)) = parse_color(&value) else {
            continue;
        };
        let luminance = perceived_luminance(r, g, b);
        let is_white = luminance > 0.97;
        if is_white {
            saw_white = true;
            // White fills are usually occlusion patches or label backgrounds,
            // not the colour of the drawing itself.
            if kind == PaintKind::Fill {
                continue;
            }
        }
        let paint_weight = if kind == PaintKind::Stroke { 2.0 } else { 1.0 };
        weighted_luminance += luminance * paint_weight;
        weight += paint_weight;
    }

    if weight == 0.0 {
        // Nothing but white paint: it needs a dark canvas to be visible.
        return saw_white;
    }
    weighted_luminance / weight > 0.5
}

/// Collect `fill`/`stroke` values from XML attributes and CSS declarations.
fn paints(text: &str) -> Vec<(PaintKind, String)> {
    let mut found = Vec::new();
    for (key, kind) in [("fill", PaintKind::Fill), ("stroke", PaintKind::Stroke)] {
        let mut search = 0usize;
        while let Some(offset) = text[search..].find(key) {
            let start = search + offset;
            search = start + key.len();

            // Skip longer property names such as `fill-opacity`/`stroke-width`.
            let before = text[..start].chars().next_back();
            if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                continue;
            }

            let rest = text[start + key.len()..].trim_start();
            let value = if let Some(rest) = rest.strip_prefix('=') {
                quoted_value(rest.trim_start())
            } else if let Some(rest) = rest.strip_prefix(':') {
                rest.trim_start()
                    .split([';', '}', '"'])
                    .next()
                    .map(|value| value.trim().to_string())
            } else {
                None
            };
            if let Some(value) = value {
                found.push((kind, value));
            }
        }
    }
    found
}

fn quoted_value(text: &str) -> Option<String> {
    let quote = text.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let end = text[1..].find(quote)?;
    Some(text[1..1 + end].to_string())
}

fn perceived_luminance(r: u8, g: u8, b: u8) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

fn parse_color(value: &str) -> Option<(u8, u8, u8)> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "" | "none" | "transparent" | "currentcolor" | "inherit" | "initial"
    ) || lower.starts_with("url(")
    {
        return None;
    }

    if let Some(hex) = lower.strip_prefix('#') {
        return match hex.len() {
            3 | 4 => {
                let channel = |i: usize| u8::from_str_radix(&hex[i..i + 1].repeat(2), 16).ok();
                Some((channel(0)?, channel(1)?, channel(2)?))
            }
            6 | 8 => {
                let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
                Some((channel(0)?, channel(2)?, channel(4)?))
            }
            _ => None,
        };
    }

    if let Some(inner) = lower
        .strip_prefix("rgb(")
        .or_else(|| lower.strip_prefix("rgba("))
    {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() < 3 {
            return None;
        }
        let channel = |part: &str| -> Option<u8> {
            if let Some(percent) = part.strip_suffix('%') {
                percent
                    .parse::<f32>()
                    .ok()
                    .map(|n| (n * 2.55).round().clamp(0.0, 255.0) as u8)
            } else {
                part.parse::<f32>().ok().map(|n| n.clamp(0.0, 255.0) as u8)
            }
        };
        return Some((channel(parts[0])?, channel(parts[1])?, channel(parts[2])?));
    }

    let named = match lower.as_str() {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "lime" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "cyan" | "aqua" => (0, 255, 255),
        "magenta" | "fuchsia" => (255, 0, 255),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "maroon" => (128, 0, 0),
        "olive" => (128, 128, 0),
        "green" => (0, 128, 0),
        "purple" => (128, 0, 128),
        "teal" => (0, 128, 128),
        "navy" => (0, 0, 128),
        "orange" => (255, 165, 0),
        _ => return None,
    };
    Some(named)
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
        // Bare commands need standalone's `tikz` option to get a picture.
        assert!(doc.contains("\\documentclass[tikz,border=2pt]{standalone}"));
    }

    #[test]
    fn explicit_picture_environments_are_not_double_wrapped() {
        // `[tikz]` would nest circuitikz inside another tikzpicture, which
        // produces a full letter page instead of a cropped diagram.
        let doc = document("\\begin{circuitikz}\\draw (0,0) to[R] (2,0);\\end{circuitikz}");
        assert!(
            doc.contains("\\documentclass[border=2pt]{standalone}"),
            "{doc}"
        );
        assert!(!doc.contains("\\documentclass[tikz,border=2pt]{standalone}"));
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

    #[test]
    fn spaced_control_words_are_normalized() {
        assert_eq!(
            normalize_control_spacing("\\begin {document}\n\\end {document}"),
            "\\begin{document}\n\\end{document}"
        );
        assert_eq!(
            normalize_control_spacing("\\draw [red] (0,0);"),
            "\\draw[red] (0,0);"
        );
        // `\\` (line break) and escaped braces must not be touched.
        assert_eq!(
            normalize_control_spacing("a \\\\ b \\{ c"),
            "a \\\\ b \\{ c"
        );
    }

    #[test]
    fn spaced_document_wrapper_is_not_nested() {
        // Regression: circuitikz snippets copied from notes use
        // `\begin {document}` / `\begin {circuitikz}` with a space. The spaced
        // form used to be wrapped in a second document environment, which
        // TeX rejects with "Can be used only in preamble".
        let snippet = "\\usepackage {circuitikz}\n\
                       \\begin {document}\n\
                       \\begin {circuitikz}\n\
                       \\draw (0,0) to[R] (2,0);\n\
                       \\end {circuitikz}\n\
                       \\end {document}";
        let doc = document(snippet);
        assert_eq!(doc.matches("\\begin{document}").count(), 1);
        assert_eq!(doc.matches("\\end{document}").count(), 1);
        assert_eq!(doc.matches("\\begin{circuitikz}").count(), 1);
        assert_eq!(doc.matches("\\usepackage{circuitikz}").count(), 1);
    }

    fn with_background(svg: &str, background: SvgBackground) -> String {
        String::from_utf8(add_background(svg.as_bytes(), background)).unwrap()
    }

    #[test]
    fn dark_ink_gets_a_white_background() {
        let svg = "<svg viewBox='0 0 100 50'><path stroke='#000' fill='none' d='M0 0'/></svg>";
        let out = with_background(svg, SvgBackground::Auto);
        assert!(out.contains("data-retyp-background=\"true\""));
        assert!(out.contains("fill=\"#ffffff\""));
        assert!(out.contains("width=\"100\"") && out.contains("height=\"50\""));
        // The rectangle is the first child of <svg>.
        assert!(out.contains("><rect "), "{out}");
    }

    #[test]
    fn light_ink_gets_a_dark_background() {
        let svg = "<svg viewBox='0 0 10 10'><path stroke='rgb(255,255,255)' fill='none'/></svg>";
        let out = with_background(svg, SvgBackground::Auto);
        assert!(out.contains("fill=\"#1e1e1e\""), "{out}");
    }

    #[test]
    fn white_occlusion_fills_do_not_force_a_dark_background() {
        let svg = "<svg viewBox='0 0 10 10'>\
                   <path stroke='#000' fill='none'/>\
                   <rect fill='#fff'/><rect fill='#fff'/><rect fill='#fff'/>\
                   </svg>";
        let out = with_background(svg, SvgBackground::Auto);
        assert!(out.contains("fill=\"#ffffff\""), "{out}");
    }

    #[test]
    fn explicit_backgrounds_override_detection() {
        let svg = "<svg viewBox='0 0 10 10'><path stroke='#fff'/></svg>";
        assert!(with_background(svg, SvgBackground::Light).contains("fill=\"#ffffff\""));
        assert!(with_background(svg, SvgBackground::Dark).contains("fill=\"#1e1e1e\""));
        assert_eq!(with_background(svg, SvgBackground::None), svg);
    }

    #[test]
    fn background_is_not_added_twice() {
        let svg = "<svg viewBox='0 0 10 10'><path stroke='#000'/></svg>";
        let once = with_background(svg, SvgBackground::Auto);
        let twice = with_background(&once, SvgBackground::Auto);
        assert_eq!(once, twice);
    }
}
