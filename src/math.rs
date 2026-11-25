use tylax::latex_math_to_typst_with_diagnostics;
use tylax::maps::TYPST_TO_TEX;

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
/// TyLax is not idempotent: feeding it Typst such as `emptyset`, `A inter B`
/// or `cos theta` turns the identifiers into `e m p t y s e t`,
/// `A i n t e r B` and `c o s theta`. We therefore leave a body alone when it
/// contains a known Typst symbol/function or a Typst set literal.
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

    // `\\` is a line break in LaTeX, but a single `\` followed by whitespace
    // is a line break in Typst.
    if body.contains("\\\\") || body.contains("\\{") || body.contains("\\}") {
        return false;
    }
    if body.contains("\\ ") || body.contains("\\\n") || body.ends_with('\\') {
        return true;
    }

    // Quoted text is Typst syntax (`v_"des"`); LaTeX would use `\text{...}`.
    if body.contains('"') {
        return true;
    }

    // `^3` / `_{D}`: LaTeX permits an empty base; TyLax repairs it to `""^3`.
    let trimmed = body.trim_start();
    if trimmed.starts_with('^') || trimmed.starts_with('_') {
        return false;
    }

    // `x^{2}` / `P_{in}`: LaTeX script grouping. If the rest of the body is
    // already Typst (`phi_{n_x n_y n_z} = sin(...)`), converting would space
    // out `sin`, `frac`, ... so leave the mixed body untouched instead.
    if has_script_group(body) {
        let outside = without_script_groups(body);
        return contains_typst_symbol(&outside);
    }

    // Typst attach syntax uses parentheses: `x^(2)`, `A_(i)`.
    if body.contains("^(") || body.contains("_(") {
        return true;
    }

    // Bare names from Typst's math vocabulary (`emptyset`, `inter`, `cos`,
    // `delta`, ...) and function calls such as `frac(` or `floor(`.
    if contains_typst_symbol(body) {
        return true;
    }

    // Bare `{}` set literals (LaTeX would escape them as `\{`/`\}`).
    if has_typst_set_literal(body) {
        return true;
    }

    // `|...|` is valid in both languages and TyLax would only add spaces.
    if body.contains('|') {
        return true;
    }

    // A bare multi-letter word that is not a known Typst name is almost
    // certainly concatenated LaTeX variables (`mc^2`, `abc`), which Typst
    // would render as one identifier.
    if body
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|word| word.len() >= 2)
    {
        return false;
    }

    // Plain math with single-letter identifiers (`a^2`, `t_0`) renders the
    // same in both languages; leave it byte-for-byte unchanged.
    true
}

/// True when a `^` or `_` is followed by a `{` group, possibly after spaces.
fn has_script_group(body: &str) -> bool {
    body.char_indices().any(|(index, ch)| {
        (ch == '^' || ch == '_') && body[index + ch.len_utf8()..].trim_start().starts_with('{')
    })
}

/// Remove `^{...}` / `_{...}` groups so LaTeX-only script contents do not
/// count as Typst identifiers.
fn without_script_groups(body: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '^' || chars[index] == '_' {
            let mut next = index + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            if next < chars.len() && chars[next] == '{' {
                let mut depth = 0usize;
                while next < chars.len() {
                    match chars[next] {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                next += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    next += 1;
                }
                // Keep words on either side separated.
                out.push(' ');
                index = next;
                continue;
            }
        }
        out.push(chars[index]);
        index += 1;
    }

    out
}

/// True when the body contains a known Typst math identifier or function.
fn contains_typst_symbol(body: &str) -> bool {
    // Function-style names that are not in the symbol map.
    const FUNCTIONS: &[&str] = &[
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
    ];
    if FUNCTIONS.iter().any(|token| body.contains(token)) {
        return true;
    }

    // Common Typst names missing from TyLax's symbol map.
    const EXTRA_NAMES: &[&str] = &[
        "abs",
        "accent",
        "arrow",
        "bb",
        "bold",
        "cal",
        "cancel",
        "ceil",
        "circlearrowright",
        "display",
        "eval",
        "floor",
        "frak",
        "fract",
        "highlight",
        "inline",
        "italic",
        "limits",
        "med",
        "mod",
        "mono",
        "norm",
        "nothing",
        "overbrace",
        "primes",
        "quad",
        "quo",
        "rem",
        "round",
        "sans",
        "script",
        "serif",
        "slash",
        "sscript",
        "subsetneq",
        "supsetneq",
        "thick",
        "thin",
        "trunc",
        "upgamma",
        "uplambda",
        "upright",
        "wide",
    ];
    if body
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|word| EXTRA_NAMES.contains(&word))
    {
        return true;
    }

    // Unknown `name(` forms are still Typst function calls (`floor(x)`).
    if has_function_call(body) {
        return true;
    }

    TYPST_TO_TEX.keys().any(|key| {
        if key.chars().all(|c| c.is_ascii_alphanumeric()) {
            key.len() >= 2
                && body
                    .split(|c: char| !c.is_ascii_alphabetic())
                    .any(|word| word == *key)
        } else {
            body.contains(*key)
        }
    })
}

/// True when a multi-letter identifier is immediately followed by `(`.
fn has_function_call(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphabetic() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
                index += 1;
            }
            if index - start >= 2 && bytes.get(index) == Some(&b'(') {
                return true;
            }
        } else {
            index += 1;
        }
    }
    false
}

/// True when the body contains bare braces. LaTeX would escape literal braces
/// as `\{`/`\}`; script grouping is handled earlier, so any remaining braces
/// are Typst set literals.
fn has_typst_set_literal(body: &str) -> bool {
    body.contains('{') || body.contains('}')
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
        // Braces used for scripting mean it is still LaTeX, even though `in`
        // is a Typst symbol.
        assert_eq!(convert(r"P_{in}").unwrap().body, "P_(i n)");
        assert_eq!(convert(r"x^{2}").unwrap().body, "x^(2)");
        // Concatenated variables are LaTeX and need spacing.
        assert_eq!(convert("E = mc^2").unwrap().body, "E = m c^(2)");
    }

    #[test]
    fn already_typst_math_is_left_alone() {
        for body in [
            "beta",
            "a & = b \\ c & = d",
            "frac(a, b)",
            r#"""(3)"#,
            "emptyset",
            "A times B",
            "A inter B = {x ; x in A and x in B }",
            "cos theta + sin theta",
            "|a| = 1",
            "2^(n)",
            "A_(i)",
            "t_0",
            "x^2 + y^2 = r^2",
            "E = m c^2",
            "floor(x)",
        ] {
            assert_eq!(convert(body).unwrap().body, body, "body was: {body}");
        }
    }

    #[test]
    fn latex_line_breaks_and_escaped_braces_still_convert() {
        assert_eq!(convert(r"a \\ b").unwrap().body, r"a \ b");
        assert_eq!(convert(r"\{1, 2\}").unwrap().body, "{1, 2 }");
    }
}
