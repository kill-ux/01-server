use std::fmt::{Display, Formatter, Result};

#[derive(Debug, PartialEq, Clone)]
pub enum TokenType {
    Text(String),        // host, 127.0.0.1
    StringLit(String),   // "host", "GET"
    Number(u64),         // 8080
    Colon,               // :
    Dash,                // -
    LBracket,            // [
    RBracket,            // ]
    Comma,               // ,
    Newline,             // \n
    Indent(usize),       // Critical for location blocks
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenType,
    pub loc: Loc,
}

#[derive(Debug, Clone, Copy)]
pub struct Loc {
    pub line: usize,
    pub col: usize,
}



// Display for TokenType
impl Display for TokenType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            TokenType::Text(s) => write!(f, "{s}"),
            TokenType::StringLit(s) => write!(f, "\"{s}\""),
            TokenType::Number(n) => write!(f, "{n}"),
            TokenType::Colon => write!(f, ":"),
            TokenType::Dash => write!(f, "-"),
            TokenType::LBracket => write!(f, "["),
            TokenType::RBracket => write!(f, "]"),
            TokenType::Comma => write!(f, ","),
            TokenType::Newline => write!(f, "\\n"),
            TokenType::Indent(n) => {
                // render as that many spaces
                for _ in 0..*n {
                    write!(f, " ")?;
                }
                Ok(())
            }
        }
    }
}

// Display for Loc (optional but useful)
impl Display for Loc {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "line {}, col {}", self.line, self.col)
    }
}

// Display for Token
impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        // Example format: `StringLit("host") at line 1, col 1`
        write!(f, "'{}' at {}", self.kind, self.loc)
    }
}