pub mod from_yaml;
pub use from_yaml::*;
use std::{collections::BTreeMap, fmt::Debug};

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

use std::fmt;

// #[derive(Debug)]
pub enum YamlError {
    Lexer(LexerError),
    UnexpectedToken { expected: String, found: String },
    Indentation { expected: usize, found: usize },
    DuplicateKey(String),
    ForbiddenBlock(String),
    Generic(String),
}

impl std::error::Error for YamlError {}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            YamlError::Lexer(e) => write!(f, "Lexer error: {:?}", e),
            YamlError::UnexpectedToken { expected, found } => {
                write!(f, "Syntax Error: Expected {}, found {}", expected, found)
            }
            YamlError::Indentation { expected, found } => {
                write!(
                    f,
                    "Indentation Error: Expected {}, found {}",
                    expected, found
                )
            }
            YamlError::DuplicateKey(k) => write!(f, "Duplicate key found: '{}'", k),
            YamlError::ForbiddenBlock(k) => {
                write!(f, "Forbidden block value on same line as key: '{}'", k)
            }
            YamlError::Generic(s) => write!(f, "Error: {}", s),
        }
    }
}

impl Debug for YamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "4444{}", self)
    }
}

// Allow automatic conversion from LexerError to YamlError
impl From<LexerError> for YamlError {
    fn from(err: LexerError) -> Self {
        YamlError::Lexer(err)
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

    pub fn parse(&mut self) -> Result<YamlValue<'a>, YamlError> {
        self.skip_junk()?;

        // If the file starts with an Indent, consume it before parsing the first value
        if let Token::Indent(n) = self.lookahead {
            let start_indent = n;
            self.advance()?;
            self.parse_value(start_indent)
        } else {
            self.parse_value(0)
        }
    }

    pub fn parse_value(&mut self, current_indent: usize) -> Result<YamlValue<'a>, YamlError> {
        // 1. Skip junk (NewLines)
        self.skip_junk()?;

        match &self.lookahead {
            Token::Indent(n) => {
                let n_val = *n;
                // If the indent is deeper than our current scope, it's a new block (Map/List)
                if n_val > current_indent {
                    self.advance()?;
                    match &self.lookahead {
                        Token::Dash => return self.parse_list(n_val, current_indent),
                        Token::Identifier(s) => {
                            let key = *s;
                            self.advance()?;
                            return self.parse_map(key, n_val);
                        }
                        _ => return self.parse_value(n_val),
                    }
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
                self.advance()?;
                if matches!(self.lookahead, Token::Colon) {
                    // If it's a key: value pair, start a map
                    self.parse_map(val, current_indent)
                } else {
                    // if just one identifier, treat as scalar if more then one word throw error

                    // Ok(YamlValue::Scalar(val))
                    let mut word_count = 1;
                    while let Token::Identifier(next_part) = self.lookahead {
                        word_count += 1;
                        if word_count > 1 {
                            return Err(YamlError::Generic(format!(
                                "Multiple words found for scalar value: '{} {}'",
                                val, next_part
                            )));
                        }
                        self.advance()?;
                    }
                    Ok(YamlValue::Scalar(val)) // Always return as scalar for now)
                }
            }
            Token::Scalar(s) => {
                let val = *s;
                self.advance()?;
                Ok(YamlValue::Scalar(val))
            }
            Token::Eof => Ok(YamlValue::Scalar("")),
            _ => Err(YamlError::UnexpectedToken {
                expected: "value".to_string(),
                found: format!("{:?}", self.lookahead),
            }),
        }
    }

    pub fn parse_list(
        &mut self,
        list_indent: usize,
        parent_indent: usize,
    ) -> Result<YamlValue<'a>, YamlError> {
        let mut items = Vec::new();

