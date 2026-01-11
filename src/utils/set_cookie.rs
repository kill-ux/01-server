pub struct SetCookie {
    name: String,
    value: String,
    path: String,
    max_age: Option<u64>,
    http_only: bool,
}

impl SetCookie {
    pub fn new(name: &str, value: &str) -> Self {
        SetCookie {
            name: name.to_string(),
            value: value.to_string(),
            path: "/".to_string(),
            max_age: None,
            http_only: true,
        }
    }

    pub fn max_age(mut self, seconds: u64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    pub fn to_header(&self) -> String {
        let mut header = format!("{}={}", self.name, self.value);

        header.push_str(&format!("; Path={}", self.path));

        if let Some(age) = self.max_age {
            header.push_str(&format!("; Max-Age={}", age));
        }

        if self.http_only {
            header.push_str("; HttpOnly");
        }

        header.push_str("; SameSite=Lax");

        header
    }
}
