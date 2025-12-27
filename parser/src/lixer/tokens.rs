pub enum TokenKind {
    Eof,
    
    Number,
    String,
    Identifire,

    OpenBracket,
    CloseBracket,

    key,
    value,

    Colon,
    comma,
}

pub struct Token {
    pub kind: TokenKind,
    pub value: String,
}
