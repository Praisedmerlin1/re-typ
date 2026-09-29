use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::assets::AssetNamer;
use crate::cli::TikzBackend;
use crate::config::Config;
use crate::deps::Tools;
use crate::report::FileReport;
use crate::tikz;

/// What a TikZ block became.
pub enum TikzOutcome {
    /// An SVG file was (or would be) written.
    Svg(PathBuf),
    /// Native CeTZ code to embed inline.
    Cetz(String),
}

/// Shared conversion state for a single input document.
pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub tools: Tools,
    pub preamble: Option<String>,
    pub namer: AssetNamer,
    pub report: FileReport,
    pub asset_dir: PathBuf,
}

impl<'a> Ctx<'a> {
    pub fn new(
        cfg: &'a Config,
        tools: Tools,
        preamble: Option<String>,
        asset_dir: PathBuf,
        stem: String,
        input: PathBuf,
    ) -> Self {
        Self {
            cfg,
            tools,
            preamble,
            namer: AssetNamer::new(asset_dir.clone(), stem),
            report: FileReport::new(input),
            asset_dir,
        }
    }

    /// Turn the next TikZ block into either a CeTZ snippet or an SVG asset.
    ///
    /// Returns `None` when rendering failed and the block should be left as-is
    /// (only possible without `--strict`).
    pub fn render_tikz(&mut self, content: &str) -> Result<Option<TikzOutcome>> {
        self.report.tikz_total += 1;

        let try_cetz = match self.cfg.tikz_backend {
            TikzBackend::Svg => false,
            TikzBackend::Cetz => true,
            TikzBackend::Auto => !tikz::looks_unsupported_for_cetz(content),
        };

        if try_cetz && let Some(code) = tikz::cetz_from_tikz(content) {
            self.report.cetz_total += 1;
            return Ok(Some(TikzOutcome::Cetz(code)));
        }
        if try_cetz {
            self.report
                .notes
                .push("CeTZ transpilation produced no drawing; falling back to SVG".to_string());
        }

        let preview = self.namer.peek();

        if self.cfg.dry_run {
            let path = self.namer.commit();
            self.report.svgs.push(path.clone());
            return Ok(Some(TikzOutcome::Svg(path)));
        }

        if preview.exists() && !self.cfg.force {
            let path = self.namer.commit();
            self.report.svgs.push(path.clone());
            return Ok(Some(TikzOutcome::Svg(path)));
        }

        match tikz::compile(content, &self.tools, self.preamble.as_deref()) {
            Ok(svg) => {
                let path = self.namer.commit();
                std::fs::create_dir_all(&self.asset_dir)?;
                std::fs::write(&path, svg)
                    .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
                self.report.svgs.push(path.clone());
                Ok(Some(TikzOutcome::Svg(path)))
            }
            Err(err) => {
                if self.cfg.strict {
                    return Err(err);
                }
                let first = format!("{err:#}")
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("render failed")
                    .trim()
                    .to_string();
                self.report.warnings.push(format!(
                    "TikZ render failed; block left unchanged ({first})"
                ));
                Ok(None)
            }
        }
    }

    /// Record a math conversion failure; returns Err under `--strict`.
    pub fn note_math_failure(&mut self, source: &str, error: &str) -> Result<()> {
        let message = format!("math conversion failed: {error} (source: `{source}`)");
        if self.cfg.strict {
            anyhow::bail!("{message}");
        }
        self.report
            .math_failed
            .push((source.to_string(), error.to_string()));
        self.report.warnings.push(message);
        Ok(())
    }

    /// Record a warning; returns Err under `--strict`.
    pub fn warn(&mut self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        if self.cfg.strict {
            anyhow::bail!("{message}");
        }
        self.report.warnings.push(message);
        Ok(())
    }

    pub fn asset_file_name(path: &Path) -> String {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}
