#[derive(Debug, PartialEq)]
pub enum Token {
    Eof,
    Number(usize),
    Identifire(String),
    Indent(usize),

    OpenBracket,
    CloseBracket,
    OpenCurly,
    CloseCurly,

    Colon,
    Comma,
    Dash,
    Space(usize),
    Hash,
    NewLine,
    Scalar(String)
}

// pub struct Token {
//     pub kind: TokenKind,
//     pub value: String,
// }

impl Token {
    // pub fn new(kind: TokenKind, value: String) -> Self {
    //     Self { kind, value }
    // }
}