        loop {
            if !matches!(self.lookahead, Token::Dash) {
                break;
            }
            self.advance()?; // Consume '-'
            // Parse the value of the list item
            items.push(self.parse_value(list_indent + 2)?);
            // Peek for next item
            self.skip_junk()?;

            if let Token::Indent(n) = self.lookahead {
                let n_val = n;

                if n_val == list_indent {
                    // Perfect alignment!
                    self.advance()?;
                    if matches!(self.lookahead, Token::Dash) {
                        continue;
                    } else {
                        return Err(YamlError::UnexpectedToken {
                            expected: "-".to_string(),
                            found: format!("{:?}", self.lookahead),
                        });
                    }
                } else if n_val <= parent_indent {
                    // This is a dedent, the list has ended.
                    break;
                } else {
                    return Err(YamlError::Indentation {
                        expected: list_indent,
                        found: n,
                    });
                }
            } else if !matches!(self.lookahead, Token::Dash) {
                break;
            }
        }
        Ok(YamlValue::List(items))
    }

    pub fn parse_brace_map(&mut self) -> Result<YamlValue<'a>, YamlError> {
        self.advance()?;
        let mut map = BTreeMap::new();
        while !matches!(self.lookahead, Token::CloseBrace) && !matches!(self.lookahead, Token::Eof)
        {
            if matches!(self.lookahead, Token::Indent(_))
                || matches!(self.lookahead, Token::NewLine)
            {
                self.advance()?;
                continue;
            }

            let key = match self.lookahead {
                Token::Identifier(s) => {
                    let key = s;
                    self.advance()?;
                    key
                }
                _ => {
                    return Err(YamlError::UnexpectedToken {
                        expected: "identifier".to_string(),
                        found: format!("{:?}", self.lookahead),
                    });
                }
            };

            if !matches!(self.lookahead, Token::Colon) {
                return Err(YamlError::UnexpectedToken {
                    expected: ":".to_string(),
                    found: format!("{:?}", self.lookahead),
                });
            }
            self.advance()?;

            let value = self.parse_value(0)?;
            map.insert(key, value);

            if matches!(self.lookahead, Token::Comma) {
                self.advance()?;
                while matches!(self.lookahead, Token::Indent(_))
                    || matches!(self.lookahead, Token::NewLine)
                {
                    self.advance()?;
                }
            }
        }

        if !matches!(self.lookahead, Token::CloseBrace) {
            return Err(YamlError::UnexpectedToken {
                expected: "}".to_string(),
                found: format!("{:?}", self.lookahead),
            });
        }

        self.advance()?;
        Ok(YamlValue::Map(map))
    }

    fn parse_bracket_list(&mut self) -> Result<YamlValue<'a>, YamlError> {
        self.advance()?;

        let mut items = Vec::new();
        while !matches!(self.lookahead, Token::CloseBracket)
            && !matches!(self.lookahead, Token::Eof)
        {
            if matches!(self.lookahead, Token::Indent(_))
                || matches!(self.lookahead, Token::NewLine)
            {
                self.advance()?;
                continue;
            }

            items.push(self.parse_value(0)?);

            if matches!(self.lookahead, Token::Comma) {
                self.advance()?;

                while matches!(self.lookahead, Token::Indent(_))
                    || matches!(self.lookahead, Token::NewLine)
                {
                    self.advance()?;
                }
            }
        }

        if !matches!(self.lookahead, Token::CloseBracket) {
            return Err(YamlError::UnexpectedToken {
                expected: "]".to_string(),
                found: format!("{:?}", self.lookahead),
            });
        }

        self.advance()?;
        Ok(YamlValue::List(items))
    }

    pub fn parse_map(
        &mut self,
        first_key: &'a str,
        map_indent: usize, // The indent level of the keys in this map
    ) -> Result<YamlValue<'a>, YamlError> {
        let mut map = BTreeMap::new();
        let mut current_key = first_key;

        loop {
            // 1. Expect Colon after the key
            if !matches!(self.lookahead, Token::Colon) {
                return Err(YamlError::UnexpectedToken {
                    expected: ":".to_string(),
                    found: format!("{:?}", self.lookahead),
                });
            }
            self.advance()?; // Consume ':'

            if matches!(self.lookahead, Token::Dash) {
                return Err(YamlError::UnexpectedToken {
                    expected: "new line".to_string(),
                    found: format!("{:?}", self.lookahead),
                });
            }

            self.skip_junk()?;

            let value = self.parse_value(map_indent)?;

            if !map.insert(current_key, value).is_none() {
                return Err(YamlError::DuplicateKey(current_key.to_string()));
            }

            // 3. Look for the next sibling key
            self.skip_junk()?;

            if let Token::Indent(n) = self.lookahead {
                if n == map_indent {
                    // Perfect alignment!
                    self.advance()?; // Consume the Indent

                    match self.lookahead {
                        Token::Identifier(s) => {
                            current_key = s;
                            self.advance()?; // Consume the Key
                            continue;
                        }
                        _ => {
                            return Err(YamlError::UnexpectedToken {
                                expected: "identifier".to_string(),
                                found: format!("{:?}", self.lookahead),
                            });
                        }
                    }
                } else if n > map_indent {
                    return Err(YamlError::UnexpectedToken {
                        expected: "indentation".to_string(),
                        found: format!("indentation level {}", n),
                    });
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
