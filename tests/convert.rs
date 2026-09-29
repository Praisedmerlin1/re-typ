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

    // Math becomes native Typst.
    assert!(out.contains("$E = m c^(2)$"), "{out}");
    assert!(out.contains("$a^(2) + b^(2) = c^(2)$"), "{out}");
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
    // Wrong language.
    assert!(!tikz::is_tikz_block(
        "rust",
        "\\begin{tikzpicture}\\end{tikzpicture}"
    ));
    // No diagram.
    assert!(!tikz::is_tikz_block("tikz", "just text"));
}

#[test]
fn malformed_math_does_not_panic() {
    // Malformed LaTeX should be handled gracefully rather than crashing.
    let src = "$\\frac{1}$";
    let (out, report) = convert(src, Format::Md);
    assert!(report.math_total >= 1);
    assert!(!out.is_empty());
}
