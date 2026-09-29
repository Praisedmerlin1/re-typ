use std::path::PathBuf;

use crate::cli::{Cli, Format, SvgBackend, TexEngine, TikzBackend};

/// Fully resolved conversion options.
#[derive(Debug, Clone)]
pub struct Config {
    pub format: Format,
    pub output_dir: Option<PathBuf>,
    pub suffix: String,
    pub in_place: bool,
    pub force: bool,
    pub skip_typst_files: bool,
    pub strict: bool,
    pub verbose: bool,
    pub dry_run: bool,
    pub check_deps: bool,
    pub svg_backend: SvgBackend,
    pub tex_engine: TexEngine,
    pub tikz_backend: TikzBackend,
    pub tikz_preamble: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            format: Format::Md,
            output_dir: None,
            suffix: ".typst".to_string(),
            in_place: false,
            force: false,
            skip_typst_files: true,
            strict: false,
            verbose: false,
            dry_run: false,
            check_deps: false,
            svg_backend: SvgBackend::Auto,
            tex_engine: TexEngine::Auto,
            tikz_backend: TikzBackend::Auto,
            tikz_preamble: None,
        }
    }
}

impl Config {
    pub fn from_cli(cli: &Cli) -> anyhow::Result<Self> {
        if cli.in_place && cli.format == Format::Typ {
            anyhow::bail!(
                "--in-place is only supported with --format md; \
                 writing Typst markup over a Markdown file would be destructive"
            );
        }
        if cli.in_place && cli.output_dir.is_some() {
            anyhow::bail!("--in-place cannot be combined with --output-dir");
        }
        if !cli.in_place
            && cli.format == Format::Md
            && cli.suffix.is_empty()
            && cli.output_dir.is_none()
        {
            anyhow::bail!("empty --suffix would overwrite the input; pass --in-place instead");
        }
        Ok(Self {
            format: cli.format,
            output_dir: cli.output_dir.clone(),
            suffix: cli.suffix.clone(),
            in_place: cli.in_place,
            force: cli.force,
            skip_typst_files: !cli.no_skip_typst_files,
            strict: cli.strict,
            verbose: cli.verbose,
            dry_run: cli.dry_run,
            check_deps: cli.check_deps,
            svg_backend: cli.svg_backend,
            tex_engine: cli.tex_engine,
            tikz_backend: cli.tikz_backend,
            tikz_preamble: cli.tikz_preamble.clone(),
        })
    }
}
