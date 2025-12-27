pub mod tokens;
use std::{iter::Peekable, str::Chars};

pub use tokens::*;

#[derive(Debug)]
pub struct Tokenizer<'a> {
    source: Peekable<Chars<'a>>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source: source.chars().peekable(),
        }
    }

    pub fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();

        let Some(&ch) = self.source.peek() else {
            return Some(Token::Eof);
        };

        self.source.next();

        match ch {
            '\n' => Some(Token::NewLine),
            '-' => Some(Token::Dash),
            ',' => Some(Token::Comma),
            ':' => Some(Token::Colon),
            '#' => Some(Token::Hash),

            '{' => Some(Token::OpenCurly),
            '}' => Some(Token::CloseCurly),
            '[' => Some(Token::OpenBracket),
            ']' => Some(Token::CloseBracket),
            _ if ch.is_alphanumeric() || ch == '/' || ch == '.' || ch == '_' => {
                Some(self.read_identifier())
            }

            _ => Some(Token::Scalar(ch.to_string())),
        }
    }

    pub fn read_identifier(&mut self) -> Token {
        let mut ident = String::new();
        while let Some(&c) = self.source.peek() {
            if c.is_whitespace() || c == ':' || c == '#' {
                break;
            }
            ident.push(self.source.next().unwrap());
        }
        Token::Identifire(ident)
    }

    pub fn skip_whitespace(&mut self) {
        while let Some(&ch) = self.source.peek()
            && ch.is_whitespace()
        {
            self.source.next().unwrap();
        }
    }

    pub fn peek_while(&mut self) {}
}
