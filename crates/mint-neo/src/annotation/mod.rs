use crate::diagnostic::Error;
use crate::integers::parse_c_unsigned;
use crate::source::{Source, Span};

#[derive(Clone, Debug, Default)]
pub struct MintTags {
    pub block: Option<Span>,
    pub abi: Option<(String, Span)>,
    pub start_address: Option<(u32, Span)>,
    pub padding: Option<(u8, Span)>,
    pub fingerprint: Option<Span>,
    pub span: Span,
}

impl MintTags {
    pub fn has_block_metadata(&self) -> bool {
        self.block.is_some()
            || self.abi.is_some()
            || self.start_address.is_some()
            || self.padding.is_some()
    }
}

/// One leading Doxygen block carries all metadata for the next declaration.
pub fn parse_comment(source: &Source, span: Span, text: &str) -> Result<Option<MintTags>, Error> {
    if !text.contains("@mint") {
        return Ok(None);
    }
    let fail = |message| Error::schema(source, span, message);
    let Some(body) = text
        .strip_prefix("/**")
        .filter(|body| !body.starts_with('<'))
        .and_then(|body| body.strip_suffix("*/"))
    else {
        return Err(fail(
            "@mint tags require a leading /** ... */ Doxygen comment".to_owned(),
        ));
    };
    let mut tags = MintTags {
        span,
        ..MintTags::default()
    };
    for line in body.lines() {
        let line = line.trim().trim_start_matches('*').trim();
        if !line.contains("@mint") {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["@mint", "block"] => set(&mut tags.block, span, source, span, "block")?,
            ["@mint", "fingerprint"] => {
                set(&mut tags.fingerprint, span, source, span, "fingerprint")?
            }
            ["@mint", "abi", value] => set(
                &mut tags.abi,
                (value.to_string(), span),
                source,
                span,
                "abi",
            )?,
            ["@mint", "start-address", value] => {
                let number = number(
                    value,
                    source,
                    span,
                    "start-address must fit an unsigned 32-bit value",
                )?;
                set(
                    &mut tags.start_address,
                    (number, span),
                    source,
                    span,
                    "start-address",
                )?;
            }
            ["@mint", "padding", value] => {
                let number = number(value, source, span, "padding must be one unsigned octet")?;
                set(&mut tags.padding, (number, span), source, span, "padding")?;
            }
            _ => return Err(fail(format!("unknown or malformed @mint tag: '{line}'"))),
        }
    }
    Ok(Some(tags))
}

fn set<T>(
    slot: &mut Option<T>,
    value: T,
    source: &Source,
    span: Span,
    tag: &str,
) -> Result<(), Error> {
    if slot.replace(value).is_some() {
        return Err(Error::schema(
            source,
            span,
            format!("duplicate @mint {tag} tag"),
        ));
    }
    Ok(())
}

fn number<T: TryFrom<u128>>(
    text: &str,
    source: &Source,
    span: Span,
    overflow: &str,
) -> Result<T, Error> {
    let value = parse_c_unsigned(text).map_err(|message| Error::schema(source, span, message))?;
    T::try_from(value).map_err(|_| Error::schema(source, span, overflow))
}
