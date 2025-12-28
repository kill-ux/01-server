#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    Eof,
    Number(usize),
    Identifier(&'a str), // Changed from String to &str
    Indent(usize),
    OpenBracket,
    CloseBracket,
    Colon,
    Comma,
    Dash,
    Space(usize),
    Hash,
    NewLine,
    Scalar(&'a str), // Changed from String to &str
}

// pub struct Token {
//     pub kind: TokenKind,
//     pub value: String,
// }


