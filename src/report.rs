use std::path::PathBuf;

/// Per-file conversion statistics and warnings.
#[derive(Debug, Default, Clone)]
pub struct FileReport {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub math_total: usize,
    pub math_ok: usize,
    pub math_failed: Vec<(String, String)>,
    pub tikz_total: usize,
    pub cetz_total: usize,
    pub svgs: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

impl FileReport {
    pub fn new(input: PathBuf) -> Self {
        Self {
            input,
            ..Default::default()
        }
    }

    pub fn has_problems(&self) -> bool {
        !self.math_failed.is_empty() || !self.warnings.is_empty()
    }

    pub fn print(&self, dry_run: bool) {
        let verb = if dry_run {
            "would convert"
        } else {
            "converted"
        };
        println!("{}", self.input.display());
        println!(
            "  math: {}/{} {}  ({} failed)",
            self.math_ok,
            self.math_total,
            verb,
            self.math_failed.len()
        );
        println!(
            "  tikz: {} block(s), {} -> CeTZ, {} -> SVG",
            self.tikz_total,
            self.cetz_total,
            self.svgs.len()
        );
        if let Some(out) = &self.output {
            let label = if dry_run { "would write" } else { "wrote" };
            println!("  {label}: {}", out.display());
        }
        for path in &self.svgs {
            let label = if dry_run { "would write" } else { "asset" };
            println!("    - {label}: {}", path.display());
        }
        for (src, err) in &self.math_failed {
            println!("  math error: {err}");
            println!("    source: {src}");
        }
        for warning in &self.warnings {
            println!("  warning: {warning}");
        }
        for note in &self.notes {
            println!("  note: {note}");
        }
    }
}

/// Aggregate totals for a run.
#[derive(Debug, Default)]
pub struct Summary {
    pub files: usize,
    pub math_total: usize,
    pub math_ok: usize,
    pub math_failed: usize,
    pub tikz_total: usize,
    pub cetz: usize,
    pub svgs: usize,
    pub warnings: usize,
}

impl Summary {
    pub fn from_reports(reports: &[FileReport]) -> Self {
        let mut s = Self {
            files: reports.len(),
            ..Default::default()
        };
        for r in reports {
            s.math_total += r.math_total;
            s.math_ok += r.math_ok;
            s.math_failed += r.math_failed.len();
            s.tikz_total += r.tikz_total;
            s.cetz += r.cetz_total;
            s.svgs += r.svgs.len();
            s.warnings += r.warnings.len();
        }
        s
    }

    pub fn print(&self, dry_run: bool) {
        println!();
        println!(
            "summary: {} file(s), math {}/{}, tikz blocks {} (CeTZ {}, SVG {}), warnings {}",
            self.files,
            self.math_ok,
            self.math_total,
            self.tikz_total,
            self.cetz,
            self.svgs,
            self.warnings
        );
        if self.math_failed > 0 {
            println!(
                "         {} math span(s) could not be converted",
                self.math_failed
            );
        }
        if dry_run {
            println!("         (dry run: no files written, no TeX invoked)");
        }
    }
}
