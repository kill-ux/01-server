use std::collections::BTreeMap;

use crate::lexer::{LexerError, Token, Tokenizer};

pub mod lexer;

#[derive(Debug)]
pub enum YamlValue<'a> {
    Map(BTreeMap<&'a str, YamlValue<'a>>),
    List(Vec<YamlValue<'a>>),
    Scalar(&'a str),
}

impl<'a> YamlValue<'a> {
    pub fn get(&self, key: &str) -> Option<&YamlValue<'a>> {
        if let YamlValue::Map(m) = self {
            m.get(key)
        } else {
            None
        }
    }

    pub fn index(&self, i: usize) -> Option<&YamlValue<'a>> {
        if let YamlValue::List(l) = self {
            l.get(i)
        } else {
            None
        }
    }
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
        println!("DEBUG: Next token is {:?}", self.lookahead); // Uncomment this
        Ok(())
    }

    pub fn parse_all(&mut self) -> Result<YamlValue<'a>, String> {
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
            self.advance().map_err(|e| format!("{:?}", e))?; // Consume '-'

            // Parse the value of the list item (usually a Map)
            items.push(self.parse_value(list_indent)?);

            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            if let Token::Indent(n) = self.lookahead {
                if n == list_indent {
                    // If we are at the same indent, we MUST check if it's a Dash.
                    // If your tokenizer can't peek, we have to advance.
                    self.advance().map_err(|e| format!("{:?}", e))?;
                    if matches!(self.lookahead, Token::Dash) {
                        continue;
                    } else {
                        // This is a key for the parent map, not a dash.
                        // IMPORTANT: Your logic must account for this "consumed" token.
                        break;
                    }
                } else {
                    break; // Indent decreased, list is over.
                }
            } else {
                break;
            }
        }
        Ok(YamlValue::List(items))
    }

    // pub fn parse_list(&mut self, list_indent: usize) -> Result<YamlValue<'a>, String> {
    //     let mut items = Vec::new();

    //     loop {
    //         // 1. We MUST be on a Dash to start/continue a list item
    //         if !matches!(self.lookahead, Token::Dash) {
    //             break;
    //         }
    //         self.advance().map_err(|e| format!("{:?}", e))?; // Consume '-'

    //         // 2. Parse the value.
    //         // We pass 'list_indent + 2' (usually) or let parse_value find the next Indent.
    //         items.push(self.parse_value(list_indent)?);

    //         // 3. Look for the next item
    //         self.skip_junk().map_err(|e| format!("{:?}", e))?;

    //         if let Token::Indent(n) = self.lookahead {
    //             if n == list_indent {
    //                 self.advance().map_err(|e| format!("{:?}", e))?;
    //                 // If the next thing after indent is a Dash, we continue the list
    //                 if !matches!(self.lookahead, Token::Dash) {
    //                     break;
    //                 }
    //             } else {
    //                 // Indent doesn't match the list start, so the list is over
    //                 break;
    //             }
    //         } else if !matches!(self.lookahead, Token::Dash) {
    //             break;
    //         }
    //     }
    //     Ok(YamlValue::List(items))
    // }

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

    // pub fn parse_map(
    //     &mut self,
    //     first_key: &'a str,
    //     _caller_indent: usize,
    // ) -> Result<YamlValue<'a>, String> {
    //     let mut map = BTreeMap::new();
    //     let mut key = first_key;
    //     let mut map_level: Option<usize> = None;

    //     loop {
    //         // 1. Process the Colon for the current key
    //         if !matches!(self.lookahead, Token::Colon) {
    //             return Err(format!(
    //                 "Expected ':' after key '{}', found {:?}",
    //                 key, self.lookahead
    //             ));
    //         }
    //         self.advance().map_err(|e| format!("{:?}", e))?;

    //         // 2. Parse Value
    //         let value = match &self.lookahead {
    //             Token::Indent(n) => {
    //                 let n_val = *n;
    //                 self.advance().map_err(|e| format!("{:?}", e))?;
    //                 self.parse_value(n_val)?
    //             }
    //             _ => self.parse_value(0)?,
    //         };
    //         map.insert(key, value);

    //         // 3. Look for the next key
    //         // Crucial: Use a peek-like strategy or be very careful with advances
    //         if matches!(self.lookahead, Token::NewLine) {
    //             self.advance().map_err(|e| format!("{:?}", e))?;
    //         }

    //         if let Token::Indent(n) = self.lookahead {
    //             let current_n = n;
    //             if map_level.is_none() {
    //                 map_level = Some(current_n);
    //             }

    //             if Some(current_n) == map_level {
    //                 self.advance().map_err(|e| format!("{:?}", e))?; // Consume Indent
    //                 if let Token::Identifier(next_key) = self.lookahead {
    //                     key = next_key;
    //                     self.advance().map_err(|e| format!("{:?}", e))?; // Consume Identifier
    //                     // Now the loop restarts, and lookahead is the Colon. Perfect.
    //                     continue;
    //                 }
    //             }
    //         }
    //         break;
    //     }
    //     Ok(YamlValue::Map(map))
    // }

    pub fn parse_map(
        &mut self,
        first_key: &'a str,
        indent: usize,
    ) -> Result<YamlValue<'a>, String> {
        let mut map = BTreeMap::new();
        let mut current_key = first_key;

        loop {
            // 1. We are sitting on the key name. Next must be Colon.
            if !matches!(self.lookahead, Token::Colon) {
                return Err(format!(
                    "Expected ':' after '{}', found {:?}",
                    current_key, self.lookahead
                ));
            }
            self.advance().map_err(|e| format!("{:?}", e))?; // Consume ':'

            // 2. Parse the value.
            // We skip junk to see if the value is on a new line (Indent) or same line.
            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            let value = if let Token::Indent(n) = self.lookahead {
                let inner_indent = n;
                self.advance().map_err(|e| format!("{:?}", e))?; // Consume Indent
                self.parse_value(inner_indent)?
            } else {
                self.parse_value(indent)?
            };
            map.insert(current_key, value);

            // 3. KEY PART: Peek for the next key at OUR level
            // We skip junk (newlines) to find the next Indent token
            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            if let Token::Indent(n) = self.lookahead {
                if n == indent {
                    self.advance().map_err(|e| format!("{:?}", e))?; // Consume Indent
                    if let Token::Identifier(next_k) = self.lookahead {
                        current_key = next_k;
                        self.advance().map_err(|e| format!("{:?}", e))?; // Consume Key
                        continue; // Continue the map loop
                    }
                }
            }

            // If indentation is less, we break and let the caller handle it.
            break;
        }
        Ok(YamlValue::Map(map))
    }
}
