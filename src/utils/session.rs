use crate::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Session {
    pub data: HashMap<String, String>,
    pub created_at: u64,
    pub expires_at: u64,
}

impl Session {
    pub fn is_expired(&self, now: u64) -> bool {
        now > self.expires_at
    }

    pub fn new(ttl: u64) -> Self {
        Session {
            data: HashMap::new(),
            created_at: current_timestamp(),
            expires_at: current_timestamp() + ttl,
        }
    }
}

use std::time::{SystemTime, UNIX_EPOCH};

pub struct SessionStore {
    pub sessions: HashMap<String, Session>,
    pub ttl: u64,
    pub counter: u64,
    pub last_cleanup: Instant,
}

impl SessionStore {
    pub fn new(ttl: u64) -> Self {
        SessionStore {
            sessions: HashMap::new(),
            ttl,
            counter: 0,
            last_cleanup: Instant::now(),
        }
    }

    pub fn cleanup(&mut self) {
        let now = current_timestamp();
        self.sessions.retain(|_, s| !s.is_expired(now));
        self.last_cleanup = Instant::now();
    }

    pub fn mange_session_store(&mut self, conn: &mut HttpConnection) {
        let cookies_header = conn.request.headers.get("cookie");
        let cookies = match cookies_header {
            Some(cks) => Cookies::parse(cks),
            None => Cookies::new(),
        };

        let mut valid_session_found = false;

        if let Some(id) = cookies.get("session_id") {
            if let Some(session) = self.sessions.get_mut(id) {
                if !session.is_expired(current_timestamp()) {
                    conn.session_id = Some(id.to_string());
                    valid_session_found = true;
                } else {
                    self.sessions.remove(id);
                }
            }
        }

        if !valid_session_found {
            let new_id = generate_session_id();
            self.sessions.insert(new_id.clone(), Session::new(self.ttl));

            let set_cookie = SetCookie::new("session_id", &new_id)
                .max_age(self.ttl)
                .to_header();

            // conn.response = HttpResponse::new(200, &HttpResponse::status_text(200));

            conn.response
                .headers
                .insert("Set-Cookie".to_string(), set_cookie);


            conn.response
                .set_header("Cache-Control", "no-cache, no-store, must-revalidate");
            conn.response.set_header("Pragma", "no-cache");
            conn.response.set_header("Expires", "0");
            conn.response.set_header("Vary", "Cookie");

            conn.session_id = Some(new_id);
        }
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn generate_session_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();

    // Get seconds and nanos
    let sec = now.as_secs();
    let nano = now.subsec_nanos();

    // Mix them up using bitwise operations to create four "random" segments
    // This isn't cryptographically secure, but it's unique for a school project.
    let part1 = nano;
    let part2 = (nano ^ 0x55555555) >> 8;
    let part3 = (sec & 0xFFFF) as u32;
    let part4 = (nano << 4) ^ (sec as u32);

    format!("{:08x}-{:08x}-{:04x}-{:08x}", part1, part2, part3, part4)
}
