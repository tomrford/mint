use tree_sitter::{Node, Parser, Tree};

use crate::diagnostic::{Category, Error};
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
                Error::named(
                    Category::Schema,
                    &source.name,
                    format!("failed to load C grammar: {error}"),
                )
            })?;
        let tree = parser.parse(&source.text, None).ok_or_else(|| {
            Error::named(
                Category::Schema,
                &source.name,
                "C parser produced no syntax tree",
            )
        })?;
        let parsed = Self { source, tree };
        parsed.reject_tree()?;
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

    fn reject_tree(&self) -> Result<(), Error> {
        let mut stack = vec![(self.root(), false)];
        while let Some((node, in_macro)) = stack.pop() {
            if node.is_error() || node.kind() == "ERROR" {
                if self.error_is_macro_comment_residue(node) {
                    continue;
                }
                return Err(Error::schema(
                    self.source,
                    Self::span(node),
                    "invalid C syntax",
                ));
            }
            if node.is_missing() {
                return Err(Error::schema(
                    self.source,
                    Self::span(node),
                    format!("missing '{}'", node.kind()),
                ));
            }
            if !in_macro {
                self.reject_directive(node)?;
            }
            let in_macro =
                in_macro || matches!(node.kind(), "preproc_def" | "preproc_function_def");
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push((child, in_macro));
            }
        }
        Ok(())
    }

    fn reject_directive(&self, node: Node<'_>) -> Result<(), Error> {
        match node.kind() {
            "preproc_include" => self.check_include(node)?,
            "preproc_def" | "preproc_function_def" => {}
            "preproc_params" | "preproc_arg" | "preproc_directive" | "preproc_defined" => {}
            "preproc_call" => self.check_pragma(node)?,
            "preproc_if" | "preproc_ifdef" | "preproc_ifndef" | "preproc_else" | "preproc_elif"
            | "preproc_elifdef" | "preproc_endif" | "preproc_elifndef" => {
                return Err(self.directive_error(node, "conditional preprocessing"));
            }
            kind if kind.starts_with("preproc_") => {
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
        Ok(())
    }

    fn check_include(&self, node: Node<'_>) -> Result<(), Error> {
        const ALLOWED: [&str; 4] = ["<stdint.h>", "<stdfloat.h>", "<stddef.h>", "<stdbool.h>"];
        let Some(path) = node.child_by_field_name("path") else {
            return Err(self.directive_error(node, "include"));
        };
        let path_text = self.text(path);
        if path.kind() == "system_lib_string"
            && ALLOWED.contains(&path_text)
            && normalize_directive(self.text(node)) == format!("#include {path_text}")
        {
            return Ok(());
        }
        Err(self.directive_error(node, "include"))
    }

    fn check_pragma(&self, node: Node<'_>) -> Result<(), Error> {
        let directive = node
            .child_by_field_name("directive")
            .map(|node| self.text(node));
        let argument = node
            .child_by_field_name("argument")
            .map(|node| normalize_directive(self.text(node)));
        if directive == Some("#pragma") && argument.as_deref() == Some("once") {
            return Ok(());
        }
        Err(self.directive_error(node, "pragma"))
    }

    fn error_is_macro_comment_residue(&self, node: Node<'_>) -> bool {
        if !preceded_by_block_comment_end(&self.source.text, node.start_byte()) {
            return false;
        }
        if !is_shape_expression_text(&strip_c_comments(self.text(node))) {
            return false;
        }
        let line_start = logical_preproc_line_start(&self.source.text, node.start_byte());
        is_object_like_define_line(preproc_logical_rest(&self.source.text, line_start))
    }

    fn directive_error(&self, node: Node<'_>, kind: &str) -> Error {
        Error::schema(
            self.source,
            Self::span(node),
            format!("unsupported preprocessor directive ({kind})"),
        )
    }
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_directive(text: &str) -> String {
    collapse_ws(&strip_c_comments(text))
}

fn preceded_by_block_comment_end(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let mut index = offset.min(bytes.len());
    while index > 0 && matches!(bytes[index - 1], b' ' | b'\t') {
        index -= 1;
    }
    index >= 2 && bytes[index - 2] == b'*' && bytes[index - 1] == b'/'
}

fn is_shape_expression_text(text: &str) -> bool {
    let stripped = collapse_ws(text);
    !stripped.is_empty()
        && stripped.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'+' | b'-' | b'*' | b'/' | b'%' | b'(' | b')' | b' '
                )
        })
}

