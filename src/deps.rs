use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use wait_timeout::ChildExt;

use crate::cli::{SvgBackend, TexEngine};

/// Default time budget for a single external tool invocation. Generous
/// because a TeX engine may download missing packages on its first run.
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgKind {
    Dvisvgm,
    Pdftocairo,
    Pdf2svg,
    Inkscape,
}

impl SvgKind {
    pub fn name(self) -> &'static str {
        match self {
            SvgKind::Dvisvgm => "dvisvgm",
            SvgKind::Pdftocairo => "pdftocairo",
            SvgKind::Pdf2svg => "pdf2svg",
            SvgKind::Inkscape => "inkscape",
        }
    }

    fn binary(self) -> &'static str {
        self.name()
    }

    fn version_args(self) -> &'static [&'static str] {
        match self {
            SvgKind::Dvisvgm => &["--version"],
            SvgKind::Pdftocairo => &["-v"],
            _ => &["--version"],
        }
    }

    fn candidates(pref: SvgBackend) -> &'static [SvgKind] {
        const ALL: [SvgKind; 4] = [
            SvgKind::Dvisvgm,
            SvgKind::Pdftocairo,
            SvgKind::Pdf2svg,
            SvgKind::Inkscape,
        ];
        match pref {
            SvgBackend::Auto => &ALL,
            SvgBackend::Dvisvgm => &[SvgKind::Dvisvgm],
            SvgBackend::Pdftocairo => &[SvgKind::Pdftocairo],
            SvgBackend::Pdf2svg => &[SvgKind::Pdf2svg],
            SvgBackend::Inkscape => &[SvgKind::Inkscape],
        }
    }
}

#[derive(Debug, Clone)]
pub struct TexTool {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SvgTool {
    pub kind: SvgKind,
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub tex: Option<TexTool>,
    pub svg: Option<SvgTool>,
}

impl Tools {
    pub fn require(&self) -> Result<(&TexTool, &SvgTool)> {
        let tex = self.tex.as_ref().ok_or_else(|| {
            anyhow!(
                "no TeX engine found (looked for pdflatex, xelatex, lualatex, tectonic). \
                 Install TeX Live or tectonic to render TikZ diagrams"
            )
        })?;
        let svg = self.svg.as_ref().ok_or_else(|| {
            anyhow!(
                "no PDF -> SVG backend found (looked for dvisvgm, pdftocairo, pdf2svg, inkscape). \
                 Install dvisvgm or poppler-utils (pdftocairo) to render TikZ diagrams"
            )
        })?;
        Ok((tex, svg))
    }
}

fn find(binary: &str) -> Option<PathBuf> {
    which::which(binary).ok()
}

fn version_of(path: &PathBuf, args: &[&str]) -> Option<String> {
    let output = Command::new(path).args(args).output().ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    }
    text.lines().next().map(|l| l.trim().to_string())
}

/// Detect the external toolchain without failing.
pub fn detect(svg_pref: SvgBackend, tex_pref: TexEngine) -> Tools {
    let tex = detect_tex(tex_pref);

    let svg = SvgKind::candidates(svg_pref).iter().find_map(|kind| {
        let path = find(kind.binary())?;
        Some(SvgTool {
            kind: *kind,
            version: version_of(&path, kind.version_args()),
            path,
        })
    });

    Tools { tex, svg }
}

fn detect_tex(pref: TexEngine) -> Option<TexTool> {
    let order: &[&str] = match pref {
        TexEngine::Auto => &["pdflatex", "xelatex", "lualatex", "tectonic"],
        TexEngine::Pdflatex => &["pdflatex"],
        TexEngine::Xelatex => &["xelatex"],
        TexEngine::Lualatex => &["lualatex"],
        TexEngine::Tectonic => &["tectonic"],
    };
    for name in order {
        let Some(path) = find(name) else { continue };
        // In auto mode, skip engines that cannot compile a trivial document
        // (e.g. a TeX Live install missing its format files). Tectonic is
        // accepted as-is because probing it may trigger a bundle download.
        if pref == TexEngine::Auto && *name != "tectonic" && !tex_probe_works(&path) {
            continue;
        }
        return Some(TexTool {
            name: (*name).to_string(),
            version: version_of(&path, &["--version"]),
            path,
        });
    }
    None
}

/// Compile a tiny document to verify the engine is actually usable.
fn tex_probe_works(path: &PathBuf) -> bool {
    let Ok(tmp) = tempfile::tempdir() else {
        return false;
    };
    let tex = tmp.path().join("probe.tex");
    if std::fs::write(
        &tex,
        "\\documentclass{article}\\begin{document}ok\\end{document}\n",
    )
    .is_err()
    {
        return false;
    }
    let mut cmd = Command::new(path);
    let out_dir = format!("-output-directory={}", tmp.path().display());
    let input = tex.display().to_string();
    cmd.args([
        "-no-shell-escape",
        "-interaction=batchmode",
        "-halt-on-error",
    ])
    .arg(&out_dir)
    .arg(&input);
    cmd.output()
        .map(|o| o.status.success() && tmp.path().join("probe.pdf").exists())
        .unwrap_or(false)
}

/// Print a toolchain report. Returns `true` when a usable pair exists.
pub fn print_report(tools: &Tools) -> bool {
    println!("external toolchain:");
    match &tools.tex {
        Some(t) => println!(
            "  tex engine : {:<10} {}{}",
            t.name,
            t.path.display(),
            t.version
                .as_deref()
                .map(|v| format!("  ({v})"))
                .unwrap_or_default()
        ),
        None => println!("  tex engine : MISSING (pdflatex / xelatex / lualatex / tectonic)"),
    }
    match &tools.svg {
        Some(t) => println!(
            "  svg backend: {:<10} {}{}",
            t.kind.name(),
            t.path.display(),
            t.version
                .as_deref()
                .map(|v| format!("  ({v})"))
                .unwrap_or_default()
        ),
        None => println!("  svg backend: MISSING (dvisvgm / pdftocairo / pdf2svg / inkscape)"),
    }
    tools.tex.is_some() && tools.svg.is_some()
}

/// Run a command with a timeout, redirecting stdout/stderr into `dir`.
pub fn run_with_timeout(cmd: &mut Command, dir: &std::path::Path, tag: &str) -> Result<()> {
    let out_path = dir.join(format!("{tag}.stdout"));
    let err_path = dir.join(format!("{tag}.stderr"));
    let stdout = std::fs::File::create(&out_path)
        .with_context(|| format!("creating {}", out_path.display()))?;
    let stderr = std::fs::File::create(&err_path)
        .with_context(|| format!("creating {}", err_path.display()))?;
    cmd.stdout(stdout).stderr(stderr);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn {}", cmd.get_program().to_string_lossy()))?;

    match child.wait_timeout(TOOL_TIMEOUT)? {
        Some(status) if status.success() => Ok(()),
        Some(status) => {
            let log = read_log(&err_path);
            Err(anyhow!(
                "command `{}` exited with {status}\n{}",
                cmd.get_program().to_string_lossy(),
                log
            ))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(anyhow!(
                "command `{}` timed out after {}s",
                cmd.get_program().to_string_lossy(),
                TOOL_TIMEOUT.as_secs()
            ))
        }
    }
}

fn read_log(path: &std::path::Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let tail: Vec<&str> = text.lines().rev().take(25).collect();
    tail.into_iter().rev().collect::<Vec<_>>().join("\n")
}
