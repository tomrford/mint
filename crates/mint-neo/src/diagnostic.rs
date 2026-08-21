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

    pub fn render(&self, file: Option<&Source>) -> String {
        let mut out = format!("error[{}]: {}", self.category, self.message);
        if let Some(pointer) = &self.json_pointer {
            out.push_str(&format!(" ({pointer})"));
        }
        out.push('\n');
        if let Some(span) = self.span {
            render_span(&mut out, file, &self.source, span, "");
        }
        for related in &self.related {
            render_span(
                &mut out,
                file,
                &self.source,
                related.span,
                &format!("note: {}", related.message),
            );
        }
        out
    }
}

fn render_span(out: &mut String, file: Option<&Source>, name: &str, span: Span, note: &str) {
    let Some(source) = file else {
        out.push_str(&format!(" --> {name}:1:1\n"));
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

/// One located failure. `diagnostic` is boxed so `Result<T, Error>` stays small.
/// Header or JSON text lives on `source` when the constructor had a buffer.
#[derive(Clone, Debug)]
pub struct Error {
    pub diagnostic: Box<Diagnostic>,
    source: Option<Source>,
}

impl Error {
    pub fn named(
        category: Category,
        source: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            diagnostic: Box::new(Diagnostic::new(category, source, message)),
            source: None,
        }
    }

    pub fn at(category: Category, source: &Source, span: Span, message: impl Into<String>) -> Self {
        Self {
            diagnostic: Box::new(Diagnostic {
                category,
                source: source.name.clone(),
                span: Some(span),
                message: message.into(),
                related: Vec::new(),
                json_pointer: None,
            }),
            source: Some(source.clone()),
        }
    }

    pub fn schema(source: &Source, span: Span, message: impl Into<String>) -> Self {
        Self::at(Category::Schema, source, span, message)
    }

    pub fn data(source: &Source, span: Span, pointer: &str, message: impl Into<String>) -> Self {
        let error = Self::at(Category::Data, source, span, message);
        if pointer.is_empty() {
            error
        } else {
            error.pointer(pointer)
        }
    }

    pub fn span(mut self, span: Span) -> Self {
        self.diagnostic.span = Some(span);
        self
    }

    pub fn related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.diagnostic.related.push(Related {
            span,
            message: message.into(),
        });
        self
    }

    pub fn pointer(mut self, pointer: impl Into<String>) -> Self {
        self.diagnostic.json_pointer = Some(pointer.into());
        self
    }

    pub fn exit_code(&self) -> ExitCode {
        ExitCode::from(self.diagnostic.category.exit_code())
    }

    pub fn render(&self) -> String {
        self.diagnostic.render(self.source.as_ref())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.render())
    }
}

impl std::error::Error for Error {}