fn logical_preproc_line_start(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());
    let mut start = text[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    loop {
        if start == 0 {
            return 0;
        }
        let newline = start - 1;
        let escaped = if newline > 0 && bytes[newline - 1] == b'\r' {
            newline > 1 && bytes[newline - 2] == b'\\'
        } else {
            newline > 0 && bytes[newline - 1] == b'\\'
        };
        if !escaped {
            return start;
        }
        start = text[..newline]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
    }
}

fn is_object_like_define_line(line: &str) -> bool {
    let stripped = collapse_ws(&strip_c_comments(line));
    let Some(rest) = stripped.strip_prefix("#define") else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(first) = rest.chars().next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let name_len = rest
        .find(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .unwrap_or(rest.len());
    !rest[name_len..].starts_with('(')
}

fn preproc_logical_rest(text: &str, start: usize) -> &str {
    let bytes = text.as_bytes();
    let start = start.min(bytes.len());
    let mut index = start;
    while index < bytes.len() {
        if bytes[index] == b'\\' && bytes.get(index + 1) == Some(&b'\n') {
            index += 2;
            continue;
        }
        if bytes[index] == b'\\'
            && bytes.get(index + 1) == Some(&b'\r')
            && bytes.get(index + 2) == Some(&b'\n')
        {
            index += 3;
            continue;
        }
        if bytes[index] == b'\n' {
            break;
        }
        index += 1;
    }
    &text[start..index]
}

/// Replace C comments with whitespace so directive and macro text can be
/// compared and evaluated without treating comment punctuation as tokens.
pub fn strip_c_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            if index + 1 < bytes.len() {
                index += 2;
            } else {
                index = bytes.len();
            }
            out.push(b' ');
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn descendants<'tree>(root: Node<'tree>, named: bool) -> Vec<Node<'tree>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        out.push(node);
        let mut cursor = node.walk();
        let children: Vec<_> = if named {
            node.named_children(&mut cursor).collect()
        } else {
            node.children(&mut cursor).collect()
        };
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    out
}

#[cfg(test)]
pub fn collect_macros(parsed: &ParsedFile<'_>) -> Result<Vec<MacroDef>, Error> {
    collect_comments_and_macros(parsed).map(|(_, macros)| macros)
}

pub(crate) fn collect_comments_and_macros<'a>(
    parsed: &'a ParsedFile<'a>,
) -> Result<(Vec<Comment<'a>>, Vec<MacroDef>), Error> {
    let mut comments = Vec::new();
    let mut macros = Vec::new();
    for node in descendants(parsed.root(), false) {
        match node.kind() {
            "comment" => comments.push(Comment {
                span: ParsedFile::span(node),
                text: parsed.text(node),
            }),
            "preproc_def" | "preproc_function_def" => macros.push(collect_macro(parsed, node)?),
            _ => {}
        }
    }
    comments.sort_by_key(|comment| comment.span.start);
    macros.sort_by_key(|macro_def| macro_def.span.start);
    Ok((comments, macros))
}

fn collect_macro(parsed: &ParsedFile<'_>, node: Node<'_>) -> Result<MacroDef, Error> {
    let function_like = node.kind() == "preproc_function_def";
    let name = node.child_by_field_name("name").ok_or_else(|| {
        let kind = if function_like {
            "function-like macro"
        } else {
            "object-like macro"
        };
        Error::schema(
            parsed.source,
            ParsedFile::span(node),
            format!("{kind} is missing a name"),
        )
    })?;
    let body = if function_like {
        String::new()
    } else {
        strip_c_comments(preproc_logical_rest(&parsed.source.text, name.end_byte()))
            .trim()
            .to_owned()
    };
    Ok(MacroDef {
        name: parsed.text(name).to_owned(),
        span: ParsedFile::span(name),
        body,
        function_like,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct Comment<'a> {
    pub span: Span,
    pub text: &'a str,
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

    #[test]
    fn collects_macros_in_source_order_and_strips_bodies() {
        let source = Source::new(
            "config.h",
            "#define SECOND (1u /* a */ + 1u)\n#define FIRST 1u // first\n",
        );
        let parsed = ParsedFile::parse(&source).expect("parse");
        let macros = super::collect_macros(&parsed).expect("macros");
        assert_eq!(
            macros
                .iter()
                .map(|macro_def| (macro_def.name.as_str(), macro_def.body.as_str()))
                .collect::<Vec<_>>(),
            vec![("SECOND", "(1u   + 1u)"), ("FIRST", "1u")]
        );
    }
}
