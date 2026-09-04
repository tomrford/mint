use std::collections::HashMap;

use crate::abi::Abi;
use crate::diagnostic::Error;
mod integer;
use crate::source::{Source, Span};
use crate::syntax::{MacroDef, strip_c_comments};
use integer::Integer;

pub const MAX_MACRO_DEPTH: usize = 128;
const MAX_EXPANSION_TOKENS: usize = 16_384;

#[derive(Clone, Debug)]
pub enum EnumValue {
    Expression(String),
    Successor(String),
}

#[derive(Clone, Debug)]
pub struct EnumConstant {
    pub name: String,
    pub span: Span,
    pub value: EnumValue,
}

#[derive(Clone, Debug, Default)]
pub struct ShapeEnv {
    macros: HashMap<String, Vec<MacroDef>>,
    constants: HashMap<String, Vec<EnumConstant>>,
}

impl ShapeEnv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_enum(&mut self, constant: EnumConstant) {
        self.constants
            .entry(constant.name.clone())
            .or_default()
            .push(constant);
    }

    #[cfg(test)]
    pub fn insert_constant(&mut self, name: String, value: u64, span: Span) {
        self.insert_enum(EnumConstant {
            name,
            span,
            value: EnumValue::Expression(value.to_string()),
        });
    }

    pub fn insert_macro(&mut self, def: MacroDef) {
        self.macros.entry(def.name.clone()).or_default().push(def);
    }

    pub fn reject_macro_use(&self, source: &Source, span: Span) -> Result<(), Error> {
        let name = source.slice(span);
        if let Some(def) = self.macros.get(name).and_then(|defs| {
            defs.iter()
                .find(|def| def.span.end <= span.start && !def.function_like)
        }) {
            return Err(Error::schema(
                source,
                span,
                format!("macro '{name}' may only be used in shape expressions"),
            )
            .related(def.span, "macro defined here"));
        }
        Ok(())
    }
}

pub fn evaluate(
    source: &Source,
    span: Span,
    text: &str,
    env: &ShapeEnv,
    abi: Abi,
) -> Result<u64, Error> {
    let mut evaluation = Evaluation {
        source,
        env,
        abi,
        visiting: Vec::new(),
        remaining: MAX_EXPANSION_TOKENS,
        enum_values: HashMap::new(),
    };
    let value = evaluation.expression(span, text, span.start)?;
    if value.value() <= 0 {
        return Err(Error::schema(
            source,
            span,
            "array extent must be a positive integer",
        ));
    }
    u64::try_from(value.value())
        .map_err(|_| Error::schema(source, span, "array extent does not fit u64"))
}

struct Evaluation<'a> {
    abi: Abi,
    source: &'a Source,
    env: &'a ShapeEnv,
    visiting: Vec<(String, Span)>,
    remaining: usize,
    enum_values: HashMap<String, Integer>,
}

