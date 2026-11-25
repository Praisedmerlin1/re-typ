use std::path::Path;

use re_typ::cli::{Format, TikzBackend};
use re_typ::config::Config;
use re_typ::deps::Tools;
use re_typ::report::FileReport;
use re_typ::{convert_source, tikz};

fn convert(src: &str, format: Format) -> (String, FileReport) {
    let cfg = Config {
        format,
        ..Default::default()
    };
    convert_source(src, Path::new("doc.md"), &cfg, &Tools::default(), None)
        .expect("conversion failed")
}

#[test]
fn md_mode_only_touches_math() {
    let src = include_str!("fixtures/math.md");
    let (out, report) = convert(src, Format::Md);

    assert_eq!(report.math_total, 3);
    assert_eq!(report.math_ok, 3);
    assert!(report.math_failed.is_empty());

    // Math becomes native Typst. `a^2` is already valid Typst and is left
    // byte-for-byte unchanged; `mc^2` needs spacing between the variables.
    assert!(out.contains("$E = m c^(2)$"), "{out}");
    assert!(out.contains("$a^2 + b^2 = c^2$"), "{out}");
    assert!(out.contains("$$frac(sqrt(pi), 2)$$"), "{out}");

    // Everything else is preserved verbatim.
    assert!(out.contains("# Title"));
    assert!(out.contains("Text with **bold**, *italic*, and `code`."));
    assert!(out.contains("- alpha\n- beta"));
    assert!(out.contains("> quote"));
    assert!(out.contains("| A | B |"));

    // No Typst markup translation leaked into Markdown mode.
    assert!(!out.contains("= Title"));
    assert!(!out.contains("#table("));
}

#[test]
fn typ_mode_translates_structure_and_math() {
    let src = include_str!("fixtures/math.md");
    let (out, report) = convert(src, Format::Typ);

    assert_eq!(report.math_ok, 3);
    assert!(out.contains("= Title"), "{out}");
    assert!(out.contains("*bold*"));
    assert!(out.contains("_italic_"));
    assert!(out.contains("#quote(block: true)["));
    assert!(out.contains("#table("));
    assert!(out.contains("table.header([A], [B]"));
    assert!(out.contains("$E = m c^(2)$"));
    assert!(out.contains("$ frac(sqrt(pi), 2) $"));
}

#[test]
fn dry_run_reports_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("diagram.md");
    let src = include_str!("fixtures/tikz.md");
    // Force SVG so the dry run predicts an asset instead of CeTZ.
    let cfg = Config {
        dry_run: true,
        tikz_backend: TikzBackend::Svg,
        ..Default::default()
    };

    let (_, report) = convert_source(src, &input, &cfg, &Tools::default(), None).unwrap();

    assert_eq!(report.tikz_total, 1);
    assert_eq!(report.cetz_total, 0);
    assert_eq!(report.svgs.len(), 1);
    assert_eq!(report.svgs[0].file_name().unwrap(), "diagram-1.svg");
    // Nothing is written in dry-run mode.
    assert!(!dir.path().join("diagram-1.svg").exists());
    assert!(!dir.path().join("diagram.typst.md").exists());
}

#[test]
fn auto_backend_transpiles_basic_tikz_to_cetz_without_tex() {
    // No toolchain is available (Tools::default), so this only passes if the
    // auto backend selects CeTZ and never reaches the TeX compiler.
    let src = include_str!("fixtures/tikz.md");
    let (out, report) = convert(src, Format::Md);

    assert_eq!(report.tikz_total, 1);
    assert_eq!(report.cetz_total, 1);
    assert!(report.svgs.is_empty());
    assert!(out.contains("```typst"), "{out}");
    assert!(out.contains("#canvas("), "{out}");
    assert!(!out.contains("![["));
}

#[test]
fn cetz_renders_inline_in_typ_mode() {
    let src = include_str!("fixtures/tikz.md");
    let (out, report) = convert(src, Format::Typ);
    assert_eq!(report.cetz_total, 1);
    assert!(out.contains("#import \"@preview/cetz"), "{out}");
    assert!(out.contains("#canvas("), "{out}");
    assert!(!out.contains("#image("));
}

#[test]
fn unsupported_constructs_fall_back_to_svg() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("circuit.md");
    let src =
        "# C\n\n```tikz\n\\begin{circuitikz}\n\\draw (0,0) to[R] (2,0);\n\\end{circuitikz}\n```\n";
    let cfg = Config {
        dry_run: true,
        ..Default::default()
    };

    let (_, report) = convert_source(src, &input, &cfg, &Tools::default(), None).unwrap();

    assert_eq!(report.tikz_total, 1);
    assert_eq!(report.cetz_total, 0);
    assert_eq!(report.svgs.len(), 1);
}

