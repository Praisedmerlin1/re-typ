use anyhow::Result;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

use crate::context::{Ctx, TikzOutcome};
use crate::math;

struct OpenCode {
    info: String,
    content: String,
}

struct TableState {
    columns: usize,
    in_head: bool,
    header: Option<Vec<String>>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_cell: bool,
}

impl TableState {
    fn new(columns: usize) -> Self {
        Self {
            columns,
            in_head: false,
            header: None,
            rows: Vec::new(),
            current_row: Vec::new(),
            current_cell: String::new(),
            in_cell: false,
        }
    }

    fn push(&mut self, s: &str) {
        if self.in_cell {
            self.current_cell.push_str(s);
        }
    }

    fn start_row(&mut self) {
        self.current_row.clear();
    }

    fn end_row(&mut self) {
        let row = std::mem::take(&mut self.current_row);
        if self.in_head {
            self.header = Some(row);
        } else {
            self.rows.push(row);
        }
    }

    /// Flush a header that arrived as bare cells (no `TableRow` wrapper).
    fn flush_head(&mut self) {
        if self.in_head && !self.current_row.is_empty() {
            self.header = Some(std::mem::take(&mut self.current_row));
        }
        self.in_head = false;
    }

    fn start_cell(&mut self) {
        self.current_cell.clear();
        self.in_cell = true;
    }

    fn end_cell(&mut self) {
        self.in_cell = false;
        let cell = std::mem::take(&mut self.current_cell);
        self.current_row.push(cell);
    }
}

struct Writer<'a, 'b> {
    ctx: &'a mut Ctx<'b>,
    out: String,
    list_stack: Vec<bool>,
    in_image: bool,
    in_metadata: bool,
    metadata_text: String,
    html_warned: bool,
    code: Option<OpenCode>,
    table: Option<TableState>,
}

impl<'a, 'b> Writer<'a, 'b> {
    fn new(ctx: &'a mut Ctx<'b>) -> Self {
        Self {
            ctx,
            out: String::new(),
            list_stack: Vec::new(),
            in_image: false,
            in_metadata: false,
            metadata_text: String::new(),
            html_warned: false,
            code: None,
            table: None,
        }
    }

    fn finish(self) -> String {
        self.out
    }

    fn emit(&mut self, s: &str) {
        if let Some(table) = self.table.as_mut() {
            table.push(s);
        } else {
            self.out.push_str(s);
        }
    }