impl Evaluation<'_> {
    fn expression(&mut self, span: Span, text: &str, at: usize) -> Result<Integer, Error> {
        let mut tokens = Vec::new();
        self.expand(span, text, at, &mut tokens)?;
        let mut parser = Parser {
            abi: self.abi,
            source: self.source,
            span,
            tokens,
            index: 0,
            depth: 0,
        };
        let value = parser.expr()?;
        parser.expect_eof()?;
        Ok(value)
    }

    // Macros replace tokens. Only enumerators are evaluated separately,
    // using the definitions visible at their declaration.
    fn expand(
        &mut self,
        span: Span,
        text: &str,
        at: usize,
        out: &mut Vec<Token>,
    ) -> Result<(), Error> {
        for token in lex(self.source, span, text)? {
            self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
                Error::schema(self.source, span, "shape expansion exceeds 16384 tokens")
            })?;
            let Token::Ident(name) = token else {
                out.push(token);
                continue;
            };
            if matches!(
                name.as_str(),
                "sizeof" | "_Alignof" | "alignof" | "offsetof" | "_Pragma"
            ) {
                return Err(Error::schema(
                    self.source,
                    span,
                    format!("'{name}' is not allowed in shape expressions"),
                ));
            }
            if let Some(defs) = self.env.macros.get(&name) {
                if defs.len() > 1 {
                    return Err(self.duplicate(
                        span,
                        &name,
                        "referenced macro",
                        defs.iter().map(|d| d.span),
                    ));
                }
                if let Some(def) = defs.iter().find(|def| def.span.end <= at) {
                    if let Some(constant) = self
                        .env
                        .constants
                        .get(&name)
                        .and_then(|defs| defs.iter().find(|d| d.span.end <= at))
                    {
                        return Err(Error::schema(self.source, span,
                            format!("shape constant '{name}' is defined as both a macro and an enumerator"))
                            .related(def.span, "macro defined here")
                            .related(constant.span, "enumerator defined here"));
                    }
                    if def.function_like {
                        return Err(Error::schema(
                            self.source,
                            span,
                            format!(
                                "function-like macro '{name}' cannot be used as an array extent"
                            ),
                        )
                        .related(def.span, "macro defined here"));
                    }
                    self.enter(&name, def.span)?;
                    self.expand(def.span, &def.body, at, out)?;
                    self.visiting.pop();
                    continue;
                }
            }
            let value = self.enumerator(&name, span, at)?;
            out.push(Token::Value(value));
        }
        Ok(())
    }

    fn enumerator(&mut self, name: &str, span: Span, at: usize) -> Result<Integer, Error> {
        let Some(defs) = self.env.constants.get(name) else {
            let reason = if self.env.macros.contains_key(name) {
                "is not available here"
            } else {
                "is unknown"
            };
            return Err(Error::schema(
                self.source,
                span,
                format!("shape constant '{name}' {reason}"),
            ));
        };
        if defs.len() > 1 {
            return Err(self.duplicate(span, name, "enumerator", defs.iter().map(|d| d.span)));
        }
        let def = &defs[0];
        if def.span.end > at {
            return Err(Error::schema(
                self.source,
                span,
                format!("shape constant '{name}' is not available here"),
            ));
        }
        if let Some(&value) = self.enum_values.get(name) {
            return Ok(value);
        }
        self.env.reject_macro_use(self.source, def.span)?;
        self.enter(name, def.span)?;
        let value = match &def.value {
            EnumValue::Expression(text) => self.expression(def.span, text, def.span.start)?,
            EnumValue::Successor(previous) => Integer::enumerator(
                self.enumerator(previous, def.span, def.span.start)?.value() + 1,
                self.abi,
            )
            .map_err(|message| Error::schema(self.source, def.span, message))?,
        };
        let value = Integer::enumerator(value.value(), self.abi)
            .map_err(|message| Error::schema(self.source, def.span, message))?;
        self.visiting.pop();
        self.enum_values.insert(name.to_owned(), value);
        Ok(value)
    }

    fn enter(&mut self, name: &str, span: Span) -> Result<(), Error> {
        if self.visiting.iter().any(|(n, _)| n == name) {
            let mut error = Error::schema(
                self.source,
                span,
                format!("cyclic shape-constant dependency involving '{name}'"),
            );
            for (name, span) in &self.visiting {
                error = error.related(*span, format!("'{name}' participates in the cycle"));
            }
            return Err(error);
        }
        if self.visiting.len() >= MAX_MACRO_DEPTH {
            return Err(Error::schema(
                self.source,
                span,
                "shape-constant expansion exceeds 128 levels",
            ));
        }
        self.visiting.push((name.to_owned(), span));
        Ok(())
    }

    fn duplicate(
        &self,
        span: Span,
        name: &str,
        kind: &str,
        spans: impl Iterator<Item = Span>,
    ) -> Error {
        let mut error = Error::schema(self.source, span, format!("duplicate {kind} '{name}'"));
        for span in spans {
            error = error.related(span, "defined here");
        }
        error
    }
}

