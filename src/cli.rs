use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Output flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum Format {
    /// Obsidian-friendly Markdown: only math and TikZ spans are rewritten.
    #[default]
    Md,
    /// Typst document: Markdown structure is translated, math/TikZ converted.
    Typ,
}

/// PDF -> SVG conversion backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum SvgBackend {
    /// Pick the first available backend.
    #[default]
    Auto,
    Dvisvgm,
    Pdftocairo,
    Pdf2svg,
    Inkscape,
}

/// TeX engine used to compile TikZ snippets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TexEngine {
    /// Pick the first engine that can compile a trivial document.
    #[default]
    Auto,
    Pdflatex,
    Xelatex,
    Lualatex,
    Tectonic,
}

/// How TikZ blocks are turned into Typst output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TikzBackend {
    /// CeTZ when the block is supported, otherwise an SVG image.
    #[default]
    Auto,
    /// Always transpile to CeTZ (falls back to SVG if the result is empty).
    Cetz,
    /// Always render to an SVG image (requires a TeX toolchain).
    Svg,
}

#[derive(Debug, Parser)]
#[command(
    name = "re-typ",
    version,
    about = "Rewrite LaTeX math as native Typst math and TikZ as CeTZ or SVG, preserving everything else.",
    long_about = "re-typ rewrites Markdown files surgically: `$...$`/`$$...$$` LaTeX math becomes \
native Typst math and `tikz`/`latex` fenced blocks become CeTZ code or SVG images. \
The default output is Markdown, which is what Obsidian + TypstMate consume. With \
`--format typ` a full Markdown -> Typst document conversion is performed instead."
)]
pub struct Cli {
    /// Input `.md` files or directories to search recursively.
    #[arg(value_name = "INPUT", required_unless_present = "check_deps")]
    pub inputs: Vec<PathBuf>,

    /// Output flavor.
    #[arg(long, value_enum, default_value_t = Format::Md)]
    pub format: Format,

    /// Target directory for generated files and SVG assets.
    #[arg(short, long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Suffix inserted before the extension for sibling Markdown output.
    #[arg(long, default_value = ".typst", value_name = "SUFFIX")]
    pub suffix: String,

    /// Overwrite the input file in place (Markdown output only).
    #[arg(long)]
    pub in_place: bool,

    /// Rebuild SVG assets even if they already exist.
    #[arg(long)]
    pub force: bool,

    /// Process files that already contain `typst`/`cetz` code blocks.
    /// By default such files are skipped as already migrated.
    #[arg(long = "no-skip-typst-files")]
    pub no_skip_typst_files: bool,

    /// Treat warnings (failed math, unsupported constructs) as fatal.
    #[arg(long)]
    pub strict: bool,

    /// Print per-file details, including non-fatal math notes.
    #[arg(short, long)]
    pub verbose: bool,

    /// Parse and report, without writing files or invoking TeX.
    #[arg(long)]
    pub dry_run: bool,

    /// Check external toolchain dependencies and exit.
    #[arg(long)]
    pub check_deps: bool,

    /// PDF -> SVG backend to use.
    #[arg(long, value_enum, default_value_t = SvgBackend::Auto)]
    pub svg_backend: SvgBackend,

    /// TeX engine to use for TikZ compilation.
    #[arg(long, value_enum, default_value_t = TexEngine::Auto)]
    pub tex_engine: TexEngine,

    /// How to render TikZ blocks: CeTZ code or an SVG image.
    #[arg(long, value_enum, default_value_t = TikzBackend::Auto)]
    pub tikz_backend: TikzBackend,

    /// Extra LaTeX preamble injected into the generated TikZ standalone document.
    #[arg(long, value_name = "FILE")]
    pub tikz_preamble: Option<PathBuf>,
}
