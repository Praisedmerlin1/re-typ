use tylax::latex_math_to_typst_with_diagnostics;

/// Result of converting one LaTeX math body.
pub struct MathOutput {
    /// Typst math body, without `$` delimiters.
    pub body: String,
    /// Non-fatal diagnostics from the converter (e.g. an empty-base
    /// superscript that was repaired automatically).
    pub diagnostics: Vec<String>,
}

/// Convert a LaTeX math body (no delimiters) into native Typst math source.
///
/// Uses TyLax, the same converter TypstMate uses, so the output is standalone
/// Typst with no package helpers required.
pub fn convert(body: &str) -> Result<MathOutput, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(MathOutput {
            body: String::new(),
            diagnostics: Vec::new(),
        });
    }

    // Guard against re-converting output that is already Typst. TyLax is not
    // idempotent: a second pass turns `beta` into `b e t a` and `frac(a, b)`
    // into `f r a c (a, b)`, so we leave already-converted math untouched.
    if looks_like_typst_math(trimmed) {
        return Ok(MathOutput {
            body: trimmed.to_string(),
            diagnostics: vec!["input already looks like Typst math; left unchanged".to_string()],
        });
    }

    let result = latex_math_to_typst_with_diagnostics(trimmed);
    let output = strip_math_delimiters(result.output.trim());
    if output.is_empty() {
        return Err("converter produced empty output".to_string());
    }
    let diagnostics = result
        .warnings
        .iter()
        .map(|warning| warning.message.trim().to_string())
        .collect();
    Ok(MathOutput {
        body: output,
        diagnostics,
    })
}

/// TyLax returns some top-level environments (`align`, `gather`, `equation`,
/// `align*`, ...) already wrapped in `$...$`, or as a `#math.equation(...)`
/// block. Peel that off so the caller can wrap it exactly once.
fn strip_math_delimiters(output: &str) -> String {
    let mut text = output.trim().to_string();

    if (text.starts_with("#math.") || text.starts_with("#block("))
        && let (Some(first), Some(last)) = (text.find('$'), text.rfind('$'))
        && first < last
    {
        text = text[first + 1..last].trim().to_string();
    }

    if text.starts_with('$') && text.ends_with('$') && text.len() >= 2 {
        text = text.trim_matches('$').trim().to_string();
    }

    text
}

/// Best-effort detection of math that is already in Typst syntax.
///
/// Only fires when the body contains no LaTeX commands (`\name`) but does
/// contain Typst-only tokens, so ordinary LaTeX is never skipped.
fn looks_like_typst_math(body: &str) -> bool {
    let has_latex_command = body.char_indices().any(|(i, ch)| {
        ch == '\\'
            && body[i + 1..]
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_alphabetic())
    });
    if has_latex_command {
        return false;
    }

    // Typst math does not use curly braces (it uses `()` and `[]`), so their
    // presence means this is still LaTeX (e.g. `P_{in}`) and must be converted.
    if body.contains('{') || body.contains('}') {
        return false;
    }

    // Bare alignment ampersands and backslash line breaks are Typst display
    // math syntax; in LaTeX they only appear inside a `\begin{...}` (braces).
    if body.contains('&') || body.contains("\\ ") || body.contains("\\\n") || body.ends_with('\\') {
        return true;
    }

    const TOKENS: &[&str] = &[
        "frac(",
        "sqrt(",
        "root(",
        "mat(",
        "cases(",
        "binom(",
        "op(",
        "attach(",
        "lr(",
        "vec(",
        "arrow(",
        "overline(",
        "underline(",
        "#text[",
        "#math.",
        "#h(",
        "\"\"",
        "thin",
        "dot",
        "slash",
        "oo",
        "->",
        "<=",
        ">=",
        "!=",
        "approx",
        "equiv",
        "diff",
    ];
    if TOKENS.iter().any(|token| body.contains(token)) {
        return true;
    }

    // Typst symbols that TyLax would otherwise split back into letters.
    const SYMBOLS: &[&str] = &[
        "alpha",
        "beta",
        "gamma",
        "delta",
        "epsilon",
        "varepsilon",
        "zeta",
        "eta",
        "theta",
        "vartheta",
        "iota",
        "kappa",
        "lambda",
        "mu",
        "nu",
        "xi",
        "omicron",
        "pi",
        "varpi",
        "rho",
        "varrho",
        "sigma",
        "varsigma",
        "tau",
        "upsilon",
        "phi",
        "varphi",
        "chi",
        "psi",
        "omega",
        "Gamma",
        "Delta",
        "Theta",
        "Lambda",
        "Xi",
        "Pi",
        "Sigma",
        "Upsilon",
        "Phi",
        "Psi",
        "Omega",
        "infinity",
        "degree",
        "arrow",
        "planck",
        "hbar",
        "ell",
        "nabla",
        "partial",
        "sum",
        "product",
        "integral",
        "union",
        "sect",
        "subset",
        "supset",
        "in",
        "notin",
        "forall",
        "exists",
        "angle",
        "triangle",
    ];
    body.split(|ch: char| !ch.is_ascii_alphabetic())
        .any(|word| SYMBOLS.contains(&word))
}

/// Wrap a converted body with inline math delimiters for Markdown/Typst.
pub fn wrap_inline(body: &str) -> String {
    format!("${body}$")
}

/// Wrap a converted body with display math delimiters (`$$...$$`).
pub fn wrap_display_dollar(body: &str) -> String {
    format!("$${body}$$")
}

/// Wrap a converted body with Typst display math delimiters (`$ ... $`).
pub fn wrap_display_typst(body: &str) -> String {
    format!("$ {body} $")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_environment_is_not_double_wrapped() {
        // TyLax returns `align` already wrapped in `$...$`; we must peel it off.
        let out = convert(r"\begin{align} a &= b \\ c &= d \end{align}").unwrap();
        assert_eq!(out.body, r"a & = b \ c & = d");
    }

    #[test]
    fn equation_star_block_is_unwrapped() {
        let out = convert(r"\begin{align*} a &= b \end{align*}").unwrap();
        assert_eq!(out.body, "a & = b");
    }

    #[test]
    fn plain_latex_still_converts() {
        assert_eq!(convert(r"\beta").unwrap().body, "beta");
        assert_eq!(convert(r"\frac{a}{b}").unwrap().body, "a/b");
        // Braces mean it is still LaTeX, even though `in` is a Typst symbol.
        assert_eq!(convert(r"P_{in}").unwrap().body, "P_(i n)");
    }

    #[test]
    fn already_typst_math_is_left_alone() {
        for body in ["beta", "a & = b \\ c & = d", "frac(a, b)", r#"""(3)"#] {
            assert_eq!(convert(body).unwrap().body, body, "body was: {body}");
        }
    }
}
