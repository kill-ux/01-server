pub struct Token {
    pub value: usize,
}

impl Token {
    pub fn new() -> Token {
        Token {
            value: 0,
        }
    }

    pub fn next(&self) -> usize {
        return self.value + 1;
    }
}
