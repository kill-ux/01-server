pub mod from_yaml;
pub use from_yaml::*;
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
    pub fn new(source: &'a str) -> Result<Self, LexerError> {
        let mut tokenizer = Tokenizer::new(source);
        let first = tokenizer.next_token()?;
        Ok(Self {
            tokenizer,
            lookahead: first,
            indent_stack: vec![0],
        })
    }

    fn skip_junk(&mut self) -> Result<(), LexerError> {
        while let Token::NewLine = self.lookahead {
            self.advance()?
        }
        Ok(())
    }

    fn advance(&mut self) -> Result<(), LexerError> {
        self.lookahead = self.tokenizer.next_token()?;
        // println!("DEBUG: Next token is {:?}", self.lookahead); // Uncomment this
        Ok(())
    }

    pub fn parse(&mut self) -> Result<YamlValue<'a>, String> {
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
                    if matches!(self.lookahead, Token::Dash) {
                        return self.parse_list(n_val, current_indent);
                    }
                    return self.parse_value(n_val);
                    // return self.parse_value(n_val);
                }
                // If it's a dedent or sibling, we stop here.
                // This allows the parent map/list to see the Indent token.
                Ok(YamlValue::Scalar(""))
            }
            Token::Dash => self.parse_list(current_indent, current_indent),

            // ADD THIS: Missing link to your inline list parser
            Token::OpenBracket => self.parse_bracket_list(),
            Token::OpenBrace => self.parse_brace_map(),

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

    pub fn parse_list(
        &mut self,
        list_indent: usize,
        parent_indent: usize,
    ) -> Result<YamlValue<'a>, String> {
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
                    // Perfect alignment!
                    self.advance().map_err(|e| format!("{:?}", e))?;
                    if matches!(self.lookahead, Token::Dash) {
                        continue;
                    } else {
                        return Err(format!(
                            "Expected '-' for list item, found {:?}",
                            self.lookahead
                        ));
                    }
                } else if n_val <= parent_indent {
                    // This is a dedent, the list has ended.
                    break;
                } else {
                    return Err(format!(
                        "Indentation Error: Sequence items must start at the same column (expected {}, found {})",
                        list_indent, n_val
                    ));
                }
            } else if !matches!(self.lookahead, Token::Dash) {
                break;
            }
        }
        Ok(YamlValue::List(items))
    }

    pub fn parse_brace_map(&mut self) -> Result<YamlValue<'a>, String> {
        self.advance().map_err(|e| format!("{:?}", e))?;
        let mut map = BTreeMap::new();
        while !matches!(self.lookahead, Token::CloseBrace) && !matches!(self.lookahead, Token::Eof)
        {
            if matches!(self.lookahead, Token::Indent(_))
                || matches!(self.lookahead, Token::NewLine)
            {
                self.advance().map_err(|e| format!("{:?}", e))?;
                continue;
            }

            let key = match self.lookahead {
                Token::Identifier(s) => {
                    let key = s;
                    self.advance().map_err(|e| format!("{:?}", e))?;
                    key
                }
                _ => return Err("Expected identifier".into()),
            };

            if !matches!(self.lookahead, Token::Colon) {
                return Err("Expected colon".into());
            }
            self.advance().map_err(|e| format!("{:?}", e))?;

            let value = self.parse_value(0)?;
            map.insert(key, value);

            if matches!(self.lookahead, Token::Comma) {
                self.advance().map_err(|e| format!("{:?}", e))?;
                while matches!(self.lookahead, Token::Indent(_))
                    || matches!(self.lookahead, Token::NewLine)
                {
                    self.advance().map_err(|e| format!("{:?}", e))?;
                }
            }
        }

        if !matches!(self.lookahead, Token::CloseBrace) {
            return Err("Expected closing brace '}'".into());
        }

        self.advance().map_err(|e| format!("{:?}", e))?;
        Ok(YamlValue::Map(map))
    }

    fn parse_bracket_list(&mut self) -> Result<YamlValue<'a>, String> {
        self.advance().map_err(|e| format!("{:?}", e))?;

        let mut items = Vec::new();
        while !matches!(self.lookahead, Token::CloseBracket)
            && !matches!(self.lookahead, Token::Eof)
        {
            if matches!(self.lookahead, Token::Indent(_))
                || matches!(self.lookahead, Token::NewLine)
            {
                self.advance().map_err(|e| format!("{:?}", e))?;
                continue;
            }

            items.push(self.parse_value(0)?);

            if matches!(self.lookahead, Token::Comma) {
                self.advance().map_err(|e| format!("{:?}", e))?;

                while matches!(self.lookahead, Token::Indent(_))
                    || matches!(self.lookahead, Token::NewLine)
                {
                    self.advance().map_err(|e| format!("{:?}", e))?;
                }
            }
        }

        if !matches!(self.lookahead, Token::CloseBracket) {
            return Err("Expected closing bracket ']'".into());
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

            let value = self.parse_value(map_indent)?;

            if !map.insert(current_key, value).is_none() {
                return Err(format!("Duplicate key found: {}", current_key));
            }

            // 3. Look for the next sibling key
            self.skip_junk().map_err(|e| format!("{:?}", e))?;

            if let Token::Indent(n) = self.lookahead {
                if n == map_indent {
                    // Perfect alignment!
                    self.advance().map_err(|e| format!("{:?}", e))?; // Consume the Indent

                    match self.lookahead {
                        Token::Identifier(s) => {
                            current_key = s;
                            self.advance().map_err(|e| format!("{:?}", e))?; // Consume the Key
                            continue;
                        }
                        _ => {
                            return Err(format!(
                                "Expected identifier for map key, found {:?}",
                                self.lookahead
                            ));
                        }
                    }
                } else if n > map_indent {
                    return Err(format!(
                        "Indentation Error: Map keys must align at the same column (expected {}, found {})",
                        map_indent, n
                    ));
                } else {
                    // Dedent or end of map
                    break;
                }
            }

            break;
        }
        Ok(YamlValue::Map(map))
    }
}

// map.insert(current_key, value);

// 2. Determine the value
// We look ahead to see if the value is nested (greater indent)
// let value = if let Token::Indent(n) = self.lookahead {
//     dbg!(&n, &map_indent, &current_key);
//     if n > map_indent {
//         // Nested content! Consume indent and parse
//         let next_lvl = n;
//         self.advance().map_err(|e| format!("{:?}", e))?;
//         self.parse_value(next_lvl)?
//     } else {
//         // Sibling or Dedent. The value for this key is effectively null/empty
//         YamlValue::Scalar("")
//     }
// } else {
//     self.parse_value(map_indent)?
// };
