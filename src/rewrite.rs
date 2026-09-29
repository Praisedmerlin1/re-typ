use anyhow::Result;
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

use crate::context::{Ctx, TikzOutcome};
use crate::math;

struct OpenCode {
    start: usize,
    info: String,
    content: String,
}

/// Surgically rewrite a Markdown document in place: only math spans and TikZ
/// code blocks are changed. Everything else is preserved byte-for-byte.
pub fn convert(src: &str, ctx: &mut Ctx) -> Result<String> {
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    let mut code: Option<OpenCode> = None;

    for (event, range) in Parser::new_ext(src, crate::markdown_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let info = match &kind {
                    CodeBlockKind::Fenced(info) => info.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some(OpenCode {
                    start: range.start,
                    info,
                    content: String::new(),
                });
            }
            Event::Text(text) => {
                if let Some(open) = code.as_mut() {
                    open.content.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(open) = code.take()
                    && crate::tikz::is_tikz_block(&open.info, &open.content)
                {
                    match ctx.render_tikz(&open.content)? {
                        Some(TikzOutcome::Svg(path)) => {
                            let name = Ctx::asset_file_name(&path);
                            replacements.push((open.start, range.end, format!("![[{name}]]")));
                        }
                        Some(TikzOutcome::Cetz(code)) => {
                            let block = format!("```typst\n{code}\n```");
                            replacements.push((open.start, range.end, block));
                        }
                        // Rendering failed: leave the original fence untouched.
                        None => {}
                    }
                }
            }
            Event::InlineMath(body) => {
                ctx.report.math_total += 1;
                match math::convert(&body) {
                    Ok(out) => {
                        ctx.report.math_ok += 1;
                        ctx.note_math(&out.diagnostics);
                        replacements.push((range.start, range.end, math::wrap_inline(&out.body)));
                    }
                    Err(err) => ctx.note_math_failure(&body, &err)?,
                }
            }
            Event::DisplayMath(body) => {
                ctx.report.math_total += 1;
                match math::convert(&body) {
                    Ok(out) => {
                        ctx.report.math_ok += 1;
                        ctx.note_math(&out.diagnostics);
                        replacements.push((
                            range.start,
                            range.end,
                            math::wrap_display_dollar(&out.body),
                        ));
                    }
                    Err(err) => ctx.note_math_failure(&body, &err)?,
                }
            }
            _ => {}
        }
    }

    if replacements.is_empty() {
        return Ok(src.to_string());
    }

    replacements.sort_by_key(|(start, _, _)| *start);

    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for (start, end, text) in replacements {
        if start < cursor {
            // Overlapping spans should not happen; skip defensively.
            continue;
        }
        out.push_str(&src[cursor..start]);
        out.push_str(&text);
        cursor = end;
    }
    out.push_str(&src[cursor..]);
    Ok(out)
}