struct Parser<'a> {
    abi: Abi,
    source: &'a Source,
    span: Span,
    tokens: Vec<Token>,
    index: usize,
    depth: usize,
}

impl Parser<'_> {
    fn expr(&mut self) -> Result<Integer, Error> {
        let mut value = self.term()?;
        while let Some(op) = match self.peek() {
            Some(Token::Plus) => Some('+'),
            Some(Token::Minus) => Some('-'),
            _ => None,
        } {
            self.bump();
            let rhs = self.term()?;
            value = value
                .binary(rhs, op, self.abi)
                .map_err(|message| self.error(message))?;
        }
        Ok(value)
    }

    fn term(&mut self) -> Result<Integer, Error> {
        let mut value = self.factor()?;
        while let Some(op) = match self.peek() {
            Some(Token::Star) => Some('*'),
            Some(Token::Slash) => Some('/'),
            Some(Token::Percent) => Some('%'),
            _ => None,
        } {
            self.bump();
            let rhs = self.factor()?;
            value = value
                .binary(rhs, op, self.abi)
                .map_err(|message| self.error(message))?;
        }
        Ok(value)
    }

    fn factor(&mut self) -> Result<Integer, Error> {
        let mut negatives = 0;
        while matches!(self.peek(), Some(Token::Plus | Token::Minus)) {
            negatives += usize::from(matches!(self.peek(), Some(Token::Minus)));
            self.bump();
        }
        let mut value = self.primary()?;
        // Apply each minus, so a double negation cannot hide signed overflow.
        for _ in 0..negatives {
            value = value
                .negate(self.abi)
                .map_err(|message| self.error(message))?;
        }
        Ok(value)
    }

    fn primary(&mut self) -> Result<Integer, Error> {
        match self.peek() {
            Some(Token::Number(text)) => {
                let text = text.clone();
                self.bump();
                Integer::literal(&text, self.abi).map_err(|message| self.error(message))
            }
            Some(Token::Value(value)) => {
                let value = *value;
                self.bump();
                Ok(value)
            }
            Some(Token::LParen) => {
                self.bump();
                if self.depth >= MAX_MACRO_DEPTH {
                    return Err(self.error("shape-expression nesting exceeds 128 levels"));
                }
                self.depth += 1;
                let value = self.expr()?;
                self.depth -= 1;
                if !matches!(self.peek(), Some(Token::RParen)) {
                    return Err(self.error("expected ')'"));
                }
                self.bump();
                Ok(value)
            }
            _ => Err(self.error("expected a shape expression")),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn bump(&mut self) {
        self.index += 1;
    }

    fn expect_eof(&self) -> Result<(), Error> {
        if self.index == self.tokens.len() {
            Ok(())
        } else {
            Err(self.error("unexpected tokens after shape expression"))
        }
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error::schema(self.source, self.span, message)
    }
}

#[derive(Clone, Debug)]
enum Token {
    Value(Integer),
    Number(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
}

fn lex(source: &Source, span: Span, text: &str) -> Result<Vec<Token>, Error> {
    let stripped = strip_c_comments(text);
    let bytes = stripped.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'+' | b'-') && bytes.get(index + 1) == Some(&bytes[index]) {
            return Err(Error::schema(
                source,
                span,
                "increment and decrement are not shape operators",
            ));
        }
        let simple = match bytes[index] {
            b' ' | b'\t' | b'\n' | b'\r' => {
                index += 1;
                continue;
            }
            b'+' => Some(Token::Plus),
            b'-' => Some(Token::Minus),
            b'*' => Some(Token::Star),
            b'/' => Some(Token::Slash),
            b'%' => Some(Token::Percent),
            b'(' => Some(Token::LParen),
            b')' => Some(Token::RParen),
            _ => None,
        };
        if let Some(token) = simple {
            tokens.push(token);
            index += 1;
            continue;
        }
        if bytes[index].is_ascii_digit() {
            let start = index;
            index = scan_number(bytes, index);
            tokens.push(Token::Number(stripped[start..index].to_owned()));
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token::Ident(stripped[start..index].to_owned()));
            continue;
        }
        return Err(Error::schema(
            source,
            span,
            format!("invalid character in shape expression '{text}'"),
        ));
    }
    Ok(tokens)
}

