use std::collections::{HashMap, HashSet};

use crate::diagnostic::{Category, Diagnostic, Error};
use crate::integers::parse_c_unsigned;
use crate::source::{Source, Span};

pub const MAX_MACRO_DEPTH: usize = 128;

#[derive(Clone, Debug)]
pub struct ShapeEnv {
    macros: HashMap<String, Vec<ShapeValue>>,
    constants: HashMap<String, ShapeValue>,
}

#[derive(Clone, Debug)]
pub struct ShapeValue {
    pub value: u64,
    pub span: Span,
    pub function_like: bool,
    pub body: Option<String>,
}

impl ShapeEnv {
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            constants: HashMap::new(),
        }
    }

    pub fn insert_constant(&mut self, name: String, value: u64, span: Span) {
        self.constants.insert(
            name,
            ShapeValue {
                value,
                span,
                function_like: false,
                body: None,
            },
        );
    }

    pub fn insert_macro(&mut self, name: String, span: Span, body: String, function_like: bool) {
        self.macros.entry(name).or_default().push(ShapeValue {
            value: 0,
            span,
            function_like,
            body: Some(body),
        });
    }
}

pub fn evaluate(source: &Source, span: Span, text: &str, env: &ShapeEnv) -> Result<u64, Error> {
    let value = evaluate_any(source, span, text, env)?;
    if value == 0 {
        return Err(Error::one(
            Diagnostic::new(
                Category::Schema,
                &source.name,
                "array extent must be a positive integer",
            )
            .at(span),
        ));
    }
    u64::try_from(value).map_err(|_| {
        Error::one(
            Diagnostic::new(
                Category::Schema,
                &source.name,
                "array extent does not fit u64",
            )
            .at(span),
        )
    })
}

pub fn evaluate_any(
    source: &Source,
    span: Span,
    text: &str,
    env: &ShapeEnv,
) -> Result<u128, Error> {
    let mut visiting = HashSet::new();
    evaluate_in(source, span, span.start, text, env, &mut visiting, 0)
}

fn evaluate_in(
    source: &Source,
    span: Span,
    at: usize,
    text: &str,
    env: &ShapeEnv,
    visiting: &mut HashSet<String>,
    depth: usize,
) -> Result<u128, Error> {
    let tokens = lex(source, span, text)?;
    let mut parser = Parser {
        source,
        span,
        at,
        tokens,
        index: 0,
        env,
        visiting,
        depth,
    };
    let value = parser.expr()?;
    parser.expect_eof()?;
    Ok(value)
}

struct Parser<'a> {
    source: &'a Source,
    span: Span,
    at: usize,
    tokens: Vec<(Token, Span)>,
    index: usize,
    env: &'a ShapeEnv,
    visiting: &'a mut HashSet<String>,
    depth: usize,
}

