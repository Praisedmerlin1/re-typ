use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::cli::Format;
use crate::config::Config;

/// Expand inputs into a sorted, de-duplicated list of `.md` files.
pub fn collect_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        let meta = std::fs::metadata(input)
            .with_context(|| format!("cannot access input {}", input.display()))?;
        if meta.is_file() {
            files.push(input.clone());
        } else if meta.is_dir() {
            for entry in WalkDir::new(input).sort_by_file_name() {
                let entry = entry.with_context(|| format!("walking {}", input.display()))?;
                if entry.file_type().is_file() && is_markdown(entry.path()) {
                    files.push(entry.into_path());
                }
            }
        } else {
            anyhow::bail!(
                "input {} is neither a file nor a directory",
                input.display()
            );
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

/// Resolve the output path for `input`.
pub fn output_path(input: &Path, cfg: &Config) -> Result<PathBuf> {
    if cfg.in_place {
        return Ok(input.to_path_buf());
    }

    let dir = match &cfg.output_dir {
        Some(dir) => dir.clone(),
        None => input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };

    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| anyhow::anyhow!("input {} has no file stem", input.display()))?;

    let name = match cfg.format {
        Format::Md => format!("{stem}{}.md", cfg.suffix),
        Format::Typ => format!("{stem}.typ"),
    };
    Ok(dir.join(name))
}

/// Directory where SVG assets are written: alongside the output document.
pub fn asset_dir(output: &Path) -> PathBuf {
    output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Document stem used for asset naming.
pub fn document_stem(input: &Path) -> String {
    input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "diagram".to_string())
}