fn scan_number(bytes: &[u8], mut index: usize) -> usize {
    if matches!(bytes.get(index + 1), Some(b'x' | b'X')) {
        index += 2;
        while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
            index += 1;
        }
    } else {
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
    }
    while index < bytes.len() && matches!(bytes[index], b'u' | b'U' | b'l' | b'L') {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::{ShapeEnv, evaluate};
    use crate::abi::Abi;
    use crate::source::{Source, Span};
    use crate::syntax::MacroDef;

    fn object_macro(name: &str, span: Span, body: &str) -> MacroDef {
        MacroDef {
            name: name.to_owned(),
            span,
            body: body.to_owned(),
            function_like: false,
        }
    }

    #[test]
    fn evaluates_literals_and_macros() {
        let source = Source::new("t.h", "");
        let mut env = ShapeEnv::new();
        env.insert_macro(object_macro("CHANNEL_COUNT", Span::new(0, 1), "4u"));
        env.insert_macro(object_macro(
            "SAMPLE_COUNT",
            Span::new(1, 2),
            "(CHANNEL_COUNT * 8u)",
        ));
        assert_eq!(
            evaluate(&source, Span::new(10, 12), "4u", &env, Abi::GenericLe).unwrap(),
            4
        );
        assert_eq!(
            evaluate(
                &source,
                Span::new(10, 22),
                "SAMPLE_COUNT",
                &env,
                Abi::GenericLe
            )
            .unwrap(),
            32
        );
        assert!(evaluate(&source, Span::new(10, 15), "4 - 5", &env, Abi::GenericLe).is_err());
        assert!(evaluate(&source, Span::new(10, 15), "1 / 0", &env, Abi::GenericLe).is_err());
        assert_eq!(
            evaluate(
                &source,
                Span::new(10, 20),
                "4u /* n */",
                &env,
                Abi::GenericLe
            )
            .unwrap(),
            4
        );
    }

    #[test]
    fn rejects_duplicate_referenced_macros_and_enum_collisions() {
        let source = Source::new("t.h", "");
        let mut env = ShapeEnv::new();
        env.insert_macro(object_macro("N", Span::new(0, 1), "1u"));
        env.insert_macro(object_macro("N", Span::new(2, 3), "2u"));
        assert!(
            evaluate(&source, Span::new(10, 12), "N", &env, Abi::GenericLe)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let mut env = ShapeEnv::new();
        env.insert_macro(object_macro("AXIS", Span::new(0, 1), "3u"));
        env.insert_constant("AXIS".into(), 4, Span::new(2, 3));
        assert!(
            evaluate(&source, Span::new(10, 14), "AXIS", &env, Abi::GenericLe)
                .unwrap_err()
                .to_string()
                .contains("enumerator")
        );
    }

    #[test]
    fn bounds_acyclic_macro_expansion() {
        let source = Source::new("t.h", "");
        let mut env = ShapeEnv::new();
        env.insert_macro(object_macro("M0", Span::new(0, 1), "1u"));
        for index in 1..=super::MAX_MACRO_DEPTH {
            env.insert_macro(object_macro(
                &format!("M{index}"),
                Span::new(index, index + 1),
                &format!("M{}", index - 1),
            ));
        }
        assert!(
            evaluate(
                &source,
                Span::new(1000, 1002),
                &format!("M{}", super::MAX_MACRO_DEPTH),
                &env,
                Abi::GenericLe
            )
            .unwrap_err()
            .to_string()
            .contains("exceeds")
        );
    }
}
