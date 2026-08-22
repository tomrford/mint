use std::collections::{HashMap, HashSet};

use crate::diagnostic::Error;
use crate::integers::parse_c_unsigned;
use crate::source::{Source, Span};
use crate::syntax::{MacroDef, strip_c_comments};

pub const MAX_MACRO_DEPTH: usize = 128;

#[derive(Clone, Debug)]
pub struct ShapeEnv {
    macros: HashMap<String, Vec<MacroDef>>,
    constants: HashMap<String, (u64, Span)>,
}

impl ShapeEnv {
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            constants: HashMap::new(),
        }
    }

    pub fn insert_constant(&mut self, name: String, value: u64, span: Span) -> Option<Span> {
        self.constants
            .insert(name, (value, span))
            .map(|(_, previous)| previous)
    }

    pub fn insert_macro(&mut self, def: MacroDef) {
        self.macros.entry(def.name.clone()).or_default().push(def);
    }
}

pub fn evaluate(source: &Source, span: Span, text: &str, env: &ShapeEnv) -> Result<u64, Error> {
    let value = evaluate_any(source, span, text, env)?;
    if value == 0 {
        return Err(Error::schema(
            source,
            span,
            "array extent must be a positive integer",
        ));
    }
    u64::try_from(value).map_err(|_| Error::schema(source, span, "array extent does not fit u64"))
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
    tokens: Vec<Token>,
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
                    value = value
                        .checked_add(rhs)
                        .ok_or_else(|| self.error("shape-expression addition overflowed"))?;
                }
                Some(Token::Minus) => {
                    self.bump();
                    let rhs = self.term()?;
                    value = value.checked_sub(rhs).ok_or_else(|| {
                        self.error("shape-expression subtraction produced a negative value")
                    })?;
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
                    value = value
                        .checked_mul(rhs)
                        .ok_or_else(|| self.error("shape-expression multiplication overflowed"))?;
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
                let text = text.clone();
                self.bump();
                parse_c_unsigned(&text).map_err(|message| self.error(message))
            }
            Some(Token::Ident(name)) => {
                let name = name.clone();
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
            if let Some(def) = defs.into_iter().find(|def| def.span.end <= self.at) {
                if let Some((_, constant_span)) = self.env.constants.get(name).copied()
                    && constant_span.end <= self.at
                {
                    return Err(self.macro_enum_collision(name, def.span, constant_span));
                }
                return self.expand_macro(name, &def);
            }
        }
        if let Some((value, span)) = self.env.constants.get(name).copied() {
            if span.end > self.at {
                return Err(self.error(format!("shape constant '{name}' is not available here")));
            }
            return Ok(u128::from(value));
        }
        if self.env.macros.contains_key(name) {
            return Err(self.error(format!("shape constant '{name}' is not available here")));
        }
        Err(self.error(format!("unknown shape constant '{name}'")))
    }

    fn expand_macro(&mut self, name: &str, def: &MacroDef) -> Result<u128, Error> {
        if def.function_like {
            return Err(Error::schema(
                self.source,
                self.span,
                format!("function-like macro '{name}' cannot be used as an array extent"),
            )
            .related(def.span, "macro defined here"));
        }
        if self.depth >= MAX_MACRO_DEPTH {
            return Err(self.error(format!("macro expansion exceeds {MAX_MACRO_DEPTH} levels")));
        }
        if !self.visiting.insert(name.to_owned()) {
            return Err(self.cycle(name, def.span));
        }
        let result = evaluate_in(
            self.source,
            def.span,
            self.at,
            &def.body,
            self.env,
            self.visiting,
            self.depth + 1,
        );
        self.visiting.remove(name);
        result
    }

    fn duplicate_macros(&self, name: &str, defs: &[MacroDef]) -> Error {
        let mut error = Error::schema(
            self.source,
            self.span,
            format!("duplicate referenced macro '{name}'"),
        );
        for def in defs {
            error = error.related(def.span, format!("'{name}' defined here"));
        }
        error
    }

    fn macro_enum_collision(&self, name: &str, macro_span: Span, enumerator_span: Span) -> Error {
        Error::schema(
            self.source,
            self.span,
            format!("shape constant '{name}' is defined as both a macro and an enumerator"),
        )
        .related(macro_span, "macro defined here")
        .related(enumerator_span, "enumerator defined here")
    }

    fn cycle(&self, name: &str, span: Span) -> Error {
        let mut error = Error::schema(
            self.source,
            self.span,
            format!("cyclic shape-constant dependency involving '{name}'"),
        )
        .related(span, format!("'{name}' participates in the cycle"));
        for participant in self.visiting.iter() {
            let span = self
                .env
                .macros
                .get(participant)
                .and_then(|defs| defs.first())
                .map(|def| def.span)
                .or_else(|| self.env.constants.get(participant).map(|(_, span)| *span));
            if let Some(span) = span {
                error = error.related(span, format!("'{participant}' participates in the cycle"));
            }
        }
        error
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
        env.insert_macro(object_macro("N", Span::new(0, 1), "1u"));
        env.insert_macro(object_macro("N", Span::new(2, 3), "2u"));
        assert!(
            evaluate(&source, Span::new(10, 12), "N", &env)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let mut env = ShapeEnv::new();
        env.insert_macro(object_macro("AXIS", Span::new(0, 1), "3u"));
        let _ = env.insert_constant("AXIS".into(), 4, Span::new(2, 3));
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
                &env
            )
            .unwrap_err()
            .to_string()
            .contains("exceeds")
        );
    }
}