#[test]
fn spaced_circuitikz_blocks_are_detected() {
    // Snippets pasted from notes often put a space after control words:
    // `\usepackage {circuitikz}` / `\begin {circuitikz}`. These are valid
    // LaTeX and must be picked up like their unspaced forms.
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("circuit.md");
    let src = "# C\n\n```tikz\n\\usepackage {circuitikz}\n\\begin {document}\n\
               \\begin {circuitikz}\n\\draw (0,0) to[R] (2,0);\n\
               \\end {circuitikz}\n\\end {document}\n```\n";
    let cfg = Config {
        dry_run: true,
        ..Default::default()
    };

    let (_, report) = convert_source(src, &input, &cfg, &Tools::default(), None).unwrap();

    assert_eq!(report.tikz_total, 1);
    assert_eq!(report.cetz_total, 0);
    assert_eq!(report.svgs.len(), 1);
}

#[test]
fn cetz_support_detection() {
    assert!(!tikz::looks_unsupported_for_cetz(
        "\\begin{tikzpicture}\\draw (0,0)--(1,1);\\end{tikzpicture}"
    ));
    assert!(tikz::looks_unsupported_for_cetz(
        "\\begin{circuitikz}\\draw (0,0) to[R] (2,0);\\end{circuitikz}"
    ));
    assert!(tikz::looks_unsupported_for_cetz(
        "\\begin{axis}\\addplot {x^2};\\end{axis}"
    ));
    assert!(tikz::looks_unsupported_for_cetz(
        "\\begin{tikzcd} A \\arrow[r] & B \\end{tikzcd}"
    ));
    // Partially supported constructs must fall back to SVG.
    assert!(tikz::looks_unsupported_for_cetz(
        "\\begin{tikzpicture}\\foreach \\x in {-2,...,2} {\\draw (\\x,0)--(\\x,1);}\\end{tikzpicture}"
    ));
    assert!(tikz::looks_unsupported_for_cetz(
        "\\begin{tikzpicture}\\coordinate (A) at (0,0);\\draw (A)--(1,1);\\end{tikzpicture}"
    ));
    assert!(tikz::looks_unsupported_for_cetz(
        "\\begin{tikzpicture}\\definecolor{x}{RGB}{0,0,0}\\end{tikzpicture}"
    ));
    assert!(tikz::looks_unsupported_for_cetz(
        "\\begin{tikzpicture}\\begin{scope}[shift={(0,1)}]\\draw (0,0)--(1,1);\\end{scope}\\end{tikzpicture}"
    ));
}

#[test]
fn cetz_with_leftover_latex_labels_falls_back_to_svg() {
    // TyLax copies node math verbatim, so `\alpha` would survive into the
    // CeTZ output and fail to compile. The renderer must detect that and use
    // the SVG backend instead.
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("label.md");
    let src = "```tikz\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\
               \\node at (0,0) {$\\alpha$};\n\\end{tikzpicture}\n```\n";
    let cfg = Config {
        dry_run: true,
        ..Default::default()
    };

    let (_, report) = convert_source(src, &input, &cfg, &Tools::default(), None).unwrap();

    assert_eq!(report.tikz_total, 1);
    assert_eq!(report.cetz_total, 0);
    assert_eq!(report.svgs.len(), 1);
}

#[test]
fn coordinate_heavy_tikz_falls_back_to_svg() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("loop.md");
    let src = "```tikz\n\\begin{tikzpicture}\n\
               \\foreach \\x in {-2,...,2} {\\draw (\\x,0) -- (\\x,1);}\n\
               \\end{tikzpicture}\n```\n";
    let cfg = Config {
        dry_run: true,
        ..Default::default()
    };

    let (_, report) = convert_source(src, &input, &cfg, &Tools::default(), None).unwrap();

    assert_eq!(report.tikz_total, 1);
    assert_eq!(report.cetz_total, 0);
    assert_eq!(report.svgs.len(), 1);
}

#[test]
fn failed_render_leaves_block_unchanged() {
    // No toolchain is available here, so an SVG-forced block must fail without
    // aborting the whole document.
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("doc.md");
    let src =
        "# C\n\n```tikz\n\\begin{circuitikz}\n\\draw (0,0) to[R] (2,0);\n\\end{circuitikz}\n```\n";
    let cfg = Config {
        tikz_backend: TikzBackend::Svg,
        ..Default::default()
    };

    let (out, report) = convert_source(src, &input, &cfg, &Tools::default(), None).unwrap();

    assert_eq!(report.tikz_total, 1);
    assert_eq!(report.svgs.len(), 0);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("block left unchanged")),
        "{:?}",
        report.warnings
    );
    // Original fence survives.
    assert!(out.contains("```tikz"));
    assert!(!out.contains("![["));
}

