use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Cookies {
    values: HashMap<String, String>,
}

impl Cookies {
    pub fn new() -> Self {
        Cookies {
            values: HashMap::new(),
        }
    }

    /// Parse: "a=1; b=hello"
    pub fn parse(header_value: &str) -> Self {
        let mut cookies = Cookies::new();

        for part in header_value.split(';') {
            let part = part.trim();
            if let Some(eq) = part.find('=') {
                let key = &part[..eq];
                let val = &part[eq + 1..];
                cookies.values.insert(key.to_string(), val.to_string());
            }
        }

        cookies
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }
}
