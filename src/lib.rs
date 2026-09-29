pub mod assets;
pub mod cli;
pub mod config;
pub mod context;
pub mod deps;
pub mod fsutil;
pub mod markdown;
pub mod math;
pub mod report;
pub mod rewrite;
pub mod tikz;

use std::path::Path;

use anyhow::{Context, Result};
use cli::{Cli, Format};
use config::Config;
use context::Ctx;
use deps::Tools;
use report::{FileReport, Summary};

/// Options used for parsing Markdown in both output modes.
pub fn markdown_options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options;
    let mut options = Options::empty();
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    options
}

/// Run the CLI. Returns the process exit code.
pub fn run(cli: Cli) -> Result<i32> {
    let cfg = Config::from_cli(&cli)?;
    let tools = deps::detect(cfg.svg_backend, cfg.tex_engine);

    if cfg.check_deps {
        let ok = deps::print_report(&tools);
        return Ok(if ok { 0 } else { 1 });
    }

    if cli.inputs.is_empty() {
        anyhow::bail!("no input files or directories given (see --help)");
    }

    let files = fsutil::collect_inputs(&cli.inputs)?;
    if files.is_empty() {
        eprintln!("no markdown files found");
        return Ok(1);
    }

    let preamble = match &cfg.tikz_preamble {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("reading preamble {}", path.display()))?,
        ),
        None => None,
    };

    let mut reports = Vec::with_capacity(files.len());
    for input in &files {
        let report = convert_file(input, &cfg, &tools, preamble.clone())
            .with_context(|| format!("processing {}", input.display()))?;
        report.print(cfg.dry_run);
        reports.push(report);
    }

    Summary::from_reports(&reports).print(cfg.dry_run);

    if cfg.strict && reports.iter().any(FileReport::has_problems) {
        return Ok(1);
    }
    Ok(0)
}

fn convert_file(
    input: &Path,
    cfg: &Config,
    tools: &Tools,
    preamble: Option<String>,
) -> Result<FileReport> {
    let src = std::fs::read_to_string(input)
        .with_context(|| format!("reading input {}", input.display()))?;
    let (converted, report) = convert_source(&src, input, cfg, tools, preamble)?;

    if !cfg.dry_run
        && let Some(output) = &report.output
    {
        write_output(output, &converted, cfg.in_place)?;
    }

    Ok(report)
}

/// Convert an in-memory document. Exposed for tests and embedding.
pub fn convert_source(
    src: &str,
    input: &Path,
    cfg: &Config,
    tools: &Tools,
    preamble: Option<String>,
) -> Result<(String, FileReport)> {
    let output = fsutil::output_path(input, cfg)?;
    let asset_dir = fsutil::asset_dir(&output);
    let stem = fsutil::document_stem(input);

    if !cfg.dry_run && !asset_dir.as_os_str().is_empty() {
        std::fs::create_dir_all(&asset_dir)
            .with_context(|| format!("creating {}", asset_dir.display()))?;
    }

    let mut ctx = Ctx::new(
        cfg,
        tools.clone(),
        preamble,
        asset_dir,
        stem,
        input.to_path_buf(),
    );

    let converted = match cfg.format {
        Format::Md => rewrite::convert(src, &mut ctx)?,
        Format::Typ => markdown::convert(src, &mut ctx)?,
    };

    let mut report = std::mem::take(&mut ctx.report);
    report.output = Some(output);
    Ok((converted, report))
}

fn write_output(path: &Path, content: &str, in_place: bool) -> Result<()> {
    if in_place {
        let tmp = path.with_extension("retyp.tmp");
        std::fs::write(&tmp, content).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    } else {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}
