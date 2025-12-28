pub mod tokens;
use std::{iter::Peekable, str::Chars};
pub use tokens::*;

pub struct Tokenizer<'a> {
    source: &'a str,
    chars: Peekable<Chars<'a>>,
    pos: usize,
}

#[derive(Debug, PartialEq)]
pub enum LexerError {
    UnclosedQuote(usize), // Location of the error
    UnexpectedCharacter(char, usize),
}

impl<'a> Tokenizer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.chars().peekable(),
            pos: 0,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token<'a>>, LexerError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if matches!(token, Token::Eof) {
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }

    pub fn next_token(&mut self) -> Result<Token<'a>, LexerError> {
        let start = self.pos;
        let ch = match self.chars.peek() {
            Some(&c) => c,
            None => return Ok(Token::Eof),
        };

        match ch {
            '\n' => {
                self.consume();
                // Skip empty lines and comments until we find content
                self.skip_empty_lines_and_comments();

                // Now we are at the start of a "real" data line
                let spaces = self.count_indentation();
                if let Some(&c) = self.chars.peek() && c != '\n' && c != '#' {
                    Ok(Token::Indent(spaces))
                } else {
                    self.next_token()
                }
            }
            // 2. Handle Inline Comments (middle of a line)
            '#' => {
                self.skip_comment();
                self.next_token()
            }
            white_space if white_space.is_whitespace() => {
                self.consume();
                self.next_token()
            }
            white_space if white_space.is_whitespace() => {
                self.consume();
                self.next_token()
            }
            '"' | '\'' => self.read_quoted_string(ch),
            '-' => {
                self.consume();
                if let Some(&next) = self.chars.peek() {
                    if next == ' ' || next == '\n' {
                        return Ok(Token::Dash);
                    }
                }
                // If it's not a list dash, it's a scalar starting with '-'
                Ok(self.read_identifier_from(start))
            }
            ':' => {
                self.consume();
                if let Some(&next) = self.chars.peek() {
                    if next == ' ' || next == '\n' {
                        return Ok(Token::Colon);
                    }
                }
                Ok(self.read_identifier_from(start))
            }
            ',' => {
                self.consume();
                Ok(Token::Comma)
            }
            '[' => {
                self.consume();
                Ok(Token::OpenBracket)
            }
            ']' => {
                self.consume();
                Ok(Token::CloseBracket)
            }

            _ if ch.is_alphanumeric() || "/._".contains(ch) => Ok(self.read_identifier()),

            _ => Err(LexerError::UnexpectedCharacter(ch, self.pos)),
        }
    }

    fn consume(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_empty_lines_and_comments(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c == '\n' {
                self.consume();
            } else {
                break;
            }
        }
    }

    fn count_indentation(&mut self) -> usize {
        let mut count = 0;
        while let Some(&' ') = self.chars.peek() {
            self.consume();
            count += 1;
        }
        count
    }

    fn read_quoted_string(&mut self, quote: char) -> Result<Token<'a>, LexerError> {
        let start_pos = self.pos;
        self.consume(); // Skip opening quote
        let content_start = self.pos;

        while let Some(&c) = self.chars.peek() {
            if c == quote {
                let content_end = self.pos;
                self.consume(); // Skip closing quote
                return Ok(Token::Identifier(&self.source[content_start..content_end]));
            }
            self.consume();
        }
        // If we reach Eof without finding the closing quote
        Err(LexerError::UnclosedQuote(start_pos))
    }

    fn read_identifier(&mut self) -> Token<'a> {
        let start = self.pos;
        self.read_identifier_from(start)
    }

    // Helper to read identifiers starting from a specific byte offset
    fn read_identifier_from(&mut self, start: usize) -> Token<'a> {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || ":#[],{}".contains(c) {
                break;
            }
            self.consume();
        }
        Token::Identifier(&self.source[start..self.pos])
    }

    fn skip_comment(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c == '\n' {
                break;
            }
            self.consume();
        }
    }
}