#[test]
fn strict_makes_render_failure_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("doc.md");
    let src = "```tikz\n\\begin{circuitikz}\n\\draw (0,0) to[R] (2,0);\n\\end{circuitikz}\n```\n";
    let cfg = Config {
        tikz_backend: TikzBackend::Svg,
        strict: true,
        ..Default::default()
    };

    let result = convert_source(src, &input, &cfg, &Tools::default(), None);
    assert!(result.is_err());
}

#[test]
fn tikz_blocks_are_detected_by_language_and_content() {
    assert!(tikz::is_tikz_block(
        "tikz",
        "\\begin{tikzpicture}\\draw (0,0);\\end{tikzpicture}"
    ));
    assert!(tikz::is_tikz_block(
        "latex",
        "\\begin{tikzpicture}\\begin{axis}\\end{axis}\\end{tikzpicture}"
    ));
    assert!(tikz::is_tikz_block("tikz", "\\begin{axis}x\\end{axis}"));
    // Spaces between a control word and its braces are valid LaTeX.
    assert!(tikz::is_tikz_block(
        "tikz",
        "\\begin {tikzpicture}\\draw (0,0);\\end {tikzpicture}"
    ));
    assert!(tikz::is_tikz_block(
        "tikz",
        "\\begin {circuitikz}\\draw (0,0) to[R] (2,0);\\end {circuitikz}"
    ));
    // Wrong language.
    assert!(!tikz::is_tikz_block(
        "rust",
        "\\begin{tikzpicture}\\end{tikzpicture}"
    ));
    // No diagram.
    assert!(!tikz::is_tikz_block("tikz", "just text"));
}

#[test]
fn empty_base_scripts_are_repaired_and_deduplicated_as_notes() {
    // Common in units/physics notes: `cm$^3$`, `$_{D}$`. LaTeX allows an empty
    // base; TyLax repairs it to `""^(3)` and reports a non-fatal diagnostic.
    let src = "Units: cm$^3$ and $_{D}$ and 10$^{th}$ and $^{-3}$.";
    let (out, report) = convert(src, Format::Md);

    assert_eq!(report.math_ok, 4);
    assert!(report.math_failed.is_empty());
    // Not a warning: the conversion is valid.
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    // Deduplicated into a single note with a count.
    assert_eq!(report.math_notes.len(), 1, "{:?}", report.math_notes);
    assert_eq!(report.math_notes[0].1, 4);
    // Repaired to valid Typst.
    assert!(out.contains("\"\"^(3)"), "{out}");
    assert!(out.contains("\"\"_(D)"), "{out}");
}

#[test]
fn align_environment_is_not_double_wrapped() {
    let src = "Given\n\n$$\n\\begin{align}\na &= b \\\\\nc &= d\n\\end{align}\n$$\n";
    let (out, report) = convert(src, Format::Md);
    assert_eq!(report.math_ok, 1);
    assert!(!out.contains("$$$"), "double-wrapped dollars:\n{out}");
    assert!(out.contains("$$a & = b \\ c & = d$$"), "{out}");
}

#[test]
fn files_with_typst_blocks_are_skipped() {
    let src = "Text.\n\n```typst\n#canvas({ import draw: * })\n```\n\nMore $x$.\n";
    let (out, report) = convert(src, Format::Md);
    assert!(report.skipped);
    assert_eq!(out, src);
    assert_eq!(report.math_total, 0);
}

#[test]
fn skip_can_be_overridden() {
    let src = "Text.\n\n```typst\n#canvas({ import draw: * })\n```\n";
    let cfg = Config {
        skip_typst_files: false,
        ..Default::default()
    };
    let (_, report) =
        convert_source(src, Path::new("doc.md"), &cfg, &Tools::default(), None).unwrap();
    assert!(!report.skipped);
}

#[test]
fn conversion_is_idempotent() {
    // Running the tool on already-converted output must not mangle it.
    let once = "Given\n\n$$a & = b \\ c & = d$$\n\nUnits $beta$ and $frac(a, b)$.\n";
    let (twice, _) = convert(once, Format::Md);
    assert!(twice.contains("$$a & = b \\ c & = d$$"), "{twice}");
    assert!(twice.contains("$beta$"), "{twice}");
    assert!(twice.contains("$frac(a, b)$"), "{twice}");
    assert!(!twice.contains("b e t a"), "{twice}");
}

#[test]
fn malformed_math_does_not_panic() {
    // Malformed LaTeX should be handled gracefully rather than crashing.
    let src = "$\\frac{1}$";
    let (out, report) = convert(src, Format::Md);
    assert!(report.math_total >= 1);
    assert!(!out.is_empty());
}