    /// Ensure the block buffer ends with a single newline.
    fn newline(&mut self) {
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    /// Ensure the block buffer ends with a blank line.
    fn blank_line(&mut self) {
        self.newline();
        if !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    /// Drop trailing whitespace, used before closing a bracket construct.
    fn trim_trailing_newlines(&mut self) {
        while self.out.ends_with('\n') {
            self.out.pop();
        }
    }

    fn event(&mut self, event: Event<'_>) -> Result<()> {
        if self.in_image {
            if matches!(event, Event::End(TagEnd::Image)) {
                self.in_image = false;
            }
            return Ok(());
        }
        if self.code.is_some() {
            return self.code_event(event);
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if self.in_metadata {
                    self.metadata_text.push_str(&text);
                } else {
                    let escaped = escape_text(&text);
                    self.emit(&escaped);
                }
                Ok(())
            }
            Event::Code(code) => {
                self.emit(&format!("`{code}`"));
                Ok(())
            }
            Event::InlineMath(body) => self.math(&body, false),
            Event::DisplayMath(body) => self.math(&body, true),
            Event::SoftBreak => {
                self.emit("\n");
                Ok(())
            }
            Event::HardBreak => {
                self.emit("\\\n");
                Ok(())
            }
            Event::Rule => {
                self.blank_line();
                self.out.push_str("#line(length: 100%)\n\n");
                Ok(())
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                if !self.html_warned {
                    self.html_warned = true;
                    self.ctx.warn("raw HTML dropped during Typst conversion")?;
                }
                Ok(())
            }
            Event::FootnoteReference(label) => {
                self.ctx.warn(format!(
                    "footnote reference `{label}` emitted without content"
                ))?;
                self.emit(&format!("#footnote[{label}]"));
                Ok(())
            }
            Event::TaskListMarker(checked) => {
                self.emit(if checked { "\u{2611} " } else { "\u{2610} " });
                Ok(())
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) -> Result<()> {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.blank_line();
                self.out.push_str(&"=".repeat(level_num(level)));
                self.out.push(' ');
            }
            Tag::BlockQuote(_) => self.out.push_str("#quote(block: true)[\n"),
            Tag::CodeBlock(kind) => {
                let info = match kind {
                    CodeBlockKind::Fenced(info) => info.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some(OpenCode {
                    info,
                    content: String::new(),
                });
            }
            Tag::List(start) => {
                if !self.list_stack.is_empty() {
                    self.newline();
                }
                self.list_stack.push(start.is_some());
            }
            Tag::Item => {
                let depth = self.list_stack.len();
                let indent = "  ".repeat(depth.saturating_sub(1));
                let ordered = self.list_stack.last().copied().unwrap_or(false);
                let marker = if ordered { "+" } else { "-" };
                self.out.push_str(&format!("{indent}{marker} "));
            }
            Tag::Emphasis => self.emit("_"),
            Tag::Strong => self.emit("*"),
            Tag::Strikethrough => self.emit("#strike["),
            Tag::Superscript => self.emit("#super["),
            Tag::Subscript => self.emit("#sub["),
            Tag::Link { dest_url, .. } => {
                self.emit(&format!("#link(\"{}\")[", escape_string(&dest_url)));
            }
            Tag::Image { dest_url, .. } => {
                self.emit(&format!("#image(\"{}\")", escape_string(&dest_url)));
                self.in_image = true;
            }
            Tag::Table(alignments) => {
                self.table = Some(TableState::new(alignments.len()));
            }
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.in_head = true;
                }
            }
            Tag::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.start_row();
                }
            }
            Tag::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    table.start_cell();
                }
            }
            Tag::FootnoteDefinition(label) => {
                self.ctx
                    .warn(format!("footnote definition `{label}` translated loosely"))?;
                self.out.push_str(&format!("#footnote[{label}: "));
            }
            Tag::HtmlBlock => {}
            Tag::MetadataBlock(_) => {
                self.in_metadata = true;
                self.metadata_text.clear();
            }
            Tag::DefinitionList => {
                self.ctx.warn("definition list translated loosely")?;
            }
            Tag::DefinitionListTitle => self.emit("*"),
            Tag::DefinitionListDefinition => self.emit(": "),
        }
        Ok(())
    }

    fn end(&mut self, tag: TagEnd) -> Result<()> {
        match tag {
            TagEnd::Paragraph => self.blank_line(),
            TagEnd::Heading(_) => self.blank_line(),
            TagEnd::BlockQuote(_) => {
                self.trim_trailing_newlines();
                self.out.push_str("]\n\n");
            }
            TagEnd::CodeBlock => {}
            TagEnd::List(_) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.out.push('\n');
                }
            }
            TagEnd::Item => self.newline(),
            TagEnd::Emphasis => self.emit("_"),
            TagEnd::Strong => self.emit("*"),
            TagEnd::Strikethrough | TagEnd::Superscript | TagEnd::Subscript => self.emit("]"),
            TagEnd::Link => self.emit("]"),
            TagEnd::Image => self.in_image = false,
            TagEnd::Table => self.finish_table(),
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.flush_head();
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.end_row();
                }
            }
            TagEnd::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    table.end_cell();
                }
            }
            TagEnd::FootnoteDefinition => self.out.push_str("]\n\n"),
            TagEnd::HtmlBlock => {}
            TagEnd::MetadataBlock(_) => {
                self.in_metadata = false;
                let text = std::mem::take(&mut self.metadata_text);
                for line in text.lines() {
                    self.out.push_str("// ");
                    self.out.push_str(line);
                    self.out.push('\n');
                }
                self.out.push('\n');
            }
            TagEnd::DefinitionList => {}
            TagEnd::DefinitionListTitle => self.emit("*: "),
            TagEnd::DefinitionListDefinition => self.out.push('\n'),
        }
        Ok(())
    }

    fn code_event(&mut self, event: Event<'_>) -> Result<()> {
        match event {
            Event::Text(text) => {
                if let Some(code) = self.code.as_mut() {
                    code.content.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(code) = self.code.take() {
                    if crate::tikz::is_tikz_block(&code.info, &code.content) {
                        match self.ctx.render_tikz(&code.content)? {
                            Some(TikzOutcome::Svg(path)) => {
                                let name = Ctx::asset_file_name(&path);
                                self.blank_line();
                                self.out.push_str(&format!("#image(\"{name}\")\n\n"));
                            }
                            Some(TikzOutcome::Cetz(cetz)) => {
                                self.blank_line();
                                self.out.push_str(&cetz);
                                self.out.push_str("\n\n");
                            }
                            None => {
                                // Keep the source as a raw block so the document
                                // still compiles, and let the warning surface it.
                                let lang = code.info.split_whitespace().next().unwrap_or("tex");
                                self.blank_line();
                                self.out.push_str("```");
                                self.out.push_str(lang);
                                self.out.push('\n');
                                self.out.push_str(code.content.trim_end());
                                self.out.push_str("\n```\n\n");
                            }
                        }
                    } else {
                        let lang = code.info.split_whitespace().next().unwrap_or("");
                        self.blank_line();
                        self.out.push_str("```");
                        self.out.push_str(lang);
                        self.out.push('\n');
                        self.out.push_str(&code.content);
                        if !code.content.ends_with('\n') {
                            self.out.push('\n');
                        }
                        self.out.push_str("```\n\n");
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn math(&mut self, body: &str, display: bool) -> Result<()> {
        self.ctx.report.math_total += 1;
        let wrapped = match math::convert(body) {
            Ok(out) => {
                self.ctx.report.math_ok += 1;
                self.ctx.note_math(&out.diagnostics);
                if display {
                    math::wrap_display_typst(&out.body)
                } else {
                    math::wrap_inline(&out.body)
                }
            }
            Err(err) => {
                self.ctx.note_math_failure(body, &err)?;
                if display {
                    format!("$${body}$$")
                } else {
                    format!("${body}$")
                }
            }
        };
        if display && self.table.is_none() {
            self.blank_line();
            self.out.push_str(&wrapped);
            self.out.push_str("\n\n");
        } else {
            self.emit(&wrapped);
        }
        Ok(())
    }

    fn finish_table(&mut self) {
        if let Some(table) = self.table.take() {
            let mut s = String::from("#table(\n");
            s.push_str(&format!("  columns: {},\n", table.columns));
            if let Some(header) = &table.header {
                s.push_str("  table.header(");
                for cell in header {
                    s.push_str(&format!("[{cell}], "));
                }
                s.push_str("),\n");
            }
            for row in &table.rows {
                s.push_str("  ");
                for cell in row {
                    s.push_str(&format!("[{cell}], "));
                }
                s.push('\n');
            }
            s.push_str(")\n\n");
            self.out.push_str(&s);
        }
    }
}

fn level_num(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '~' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Full Markdown -> Typst document conversion.
pub fn convert(src: &str, ctx: &mut Ctx) -> Result<String> {
    let mut writer = Writer::new(ctx);
    for (event, _range) in Parser::new_ext(src, crate::markdown_options()).into_offset_iter() {
        writer.event(event)?;
    }
    Ok(writer.finish())
}
