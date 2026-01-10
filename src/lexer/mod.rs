pub mod tokens;

use std::iter::Peekable;
use std::str::Chars;
use crate::lexer::tokens::{Loc, Token, TokenType};

pub struct Lexer<'a> {
    input: Peekable<Chars<'a>>,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input: input.chars().peekable(), line: 1, col: 1 }
    }

    fn advance(&mut self) {
        if let Some(c) = self.input.next() {
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }

    fn peek(&mut self) -> Option<&char> {
        self.input.peek()
    }

    fn current_loc(&self) -> Loc {
        Loc { line: self.line, col: self.col }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        let mut is_start_of_line = true;

        while self.peek().is_some() {
            if is_start_of_line && *self.peek().unwrap() != '\n' {
                self.handle_indentation(&mut tokens)?;
                is_start_of_line = false;
                continue;
            }

            let loc = self.current_loc();
            let c = *self.peek().unwrap();

            // Skip comments
            if c == '#' {
                self.skip_comment();
                continue;
            }

            match c {
                ':' => {
                    tokens.push(Token { kind: TokenType::Colon, loc });
                    self.advance();
                }
                '-' => self.handle_dash(&mut tokens, loc)?,
                '[' => {
                    tokens.push(Token { kind: TokenType::LBracket, loc });
                    self.advance();
                }
                ']' => {
                    tokens.push(Token { kind: TokenType::RBracket, loc });
                    self.advance();
                }
                ',' => {
                    tokens.push(Token { kind: TokenType::Comma, loc });
                    self.advance();
                }
                '\n' => {
                    tokens.push(Token { kind: TokenType::Newline, loc });
                    self.advance();
                    is_start_of_line = true;
                }
                q if q == '"' || q == '\'' => self.handle_quoted_string(&mut tokens, loc, q),
                c if c.is_whitespace() => {
                    self.advance();
                }
                _ => self.handle_text_or_number(&mut tokens, loc)?,
            }
        }

        Ok(tokens)
    }

    fn handle_indentation(&mut self, tokens: &mut Vec<Token>) -> Result<(), String> {
        let loc = self.current_loc();
        let mut spaces = 0;

        while let Some(&w) = self.peek() {
            match w {
                ' ' => {
                    spaces += 1;
                    self.advance();
                }
                '\t' => {
                    spaces += 4; // Tab counts as 4 spaces
                    self.advance();
                }
                _ => break,
            }
        }

        // Emit indent only if relevant content follows
        if let Some(&next) = self.peek() {
            if next != '\n' && next != '#' {
                tokens.push(Token { kind: TokenType::Indent(spaces), loc });
            }
        }

        Ok(())
    }

    fn skip_comment(&mut self) {
        while let Some(&c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn handle_dash(&mut self, tokens: &mut Vec<Token>, loc: Loc) -> Result<(), String> {
        self.advance(); // Consume the dash

        let next_is_separator = match self.peek() {
            Some(n) => n.is_whitespace(),
            None => true,
        };

        if next_is_separator {
            tokens.push(Token { kind: TokenType::Dash, loc });
        } else {
            // Part of a text token like "-flag"
            let mut val = String::from("-");
            while let Some(&n) = self.peek() {
                if n.is_alphanumeric() || "._-/".contains(n) {
                    val.push(n);
                    self.advance();
                } else {
                    break;
                }
            }
            tokens.push(Token { kind: TokenType::Text(val), loc });
        }

        Ok(())
    }

    fn handle_quoted_string(&mut self, tokens: &mut Vec<Token>, loc: Loc, quote_char: char) {
        self.advance(); // Consume opening quote
        let mut val = String::new();

        while let Some(&c) = self.peek() {
            if c == quote_char {
                self.advance();
                break;
            }
            val.push(c);
            self.advance();
        }

        tokens.push(Token { kind: TokenType::StringLit(val), loc });
    }

    fn handle_text_or_number(&mut self, tokens: &mut Vec<Token>, loc: Loc) -> Result<(), String> {
        let mut val = String::new();

        while let Some(&n) = self.peek() {
            if n.is_alphanumeric() || "._-/".contains(n) {
                val.push(n);
                self.advance();
            } else {
                break;
            }
        }

        if val.is_empty() {
            let c = *self.peek().unwrap();
            return Err(format!(
                "Unexpected character: '{}' at line {}, col {}",
                c, self.line, self.col
            ));
        }

        if let Ok(num) = val.parse::<u64>() {
            tokens.push(Token { kind: TokenType::Number(num), loc });
        } else {
            tokens.push(Token { kind: TokenType::Text(val), loc });
        }

        Ok(())
    }
}