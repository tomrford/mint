use crate::diagnostic::Error;
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

    pub fn has_block_metadata(&self) -> bool {
        self.block.is_some()
            || self.abi.is_some()
            || self.start_address.is_some()
            || self.padding.is_some()
    }

    pub fn merge(&mut self, src: Self) -> Result<(), &'static str> {
        take_slot(&mut self.block, src.block, "block")?;
        take_slot(&mut self.abi, src.abi, "abi")?;
        take_slot(&mut self.start_address, src.start_address, "start-address")?;
        take_slot(&mut self.padding, src.padding, "padding")?;
        take_slot(&mut self.fingerprint, src.fingerprint, "fingerprint")?;
        self.span = if self.span.is_empty() {
            src.span
        } else {
            self.span.merge(src.span)
        };
        Ok(())
    }
}

fn take_slot<T>(
    dst: &mut Option<T>,
    src: Option<T>,
    tag: &'static str,
) -> Result<(), &'static str> {
    if let Some(src) = src {
        if dst.is_some() {
            return Err(tag);
        }
        *dst = Some(src);
    }
    Ok(())
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
            return Err(Error::schema(
                source,
                comment.span,
                "@mint tags are only accepted in Doxygen comments",
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
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with("@mint") {
            continue;
        }
        parse_tag_line(source, comment, line, &mut tags)?;
    }
    if tags.is_empty() {
        if contains_mint {
            return Err(Error::schema(
                source,
                comment.span,
                "Doxygen comment contains @mint but no recognised tag",
            ));
        }
        return Ok(None);
    }
    Ok(Some(tags))
}

fn parse_tag_line(
    source: &Source,
    comment: &RawComment,
    line: &str,
    tags: &mut MintTags,
) -> Result<(), Error> {
    let rest = line
        .strip_prefix("@mint")
        .map(str::trim_start)
        .unwrap_or(line);
    let mut parts = rest.split_whitespace();
    let tag = parts
        .next()
        .ok_or_else(|| Error::schema(source, comment.span, "expected an @mint tag name"))?;
    let value = parts.next();
    let extra = parts.next();
    match tag {
        "block" => {
            if value.is_some() {
                return Err(tag_extra(source, comment, tag));
            }
            set_once(&mut tags.block, comment.span, source, comment, tag)
        }
        "fingerprint" => {
            if value.is_some() {
                return Err(tag_extra(source, comment, tag));
            }
            set_once(&mut tags.fingerprint, comment.span, source, comment, tag)
        }
        "abi" => set_once(
            &mut tags.abi,
            (
                tag_value(source, comment, tag, value, extra)?.to_owned(),
                comment.span,
            ),
            source,
            comment,
            tag,
        ),
        "start-address" => set_once(
            &mut tags.start_address,
            (
                tag_int(
                    source,
                    comment,
                    tag,
                    value,
                    extra,
                    "start-address must fit an unsigned 32-bit value",
                )?,
                comment.span,
            ),
            source,
            comment,
            tag,
        ),
        "padding" => set_once(
            &mut tags.padding,
            (
                tag_int(
                    source,
                    comment,
                    tag,
                    value,
                    extra,
                    "padding must be one unsigned octet",
                )?,
                comment.span,
            ),
            source,
            comment,
            tag,
        ),
        other => Err(Error::schema(
            source,
            comment.span,
            format!("unknown @mint tag '{other}'"),
        )),
    }
}

fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    source: &Source,
    comment: &RawComment,
    tag: &str,
) -> Result<(), Error> {
    if slot.is_some() {
        return Err(duplicate(source, comment, tag));
    }
    *slot = Some(value);
    Ok(())
}

fn tag_value<'a>(
    source: &Source,
    comment: &RawComment,
    tag: &str,
    value: Option<&'a str>,
    extra: Option<&str>,
) -> Result<&'a str, Error> {
    if extra.is_some() {
        return Err(tag_extra(source, comment, tag));
    }
    value.ok_or_else(|| missing_value(source, comment, tag))
}

fn tag_int<T: TryFrom<u128>>(
    source: &Source,
    comment: &RawComment,
    tag: &str,
    value: Option<&str>,
    extra: Option<&str>,
    overflow: &str,
) -> Result<T, Error> {
    let parsed = parse_c_unsigned(tag_value(source, comment, tag, value, extra)?)
        .map_err(|message| Error::schema(source, comment.span, message))?;
    T::try_from(parsed).map_err(|_| Error::schema(source, comment.span, overflow))
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
    Error::schema(
        source,
        comment.span,
        format!("@mint {tag} is missing a value"),
    )
}

fn tag_extra(source: &Source, comment: &RawComment, tag: &str) -> Error {
    Error::schema(
        source,
        comment.span,
        format!("unexpected text after @mint {tag}"),
    )
}

fn duplicate(source: &Source, comment: &RawComment, tag: &str) -> Error {
    Error::schema(source, comment.span, format!("duplicate @mint {tag} tag"))
}

pub fn attach_leading(source: &Source, comment_end: usize, decl_start: usize) -> bool {
    comment_end <= decl_start && only_trivia_without_blank_line(source, comment_end, decl_start)
}

/// A leading `@mint` comment attaches to the next non-comment token when the
/// gap is only whitespace and comments, with no blank line.
fn only_trivia_without_blank_line(source: &Source, start: usize, end: usize) -> bool {
    let bytes = source.text.as_bytes();
    let start = start.min(bytes.len());
    let end = end.min(bytes.len());
    let mut index = start;
    let mut prev_newline = false;
    let mut only_horizontal_ws = true;
    while index < end {
        match bytes[index] {
            b' ' | b'\t' => index += 1,
            b'\r' => index += 1,
            b'\n' => {
                if prev_newline && only_horizontal_ws {
                    return false;
                }
                prev_newline = true;
                only_horizontal_ws = true;
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                prev_newline = false;
                only_horizontal_ws = false;
                index += 2;
                while index < end && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                prev_newline = false;
                only_horizontal_ws = false;
                index += 2;
                while index + 1 < end && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                    index += 1;
                }
                if index + 1 >= end {
                    return false;
                }
                index += 2;
            }
            _ => return false,
        }
    }
    true
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
    fn leading_attachment_skips_intervening_comments() {
        let source = Source::new(
            "t.h",
            "/** mint */\n/* ordinary */\n/// docs\ntypedef int x;\n",
        );
        let mint_end = source.text.find("*/").expect("mint") + 2;
        let decl = source.text.find("typedef").expect("typedef");
        assert!(super::attach_leading(&source, mint_end, decl));
        let blank = Source::new("t.h", "/** mint */\n\ntypedef int x;\n");
        let blank_end = blank.text.find("*/").expect("mint") + 2;
        let blank_decl = blank.text.find("typedef").expect("typedef");
        assert!(!super::attach_leading(&blank, blank_end, blank_decl));
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
