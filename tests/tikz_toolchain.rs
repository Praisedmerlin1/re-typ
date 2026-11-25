use re_typ::cli::{SvgBackend, TexEngine};
use re_typ::deps;
use re_typ::tikz;

/// End-to-end TikZ -> SVG. Requires a working TeX engine and SVG backend,
/// so it is ignored by default:
///
/// ```text
/// cargo test --test tikz_toolchain -- --ignored --nocapture
/// ```
#[test]
#[ignore = "requires a working TeX toolchain and PDF -> SVG backend"]
fn compiles_tikz_to_svg() {
    let tools = deps::detect(SvgBackend::Auto, TexEngine::Auto);
    let (tex, svg) = tools
        .require()
        .unwrap_or_else(|e| panic!("toolchain unavailable: {e}"));

    let source = "\\begin{tikzpicture}\\draw (0,0) circle (1cm);\\end{tikzpicture}";
    let bytes = tikz::compile(source, &tools, None).expect("tikz compile failed");

    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("<svg"),
        "not an SVG: {}",
        &text[..200.min(text.len())]
    );
    assert!(
        text.contains("data-retyp-background"),
        "SVG has no background: {}",
        &text[..200.min(text.len())]
    );
    eprintln!(
        "used {} + {} -> {} bytes SVG",
        tex.name,
        svg.kind.name(),
        bytes.len()
    );
}