impl Parser<'_> {
    fn expr(&mut self) -> Result<u128, Error> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.bump();
                    let rhs = self.term()?;
                    value = self.add(value, rhs)?;
                }
                Some(Token::Minus) => {
                    self.bump();
                    let rhs = self.term()?;
                    value = self.sub(value, rhs)?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn term(&mut self) -> Result<u128, Error> {
        let mut value = self.factor()?;
        loop {
            match self.peek() {
                Some(Token::Star) => {
                    self.bump();
                    let rhs = self.factor()?;
                    value = self.mul(value, rhs)?;
                }
                Some(Token::Slash) => {
                    self.bump();
                    let rhs = self.factor()?;
                    if rhs == 0 {
                        return Err(self.error("division by zero"));
                    }
                    value /= rhs;
                }
                Some(Token::Percent) => {
                    self.bump();
                    let rhs = self.factor()?;
                    if rhs == 0 {
                        return Err(self.error("modulo by zero"));
                    }
                    value %= rhs;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn factor(&mut self) -> Result<u128, Error> {
        match self.peek() {
            Some(Token::Plus) => {
                self.bump();
                self.factor()
            }
            Some(Token::Minus) => {
                Err(self.error("unary minus is not allowed in shape expressions"))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<u128, Error> {
        match self.peek() {
            Some(Token::Number(text)) => {
                self.bump();
                parse_c_unsigned(&text).map_err(|message| self.error(message))
            }
            Some(Token::Ident(name)) => {
                self.bump();
                self.lookup(&name)
            }
            Some(Token::LParen) => {
                self.bump();
                let value = self.expr()?;
                if !matches!(self.peek(), Some(Token::RParen)) {
                    return Err(self.error("expected ')'"));
                }
                self.bump();
                Ok(value)
            }
            _ => Err(self.error("expected a shape expression")),
        }
    }

    fn lookup(&mut self, name: &str) -> Result<u128, Error> {
        if matches!(
            name,
            "sizeof" | "_Alignof" | "alignof" | "offsetof" | "_Pragma"
        ) {
            return Err(self.error(format!("'{name}' is not allowed in shape expressions")));
        }
        if let Some(defs) = self.env.macros.get(name) {
            let defs = defs.clone();
            if defs.len() > 1 {
                return Err(self.duplicate_macros(name, &defs));
            }
            if let Some(value) = defs.into_iter().find(|value| value.span.end <= self.at) {
                if let Some(constant) = self.env.constants.get(name).cloned()
                    && constant.span.end <= self.at
                {
                    return Err(self.macro_enum_collision(name, &value, &constant));
                }
                return self.expand_macro(name, &value);
            }
        }
        if let Some(value) = self.env.constants.get(name).cloned() {
            if value.span.end > self.at {
                return Err(self.error(format!("shape constant '{name}' is not available here")));
            }
            return Ok(u128::from(value.value));
        }
        if self.env.macros.contains_key(name) {
            return Err(self.error(format!("shape constant '{name}' is not available here")));
        }
        Err(self.error(format!("unknown shape constant '{name}'")))
    }

    fn expand_macro(&mut self, name: &str, value: &ShapeValue) -> Result<u128, Error> {
        if value.function_like {
            return Err(Error::one(
                Diagnostic::new(
                    Category::Schema,
                    &self.source.name,
                    format!("function-like macro '{name}' cannot be used as an array extent"),
                )
                .at(self.span)
                .related(&self.source.name, value.span, "macro defined here"),
            ));
        }
        let Some(body) = &value.body else {
            return Ok(u128::from(value.value));
        };
        if self.depth >= MAX_MACRO_DEPTH {
            return Err(self.error(format!("macro expansion exceeds {MAX_MACRO_DEPTH} levels")));
        }
        if !self.visiting.insert(name.to_owned()) {
            return Err(self.cycle(name, value));
        }
        let result = evaluate_in(
            self.source,
            value.span,
            self.at,
            body,
            self.env,
            self.visiting,
            self.depth + 1,
        );
        self.visiting.remove(name);
        result
    }

    fn duplicate_macros(&self, name: &str, defs: &[ShapeValue]) -> Error {
        let mut diagnostic = Diagnostic::new(
            Category::Schema,
            &self.source.name,
            format!("duplicate referenced macro '{name}'"),
        )
        .at(self.span);
        for value in defs {
            diagnostic = diagnostic.related(
                &self.source.name,
                value.span,
                format!("'{name}' defined here"),
            );
        }
        Error::one(diagnostic)
    }

    fn macro_enum_collision(
        &self,
        name: &str,
        macro_def: &ShapeValue,
        enumerator: &ShapeValue,
    ) -> Error {
        Error::one(
            Diagnostic::new(
                Category::Schema,
                &self.source.name,
                format!("shape constant '{name}' is defined as both a macro and an enumerator"),
            )
            .at(self.span)
            .related(&self.source.name, macro_def.span, "macro defined here")
            .related(
                &self.source.name,
                enumerator.span,
                "enumerator defined here",
            ),
        )
    }

    fn cycle(&self, name: &str, value: &ShapeValue) -> Error {
        let mut diagnostic = Diagnostic::new(
            Category::Schema,
            &self.source.name,
            format!("cyclic shape-constant dependency involving '{name}'"),
        )
        .at(self.span);
        diagnostic = diagnostic.related(
            &self.source.name,
            value.span,
            format!("'{name}' participates in the cycle"),
        );
        for participant in self.visiting.iter() {
            let span = self
                .env
                .macros
                .get(participant)
                .and_then(|defs| defs.first())
                .map(|value| value.span)
                .or_else(|| self.env.constants.get(participant).map(|value| value.span));
            if let Some(span) = span {
                diagnostic = diagnostic.related(
                    &self.source.name,
                    span,
                    format!("'{participant}' participates in the cycle"),
                );
            }
        }
        Error::one(diagnostic)
    }

    fn add(&self, left: u128, right: u128) -> Result<u128, Error> {
        left.checked_add(right)
            .ok_or_else(|| self.error("shape-expression addition overflowed"))
    }

    fn sub(&self, left: u128, right: u128) -> Result<u128, Error> {
        left.checked_sub(right)
            .ok_or_else(|| self.error("shape-expression subtraction produced a negative value"))
    }

    fn mul(&self, left: u128, right: u128) -> Result<u128, Error> {
        left.checked_mul(right)
            .ok_or_else(|| self.error("shape-expression multiplication overflowed"))
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.index).map(|(token, _)| token.clone())
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
        Error::one(Diagnostic::new(Category::Schema, &self.source.name, message).at(self.span))
    }
}

#[derive(Clone, Debug)]
enum Token {
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

fn lex(source: &Source, span: Span, text: &str) -> Result<Vec<(Token, Span)>, Error> {
    let mut tokens = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\n' | b'\r' => index += 1,
            b'+' => {
                tokens.push((Token::Plus, span));
                index += 1;
            }
            b'-' => {
                tokens.push((Token::Minus, span));
                index += 1;
            }
            b'*' => {
                tokens.push((Token::Star, span));
                index += 1;
            }
            b'/' => {
                if bytes.get(index + 1) == Some(&b'/') {
                    index += 2;
                    while index < bytes.len() && bytes[index] != b'\n' {
                        index += 1;
                    }
                } else if bytes.get(index + 1) == Some(&b'*') {
                    index += 2;
                    while index + 1 < bytes.len()
                        && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                    {
                        index += 1;
                    }
                    index = index.saturating_add(2).min(bytes.len());
                } else {
                    tokens.push((Token::Slash, span));
                    index += 1;
                }
            }
            b'%' => {
                tokens.push((Token::Percent, span));
                index += 1;
            }
            b'(' => {
                tokens.push((Token::LParen, span));
                index += 1;
            }
            b')' => {
                tokens.push((Token::RParen, span));
                index += 1;
            }
            b'0'..=b'9' => {
                let start = index;
                if bytes.get(index + 1) == Some(&b'x') || bytes.get(index + 1) == Some(&b'X') {
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
                tokens.push((Token::Number(text[start..index].to_owned()), span));
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                tokens.push((Token::Ident(text[start..index].to_owned()), span));
            }
            _ => {
                return Err(Error::one(
                    Diagnostic::new(
                        Category::Schema,
                        &source.name,
                        format!("invalid character in shape expression '{text}'"),
                    )
                    .at(span),
                ));
            }
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{ShapeEnv, evaluate};
    use crate::source::{Source, Span};

    #[test]
    fn evaluates_literals_and_macros() {
        let source = Source::new("t.h", "");
        let mut env = ShapeEnv::new();
        env.insert_macro("CHANNEL_COUNT".into(), Span::new(0, 1), "4u".into(), false);
        env.insert_macro(
            "SAMPLE_COUNT".into(),
            Span::new(1, 2),
            "(CHANNEL_COUNT * 8u)".into(),
            false,
        );
        assert_eq!(evaluate(&source, Span::new(10, 12), "4u", &env).unwrap(), 4);
        assert_eq!(
            evaluate(&source, Span::new(10, 22), "SAMPLE_COUNT", &env).unwrap(),
            32
        );
        assert!(evaluate(&source, Span::new(10, 15), "4 - 5", &env).is_err());
        assert!(evaluate(&source, Span::new(10, 15), "1 / 0", &env).is_err());
        assert_eq!(
            evaluate(&source, Span::new(10, 20), "4u /* n */", &env).unwrap(),
            4
        );
    }

    #[test]
    fn rejects_duplicate_referenced_macros_and_enum_collisions() {
        let source = Source::new("t.h", "");
        let mut env = ShapeEnv::new();
        env.insert_macro("N".into(), Span::new(0, 1), "1u".into(), false);
        env.insert_macro("N".into(), Span::new(2, 3), "2u".into(), false);
        assert!(
            evaluate(&source, Span::new(10, 12), "N", &env)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let mut env = ShapeEnv::new();
        env.insert_macro("AXIS".into(), Span::new(0, 1), "3u".into(), false);
        env.insert_constant("AXIS".into(), 4, Span::new(2, 3));
        assert!(
            evaluate(&source, Span::new(10, 14), "AXIS", &env)
                .unwrap_err()
                .to_string()
                .contains("enumerator")
        );
    }

    #[test]
    fn bounds_acyclic_macro_expansion() {
        let source = Source::new("t.h", "");
        let mut env = ShapeEnv::new();
        env.insert_macro("M0".into(), Span::new(0, 1), "1u".into(), false);
        for index in 1..=super::MAX_MACRO_DEPTH {
            env.insert_macro(
                format!("M{index}"),
                Span::new(index, index + 1),
                format!("M{}", index - 1),
                false,
            );
        }
        assert!(
            evaluate(
                &source,
                Span::new(1000, 1002),
                &format!("M{}", super::MAX_MACRO_DEPTH),
                &env
            )
            .unwrap_err()
            .to_string()
            .contains("exceeds")
        );
    }
}
