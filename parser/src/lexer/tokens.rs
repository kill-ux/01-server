#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    Eof,
    Number(usize),
    Identifier(&'a str), 
    Indent(usize),
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    Colon,
    Comma,
    Dash,
    Space(usize),
    Hash,
    NewLine,
    Scalar(&'a str), 
}


