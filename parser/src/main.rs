// use parser::lexer::TokenKind;

use parser::{lexer::{LexerError, Token, Tokenizer}, Parser};

fn main() -> Result<(), LexerError> {
    let yaml = r#"
servers:
 - "host": 127.0.0.1                
   ports:
    - 8081
    - 9000"#;
    let mut tokenizer = Tokenizer::new(yaml);
    let mut parser = Parser::new(tokenizer)?;
    let res = parser.parse_all();
    dbg!(res);
    Ok(())
}
