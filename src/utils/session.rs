use crate::prelude::*;
use std::clone;
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
    sessions: HashMap<String, Session>,
    ttl: u64,
    counter: u64,
}

impl SessionStore {
    pub fn new(ttl: u64) -> Self {
        SessionStore {
            sessions: HashMap::new(),
            ttl,
            counter: 0,
        }
    }

    // pub fn get_or_create(&mut self, session_id: Option<&String>) -> (Session, bool) {
    //     let now = current_timestamp();

    //     if let Some(id) = session_id {
    //         if let Some(session) = self.sessions.get(id) {
    //             if !session.is_expired(now) {
    //                 return (session.clone(), false);
    //             }
    //         }
    //     }

    //     // let session = self.create(now);
    //     (session, true)
    // }

    // fn create(&mut self, now: u64) -> Session {
    //     self.counter += 1;

    //     // let id = format!("{}-{}", now, self.counter);

    //     let session = Session {
    //         expires_at: now + self.ttl,
    //         data: HashMap::new(),
    //     };

    //     self.sessions.insert(id.clone(), session.clone());
    //     session
    // }

    pub fn cleanup(&mut self) {
        let now = current_timestamp();
        self.sessions.retain(|_, s| !s.is_expired(now));
    }

    fn setup_new_session(&mut self, res: &mut HttpResponse) {
        let uuid = current_timestamp().to_string();
        self.sessions.insert(uuid.clone(), Session::new(self.ttl));

        let set_cookie = SetCookie::new("session_id", &uuid)
            .max_age(3600)
            .to_header();

        res.headers.insert("Set-Cookie".to_string(), set_cookie);
    }

    pub fn mange_session_store(&mut self, conn: &mut HttpConnection) {
        let cookies_header = conn.request.headers.get("cookie");
        dbg!(cookies_header);
        let cookies: Cookies = match cookies_header {
            Some(data) => Cookies::parse(data),
            None => Cookies::new(),
        };

        match cookies.get("session_id") {
            Some(session_id) => match self.sessions.get(session_id) {
                Some(session) if !session.is_expired(current_timestamp()) => {}
                _ => {
                    self.setup_new_session(&mut conn.response);
                }
            },
            _ => {
                self.setup_new_session(&mut conn.response);
            }
        };
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
