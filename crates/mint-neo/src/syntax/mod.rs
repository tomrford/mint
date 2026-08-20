use tree_sitter::{Node, Parser, Tree};

use crate::diagnostic::{Category, Diagnostic, Error};
use crate::source::{Source, Span};

pub struct ParsedFile<'a> {
    pub source: &'a Source,
    pub tree: Tree,
}

impl<'a> ParsedFile<'a> {
    pub fn parse(source: &'a Source) -> Result<Self, Error> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .map_err(|error| {
                Error::one(Diagnostic::new(
                    Category::Schema,
                    &source.name,
                    format!("failed to load C grammar: {error}"),
                ))
            })?;
        let tree = parser.parse(&source.text, None).ok_or_else(|| {
            Error::one(Diagnostic::new(
                Category::Schema,
                &source.name,
                "C parser produced no syntax tree",
            ))
        })?;
        let parsed = Self { source, tree };
        parsed.reject_errors()?;
        parsed.reject_unsupported_directives()?;
        Ok(parsed)
    }

    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    pub fn span(node: Node<'_>) -> Span {
        Span::new(node.start_byte(), node.end_byte())
    }

    pub fn text(&self, node: Node<'_>) -> &str {
        self.source.slice(Self::span(node))
    }

    fn reject_errors(&self) -> Result<(), Error> {
        let mut stack = vec![self.root()];
        while let Some(node) = stack.pop() {
            if node.is_error() || node.kind() == "ERROR" {
                return Err(Error::one(
                    Diagnostic::new(Category::Schema, &self.source.name, "invalid C syntax")
                        .at(Self::span(node)),
                ));
            }
            if node.is_missing() {
                return Err(Error::one(
                    Diagnostic::new(
                        Category::Schema,
                        &self.source.name,
                        format!("missing '{}'", node.kind()),
                    )
                    .at(Self::span(node)),
                ));
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        Ok(())
    }

    fn reject_unsupported_directives(&self) -> Result<(), Error> {
        let mut stack = vec![self.root()];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "preproc_include" => self.check_include(node)?,
                "preproc_def" | "preproc_function_def" => {}
                "preproc_call" => self.check_pragma(node)?,
                "preproc_if" | "preproc_ifdef" | "preproc_ifndef" | "preproc_else"
                | "preproc_elif" | "preproc_elifdef" | "preproc_endif" | "preproc_elifndef" => {
                    return Err(self.directive_error(node, "conditional preprocessing"));
                }
                kind if kind.starts_with("preproc_")
                    && kind != "preproc_arg"
                    && kind != "preproc_directive"
                    && kind != "preproc_defined" =>
                {
                    return Err(self.directive_error(node, kind));
                }
                "_Pragma" => {
                    return Err(self.directive_error(node, "_Pragma"));
                }
                _ => {}
            }
            if self.text(node).starts_with("_Pragma") && node.kind() == "identifier" {
                return Err(self.directive_error(node, "_Pragma"));
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        Ok(())
    }

    fn check_include(&self, node: Node<'_>) -> Result<(), Error> {
        let text = self.text(node).trim();
        const ALLOWED: [&str; 4] = [
            "#include <stdint.h>",
            "#include <stdfloat.h>",
            "#include <stddef.h>",
            "#include <stdbool.h>",
        ];
        if ALLOWED.contains(&text) {
            return Ok(());
        }
        Err(self.directive_error(node, "include"))
    }

    fn check_pragma(&self, node: Node<'_>) -> Result<(), Error> {
        let text = collapse_ws(self.text(node));
        if text == "#pragma once" {
            return Ok(());
        }
        Err(self.directive_error(node, "pragma"))
    }

    fn directive_error(&self, node: Node<'_>, kind: &str) -> Error {
        Error::one(
            Diagnostic::new(
                Category::Schema,
                &self.source.name,
                format!("unsupported preprocessor directive ({kind})"),
            )
            .at(Self::span(node)),
        )
    }

    #[cfg(test)]
    pub fn dump_kinds(&self) -> String {
        let mut out = String::new();
        dump_node(&mut out, self, self.root(), 0);
        out
    }
}

#[cfg(test)]
fn dump_node(out: &mut String, parsed: &ParsedFile<'_>, node: Node<'_>, depth: usize) {
    let extra = if node.is_extra() { " extra" } else { "" };
    out.push_str(&format!(
        "{pad}{kind} [{start}, {end}]{extra} {text:?}\n",
        pad = "  ".repeat(depth),
        kind = node.kind(),
        start = node.start_byte(),
        end = node.end_byte(),
        text = truncate(parsed.text(node), 60)
    ));
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        dump_node(out, parsed, child, depth + 1);
    }
}

