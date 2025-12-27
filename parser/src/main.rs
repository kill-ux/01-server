// use parser::lexer::TokenKind;

use parser::lexer::{Token, Tokenizer};

fn main() {
    let yaml = r#"
key: value
- item
"#;
    let mut tokenizer = Tokenizer::new(yaml);
    while let Some(token) = tokenizer.next_token() {
        println!("{:?}", token);
        if matches!(token, Token::Eof) {
            break;
        }
    }
}
