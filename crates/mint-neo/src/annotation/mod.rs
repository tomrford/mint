use crate::diagnostic::{Category, Diagnostic, Error};
use crate::integers::parse_c_unsigned;
use crate::source::{Source, Span};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentKind {
    Leading,
    Trailing,
}

#[derive(Clone, Debug, Default)]
pub struct MintTags {
    pub block: Option<Span>,
    pub abi: Option<(String, Span)>,
    pub start_address: Option<(u32, Span)>,
    pub padding: Option<(u8, Span)>,
    pub fingerprint: Option<Span>,
    pub span: Span,
    pub kind: Option<CommentKind>,
}

impl MintTags {
    pub fn is_empty(&self) -> bool {
        self.block.is_none()
            && self.abi.is_none()
            && self.start_address.is_none()
            && self.padding.is_none()
            && self.fingerprint.is_none()
    }
}

#[derive(Clone, Debug)]
pub struct RawComment {
    pub span: Span,
    pub text: String,
    pub kind: Option<CommentKind>,
}

/// Merge contiguous `///` comments and keep other comments as-is.
pub fn group_comments(source: &Source, comments: &[(Span, &str)]) -> Vec<RawComment> {
    let mut grouped = Vec::new();
    let mut index = 0;
    while index < comments.len() {
        let (span, text) = comments[index];
        if is_line_doxygen(text) {
            let mut end = span;
            let mut combined = text.to_owned();
            index += 1;
            while index < comments.len() {
                let (next_span, next_text) = comments[index];
                if !is_line_doxygen(next_text) || next_span.start < end.end {
                    break;
                }
                if !source.only_whitespace(end.end, next_span.start)
                    || source.has_blank_line(end.end, next_span.start)
                {
                    break;
                }
                combined.push('\n');
                combined.push_str(next_text);
                end = end.merge(next_span);
                index += 1;
            }
            grouped.push(RawComment {
                span: Span::new(span.start, end.end),
                text: combined,
                kind: Some(CommentKind::Leading),
            });
            continue;
        }
        grouped.push(RawComment {
            span,
            text: text.to_owned(),
            kind: doxygen_kind(text),
        });
        index += 1;
    }
    grouped
}

fn is_line_doxygen(text: &str) -> bool {
    text.trim_start().starts_with("///")
}

fn doxygen_kind(text: &str) -> Option<CommentKind> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("/**<") {
        Some(CommentKind::Trailing)
    } else if trimmed.starts_with("/**") || trimmed.starts_with("/*!") || trimmed.starts_with("///")
    {
        Some(CommentKind::Leading)
    } else {
        None
    }
}

pub fn parse_comment(source: &Source, comment: &RawComment) -> Result<Option<MintTags>, Error> {
    let contains_mint = comment.text.contains("@mint");
    let Some(kind) = comment.kind else {
        if contains_mint {
            return Err(Error::one(
                Diagnostic::new(
                    Category::Schema,
                    &source.name,
                    "@mint tags are only accepted in Doxygen comments",
                )
                .at(comment.span),
            ));
        }
        return Ok(None);
    };
    let body = strip_doxygen(&comment.text);
    let mut tags = MintTags {
        span: comment.span,
        kind: Some(kind),
        ..MintTags::default()
    };
    for (line_index, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with("@mint") {
            continue;
        }
        parse_tag_line(source, comment, line_index, line, &mut tags)?;
    }
    if tags.is_empty() {
        if contains_mint {
            return Err(Error::one(
                Diagnostic::new(
                    Category::Schema,
                    &source.name,
                    "Doxygen comment contains @mint but no recognised tag",
                )
                .at(comment.span),
            ));
        }
        return Ok(None);
    }
    Ok(Some(tags))
}

fn parse_tag_line(
    source: &Source,
    comment: &RawComment,
    _line_index: usize,
    line: &str,
    tags: &mut MintTags,
) -> Result<(), Error> {
    let rest = line
        .strip_prefix("@mint")
        .map(str::trim_start)
        .unwrap_or(line);
    let mut parts = rest.split_whitespace();
    let tag = parts.next().ok_or_else(|| {
        Error::one(
            Diagnostic::new(Category::Schema, &source.name, "expected an @mint tag name")
                .at(comment.span),
        )
    })?;
    let value = parts.next();
    let extra = parts.next();
    match tag {
        "block" => {
            if value.is_some() {
                return Err(tag_extra(source, comment, tag));
            }
            if tags.block.is_some() {
                return Err(duplicate(source, comment, tag));
            }
            tags.block = Some(comment.span);
        }
        "fingerprint" => {
            if value.is_some() {
                return Err(tag_extra(source, comment, tag));
            }
            if tags.fingerprint.is_some() {
                return Err(duplicate(source, comment, tag));
            }
            tags.fingerprint = Some(comment.span);
        }
        "abi" => {
            let value = value.ok_or_else(|| missing_value(source, comment, tag))?;
            if extra.is_some() {
                return Err(tag_extra(source, comment, tag));
            }
            if tags.abi.is_some() {
                return Err(duplicate(source, comment, tag));
            }
            tags.abi = Some((value.to_owned(), comment.span));
        }
        "start-address" => {
            let value = value.ok_or_else(|| missing_value(source, comment, tag))?;
            if extra.is_some() {
                return Err(tag_extra(source, comment, tag));
            }
            if tags.start_address.is_some() {
                return Err(duplicate(source, comment, tag));
            }
            let parsed = parse_c_unsigned(value).map_err(|message| {
                Error::one(
                    Diagnostic::new(Category::Schema, &source.name, message).at(comment.span),
                )
            })?;
            let start = u32::try_from(parsed).map_err(|_| {
                Error::one(
                    Diagnostic::new(
                        Category::Schema,
                        &source.name,
                        "start-address must fit an unsigned 32-bit value",
                    )
                    .at(comment.span),
                )
            })?;
            tags.start_address = Some((start, comment.span));
        }
        "padding" => {
            let value = value.ok_or_else(|| missing_value(source, comment, tag))?;
            if extra.is_some() {
                return Err(tag_extra(source, comment, tag));
            }
            if tags.padding.is_some() {
                return Err(duplicate(source, comment, tag));
            }
            let parsed = parse_c_unsigned(value).map_err(|message| {
                Error::one(
                    Diagnostic::new(Category::Schema, &source.name, message).at(comment.span),
                )
            })?;
            let padding = u8::try_from(parsed).map_err(|_| {
                Error::one(
                    Diagnostic::new(
                        Category::Schema,
                        &source.name,
                        "padding must be one unsigned octet",
                    )
                    .at(comment.span),
                )
            })?;
            tags.padding = Some((padding, comment.span));
        }
        other => {
            return Err(Error::one(
                Diagnostic::new(
                    Category::Schema,
                    &source.name,
                    format!("unknown @mint tag '{other}'"),
                )
                .at(comment.span),
            ));
        }
    }
    Ok(())
}

