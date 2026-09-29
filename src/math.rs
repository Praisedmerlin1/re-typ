use tylax::latex_math_to_typst_with_diagnostics;

/// Result of converting one LaTeX math body.
pub struct MathOutput {
    /// Typst math body, without `$` delimiters.
    pub body: String,
    /// Non-fatal conversion warnings (unknown commands and the like).
    pub warnings: Vec<String>,
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
            warnings: Vec::new(),
        });
    }
    let result = latex_math_to_typst_with_diagnostics(trimmed);
    let output = result.output.trim().to_string();
    if output.is_empty() {
        return Err("converter produced empty output".to_string());
    }
    Ok(MathOutput {
        body: output,
        warnings: result.format_warnings(),
    })
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