#[cfg(test)]
fn truncate(text: &str, max: usize) -> String {
    let collapsed = collapse_ws(text);
    if collapsed.len() <= max {
        collapsed
    } else {
        format!("{}…", &collapsed[..max])
    }
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn collect_comments<'a>(parsed: &'a ParsedFile<'a>) -> Vec<Comment<'a>> {
    let mut comments = Vec::new();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        if node.kind() == "comment" {
            comments.push(Comment {
                span: ParsedFile::span(node),
                text: parsed.text(node),
            });
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    comments.sort_by_key(|comment| comment.span.start);
    comments
}

#[derive(Clone, Copy, Debug)]
pub struct Comment<'a> {
    pub span: Span,
    pub text: &'a str,
}

pub fn collect_macros(parsed: &ParsedFile<'_>) -> Result<Vec<MacroDef>, Error> {
    let mut macros = Vec::new();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "preproc_def" => {
                let name = node.child_by_field_name("name").ok_or_else(|| {
                    Error::one(
                        Diagnostic::new(
                            Category::Schema,
                            &parsed.source.name,
                            "object-like macro is missing a name",
                        )
                        .at(ParsedFile::span(node)),
                    )
                })?;
                let body = node
                    .child_by_field_name("value")
                    .map(|value| parsed.text(value).trim().to_owned())
                    .unwrap_or_default();
                macros.push(MacroDef {
                    name: parsed.text(name).to_owned(),
                    span: ParsedFile::span(name),
                    body,
                    function_like: false,
                });
            }
            "preproc_function_def" => {
                let name = node.child_by_field_name("name").ok_or_else(|| {
                    Error::one(
                        Diagnostic::new(
                            Category::Schema,
                            &parsed.source.name,
                            "function-like macro is missing a name",
                        )
                        .at(ParsedFile::span(node)),
                    )
                })?;
                macros.push(MacroDef {
                    name: parsed.text(name).to_owned(),
                    span: ParsedFile::span(name),
                    body: String::new(),
                    function_like: true,
                });
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    Ok(macros)
}

#[derive(Clone, Debug)]
pub struct MacroDef {
    pub name: String,
    pub span: Span,
    pub body: String,
    pub function_like: bool,
}

#[cfg(test)]
mod tests {
    use super::ParsedFile;
    use crate::source::Source;

    #[test]
    fn dumps_supported_header_shape() {
        let source = Source::new(
            "config.h",
            r#"
#pragma once
#include <stdint.h>
#define CHANNEL_COUNT 4u

/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0x8000
 */
typedef struct {
    uint64_t fingerprint; /**< @mint fingerprint */
    uint32_t device_id;
    uint16_t samples[CHANNEL_COUNT];
} config_t;
"#,
        );
        let parsed = ParsedFile::parse(&source).expect("parse");
        let dump = parsed.dump_kinds();
        eprintln!("{dump}");
        assert!(dump.contains("type_definition"));
        assert!(dump.contains("struct_specifier"));
        assert!(dump.contains("comment"));
        assert!(dump.contains("preproc_def"));
    }

    #[test]
    fn rejects_error_nodes_and_other_includes() {
        let bad = Source::new("bad.h", "typedef struct { uint32_t\n");
        assert!(ParsedFile::parse(&bad).is_err());
        let include = Source::new("bad.h", "#include <stdio.h>\n");
        let error = match ParsedFile::parse(&include) {
            Ok(_) => panic!("stdio include"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("include"));
    }
}
