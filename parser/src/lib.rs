use std::collections::BTreeMap;

use crate::lexer::{LexerError, Token, Tokenizer};

pub mod lexer;

#[derive(Debug)]
pub enum YamlValue<'a> {
    Map(BTreeMap<&'a str, YamlValue<'a>>),
    List(Vec<YamlValue<'a>>),
    Scalar(&'a str),
}

pub struct Parser<'a> {
    pub tokenizer: Tokenizer<'a>,
    pub lookahead: Token<'a>,
    pub indent_stack: Vec<usize>,
}

impl<'a> Parser<'a> {
    pub fn new(mut tokenizer: Tokenizer<'a>) -> Result<Self, LexerError> {
        let first = tokenizer.next_token()?;
        Ok(Self {
            tokenizer: tokenizer,
            lookahead: first,
            indent_stack: vec![0],
        })
    }

    fn skip_junk(&mut self) -> Result<(), LexerError> {
        while matches!(self.lookahead, Token::NewLine | Token::Indent(_)) {
            self.advance()?;
        }
        Ok(())
    }

    fn advance(&mut self) -> Result<(), LexerError> {
        self.lookahead = self.tokenizer.next_token()?;
        Ok(())
    }

    pub fn parse(&mut self) -> Result<YamlValue<'a>, String> {
        self.parse_value(0)
    }

    pub fn parse_value(&mut self, current_indent: usize) -> Result<YamlValue<'a>, String> {
        // Skip any unnecessary newlines or indents that don't change scope
        self.skip_junk().map_err(|e| format!("{:?}", e))?;

        match &self.lookahead {
            Token::Dash => self.parse_list(current_indent),

            Token::OpenBracket => self.parse_inline_list(), // For [8080]

            Token::Identifier(s) => {
                let val = *s;
                self.advance().map_err(|e| format!("{:?}", e))?;

                if matches!(self.lookahead, Token::Colon) {
                    self.parse_map(val, current_indent)
                } else {
                    Ok(YamlValue::Scalar(val))
                }
            }

            Token::Scalar(s) => {
                let val = *s;
                self.advance().map_err(|e| format!("{:?}", e))?;
                Ok(YamlValue::Scalar(val))
            }

            _ => Err(format!("Expected value, found {:?}", self.lookahead)),
        }
    }

    pub fn parse_list(&mut self, list_indent: usize) -> Result<YamlValue<'a>, String> {
        let mut items = Vec::new();

        loop {
            if !matches!(self.lookahead, Token::Dash) {
                break;
            }
            self.advance().map_err(|e| format!("{:?}", e))?;

            // After a dash, we might have an Indent token if the value is on a new line
            // Or we might have the value immediately.
            let item_indent = if let Token::Indent(n) = self.lookahead {
                let n_val = n;
                self.advance().map_err(|e| format!("{:?}", e))?;
                n_val
            } else {
                list_indent
            };

            items.push(self.parse_value(item_indent)?);

            // Look ahead for the next item.
            // It must be at the same indentation level.
            if let Token::Indent(n) = self.lookahead {
                if n == list_indent {
                    self.advance().map_err(|e| format!("{:?}", e))?;
                } else {
                    break; // Indentation changed, list is over
                }
            } else if !matches!(self.lookahead, Token::Dash) {
                break;
            }
        }

        Ok(YamlValue::List(items))
    }

    fn parse_inline_list(&mut self) -> Result<YamlValue<'a>, String> {
        self.advance().map_err(|e| format!("{:?}", e))?;
        let mut items = Vec::new();
        while !matches!(self.lookahead, Token::CloseBracket)
            && !matches!(self.lookahead, Token::Eof)
        {
            items.push(self.parse_value(0)?);

            if matches!(self.lookahead, Token::Comma) {
                self.advance().map_err(|e| format!("{:?}", e))?;
            }
        }

        self.advance().map_err(|e| format!("{:?}", e))?;
        Ok(YamlValue::List(items))
    }

    pub fn parse_map(
        &mut self,
        first_key: &'a str,
        parent_indent: usize,
    ) -> Result<YamlValue<'a>, String> {
        let mut map = BTreeMap::new();
        let mut key = first_key;

        loop {
            if !matches!(self.lookahead, Token::Colon) {
                return Err(format!("Expected ':' after key '{}'", key));
            }
            self.advance().map_err(|e| format!("{:?}", e))?;

            let value = match &self.lookahead {
                Token::Indent(n) if *n > parent_indent => {
                    let new_indent = *n;
                    self.advance().map_err(|e| format!("{:?}", e))?;
                    self.parse_value(new_indent)?
                }
                _ => {
                    // It's an inline value (on the same line)
                    self.parse_value(parent_indent)?
                }
            };

            map.insert(key, value);

            // 3. Check for the next sibling key
            // We need to look for an Indent token that matches our parent_indent
            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            // This part is tricky: we need to peek if the NEXT line
            // is at the same indentation level to continue this map.
            if let Token::Indent(n) = self.lookahead {
                if n == parent_indent {
                    self.advance().map_err(|e| format!("{:?}", e))?;
                    if let Token::Identifier(next_key) = self.lookahead {
                        key = next_key;
                        self.advance().map_err(|e| format!("{:?}", e))?;
                        continue; // Found another key at same level, loop again
                    }
                }
            }

            break; // Indentation dropped or no more identifiers, map is done
        }

        Ok(YamlValue::Map(map))
    }
}