fn strip_doxygen(text: &str) -> String {
    let trimmed = text.trim();
    let body = if let Some(rest) = trimmed.strip_prefix("/**<") {
        rest.strip_suffix("*/").unwrap_or(rest)
    } else if let Some(rest) = trimmed.strip_prefix("/**") {
        rest.strip_suffix("*/").unwrap_or(rest)
    } else if let Some(rest) = trimmed.strip_prefix("/*!") {
        rest.strip_suffix("*/").unwrap_or(rest)
    } else {
        trimmed
    };
    let mut lines = Vec::new();
    for line in body.lines() {
        let mut line = line.trim();
        if let Some(rest) = line.strip_prefix("///") {
            line = rest;
            if let Some(rest) = line.strip_prefix(' ') {
                line = rest;
            }
        } else {
            line = line.trim_start();
            if let Some(rest) = line.strip_prefix('*') {
                line = rest;
                if let Some(rest) = line.strip_prefix(' ') {
                    line = rest;
                }
            }
        }
        lines.push(line.to_owned());
    }
    lines.join("\n")
}

fn missing_value(source: &Source, comment: &RawComment, tag: &str) -> Error {
    Error::one(
        Diagnostic::new(
            Category::Schema,
            &source.name,
            format!("@mint {tag} is missing a value"),
        )
        .at(comment.span),
    )
}

fn tag_extra(source: &Source, comment: &RawComment, tag: &str) -> Error {
    Error::one(
        Diagnostic::new(
            Category::Schema,
            &source.name,
            format!("unexpected text after @mint {tag}"),
        )
        .at(comment.span),
    )
}

fn duplicate(source: &Source, comment: &RawComment, tag: &str) -> Error {
    Error::one(
        Diagnostic::new(
            Category::Schema,
            &source.name,
            format!("duplicate @mint {tag} tag"),
        )
        .at(comment.span),
    )
}

pub fn attach_leading(source: &Source, comment_end: usize, decl_start: usize) -> bool {
    comment_end <= decl_start
        && source.only_whitespace(comment_end, decl_start)
        && !source.has_blank_line(comment_end, decl_start)
}

pub fn attach_trailing(source: &Source, semicolon: usize, comment_start: usize) -> bool {
    if comment_start <= semicolon {
        return false;
    }
    let (semi_line, _) = source.locate(semicolon);
    let (comment_line, _) = source.locate(comment_start);
    semi_line == comment_line && source.only_whitespace(semicolon + 1, comment_start)
}

#[cfg(test)]
mod tests {
    use super::{RawComment, parse_comment};
    use crate::source::{Source, Span};

    #[test]
    fn parses_block_tags() {
        let source = Source::new("t.h", "");
        let comment = RawComment {
            span: Span::new(0, 10),
            text:
                "/**\n * @mint block\n * @mint abi generic-le\n * @mint start-address 0x8000\n */"
                    .into(),
            kind: Some(super::CommentKind::Leading),
        };
        let tags = parse_comment(&source, &comment).unwrap().unwrap();
        assert!(tags.block.is_some());
        assert_eq!(tags.abi.unwrap().0, "generic-le");
        assert_eq!(tags.start_address.unwrap().0, 0x8000);
    }

    #[test]
    fn rejects_unknown_and_duplicate_tags() {
        let source = Source::new("t.h", "");
        let unknown = RawComment {
            span: Span::new(0, 4),
            text: "/** @mint foo */".into(),
            kind: Some(super::CommentKind::Leading),
        };
        assert!(
            parse_comment(&source, &unknown)
                .unwrap_err()
                .to_string()
                .contains("unknown")
        );
        let dup = RawComment {
            span: Span::new(0, 4),
            text: "/** @mint block\n * @mint block */".into(),
            kind: Some(super::CommentKind::Leading),
        };
        assert!(
            parse_comment(&source, &dup)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }
}
