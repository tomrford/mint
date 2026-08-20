use std::fmt;
use std::process::ExitCode;

use crate::source::{Source, Span};

/// Stable diagnostic category. Schema, data and encoding failures exit 1.
/// Usage failures exit 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Schema,
    Data,
    Encoding,
    Usage,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Data => "data",
            Self::Encoding => "encoding",
            Self::Usage => "usage",
        }
    }

    pub fn exit_code(self) -> u8 {
        match self {
            Self::Usage => 2,
            Self::Schema | Self::Data | Self::Encoding => 1,
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Related {
    pub source: String,
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub category: Category,
    pub source: String,
    pub span: Option<Span>,
    pub message: String,
    pub related: Vec<Related>,
    pub json_pointer: Option<String>,
}

impl Diagnostic {
    pub fn new(category: Category, source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category,
            source: source.into(),
            span: None,
            message: message.into(),
            related: Vec::new(),
            json_pointer: None,
        }
    }

    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn related(
        mut self,
        source: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        self.related.push(Related {
            source: source.into(),
            span,
            message: message.into(),
        });
        self
    }

    pub fn pointer(mut self, pointer: impl Into<String>) -> Self {
        self.json_pointer = Some(pointer.into());
        self
    }

    pub fn render(&self, files: &[&Source]) -> String {
        let mut out = format!("error[{}]: {}", self.category, self.message);
        if let Some(pointer) = &self.json_pointer {
            out.push_str(&format!(" ({pointer})"));
        }
        out.push('\n');
        if let Some(span) = self.span {
            render_span(&mut out, files, &self.source, span, "");
        }
        for related in &self.related {
            render_span(
                &mut out,
                files,
                &related.source,
                related.span,
                &format!("note: {}", related.message),
            );
        }
        out
    }
}

fn render_span(out: &mut String, files: &[&Source], name: &str, span: Span, note: &str) {
    let Some(source) = files.iter().find(|source| source.name == name) else {
        let (line, column) = (1, 1);
        out.push_str(&format!(" --> {name}:{line}:{column}\n"));
        if !note.is_empty() {
            out.push_str(&format!("  {note}\n"));
        }
        return;
    };
    let (line, column) = source.locate(span.start);
    out.push_str(&format!(" --> {name}:{line}:{column}\n"));
    let text = source.line_text(line);
    let width = line.to_string().len().max(1);
    out.push_str(&format!("{:>width$} |\n", ""));
    out.push_str(&format!("{line:>width$} | {text}\n"));
    let caret_start = usize::try_from(column.saturating_sub(1)).unwrap_or(0);
    let raw_len = span.end.saturating_sub(span.start).max(1);
    let caret_len = raw_len.min(text.len().saturating_sub(caret_start)).max(1);
    out.push_str(&format!(
        "{:>width$} | {pad}{caret}\n",
        "",
        pad = " ".repeat(caret_start),
        caret = "^".repeat(caret_len)
    ));
    if !note.is_empty() {
        out.push_str(&format!("  {note}\n"));
    }
}

#[derive(Clone, Debug)]
pub struct Error {
    pub diagnostics: Vec<Diagnostic>,
    pub sources: Vec<Source>,
}

impl Error {
    pub fn one(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
            sources: Vec::new(),
        }
    }

    pub fn with_source(mut self, source: Source) -> Self {
        if !self
            .sources
            .iter()
            .any(|existing| existing.name == source.name)
        {
            self.sources.push(source);
        }
        self
    }

    pub fn exit_code(&self) -> ExitCode {
        let code = self
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.category.exit_code())
            .max()
            .unwrap_or(1);
        ExitCode::from(code)
    }

    pub fn render(&self, files: &[&Source]) -> String {
        let owned: Vec<&Source> = self.sources.iter().collect();
        let files = if files.is_empty() {
            owned.as_slice()
        } else {
            files
        };
        self.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render(files))
            .collect::<Vec<_>>()
            .join("")
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for diagnostic in &self.diagnostics {
            writeln!(
                formatter,
                "error[{}]: {}",
                diagnostic.category, diagnostic.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<Diagnostic> for Error {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::one(diagnostic)
    }
}

#[cfg(test)]
mod tests {
    use super::{Category, Diagnostic};
    use crate::source::{Source, Span};

    #[test]
    fn renders_source_excerpt() {
        let source = Source::new("config.h", "typedef int x;\n");
        let diagnostic =
            Diagnostic::new(Category::Schema, "config.h", "bad type").at(Span::new(8, 11));
        let rendered = diagnostic.render(&[&source]);
        assert!(rendered.contains("error[schema]: bad type"));
        assert!(rendered.contains("config.h:1:9"));
        assert!(rendered.contains("typedef int x;"));
    }
}
