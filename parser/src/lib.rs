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

    // fn skip_junk(&mut self) -> Result<(), LexerError> {
    //     while matches!(self.lookahead, Token::NewLine | Token::Indent(_)) {
    //         self.advance()?;
    //     }
    //     Ok(())
    // }

    fn skip_junk(&mut self) -> Result<(), LexerError> {
        loop {
            match self.lookahead {
                Token::NewLine => self.advance()?,
                // Only skip Indents if they are followed by another NewLine or a Comment
                // This is tricky. A better way is to fix the Lexer to ignore comments.
                _ => break,
            }
        }
        Ok(())
    }

    fn advance(&mut self) -> Result<(), LexerError> {
        self.lookahead = self.tokenizer.next_token()?;
        println!("DEBUG: Next token is {:?}", self.lookahead); // Uncomment this
        Ok(())
    }

    pub fn parse_all(&mut self) -> Result<YamlValue<'a>, String> {
        self.skip_junk().map_err(|e| format!("{:?}", e))?;

        // If the file starts with an Indent, consume it before parsing the first value
        if let Token::Indent(n) = self.lookahead {
            let start_indent = n;
            self.advance().map_err(|e| format!("{:?}", e))?;
            self.parse_value(start_indent)
        } else {
            self.parse_value(0)
        }
    }

    pub fn parse_value(&mut self, current_indent: usize) -> Result<YamlValue<'a>, String> {
        // 1. Skip junk (NewLines)
        self.skip_junk().map_err(|e| format!("{:?}", e))?;

        match &self.lookahead {
            Token::Indent(n) => {
                let n_val = *n;
                // If the indent is deeper than our current scope, it's a new block (Map/List)
                if n_val > current_indent {
                    self.advance().map_err(|e| format!("{:?}", e))?; // Consume the indent
                    return self.parse_value(n_val);
                }
                // If it's a dedent or sibling, we stop here.
                // This allows the parent map/list to see the Indent token.
                Ok(YamlValue::Scalar(""))
            }
            Token::Dash => self.parse_list(current_indent),

            // ADD THIS: Missing link to your inline list parser
            Token::OpenBracket => self.parse_inline_list(),

            Token::Identifier(s) => {
                let val = *s;
                self.advance().map_err(|e| format!("{:?}", e))?;
                if matches!(self.lookahead, Token::Colon) {
                    // If it's a key: value pair, start a map
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
            // Parse the value of the list item
            items.push(self.parse_value(list_indent + 2)?);
            // Peek for next item
            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            if let Token::Indent(n) = self.lookahead {
                let n_val = n;
                if n_val == list_indent {
                    self.advance().map_err(|e| format!("{:?}", e))?; // Consume indent
                    if matches!(self.lookahead, Token::Dash) {
                        continue; // Correctly found next '-'
                    } else {
                        break; // Same indent but not a dash -> parent map key
                    }
                } else if n_val > list_indent {
                    // This handles cases where extra indents/comments exist
                    continue;
                } else {
                    break; // Dedent
                }
            } else if !matches!(self.lookahead, Token::Dash) {
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

    pub fn parse_map(
        &mut self,
        first_key: &'a str,
        map_indent: usize, // The indent level of the keys in this map
    ) -> Result<YamlValue<'a>, String> {
        let mut map = BTreeMap::new();
        let mut current_key = first_key;

        loop {
            // 1. Expect Colon after the key
            if !matches!(self.lookahead, Token::Colon) {
                return Err(format!(
                    "Expected ':' after '{}', found {:?}",
                    current_key, self.lookahead
                ));
            }
            self.advance().map_err(|e| format!("{:?}", e))?; // Consume ':'
            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            // 2. Determine the value
            // We look ahead to see if the value is nested (greater indent)
            let value = if let Token::Indent(n) = self.lookahead {
                if n > map_indent {
                    // Nested content! Consume indent and parse
                    let next_lvl = n;
                    self.advance().map_err(|e| format!("{:?}", e))?;
                    self.parse_value(next_lvl)?
                } else {
                    // Sibling or Dedent. The value for this key is effectively null/empty
                    YamlValue::Scalar("")
                }
            } else {
                // Value is on the same line
                self.parse_value(map_indent)?
            };

            map.insert(current_key, value);

            // 3. Look for the next sibling key
            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            if let Token::Indent(n) = self.lookahead {
                if n == map_indent {
                    // This is a sibling key at the same level (e.g., another key at Indent 6)
                    self.advance().map_err(|e| format!("{:?}", e))?; // Consume the Indent

                    if let Token::Identifier(next_k) = self.lookahead {
                        current_key = next_k;
                        self.advance().map_err(|e| format!("{:?}", e))?; // Consume the Key
                        continue; // Loop back to handle the Colon
                    }
                }
            }

            // If we reach here, the next token is not an indent at our level.
            // We BREAK and let the parent handle the dedent.
            break;
        }
        Ok(YamlValue::Map(map))
    }
}
