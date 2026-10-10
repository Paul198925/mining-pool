use std::sync::atomic::{AtomicU32, Ordering};

use serde::Deserialize;
use serde_json::{Value, json};

const EXTRANONCE2_SIZE: usize = 4;

// Stratum V1 error codes as used by Yiimp and most miners.
const ERR_OTHER: i64 = 20;
const ERR_UNAUTHORIZED: i64 = 24;
const ERR_NOT_SUBSCRIBED: i64 = 25;

static NEXT_EXTRANONCE1: AtomicU32 = AtomicU32::new(1);

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Vec<Value>,
}

/// Per-connection Stratum V1 state. Pure request/response logic, no I/O.
pub struct Session {
    extranonce1: u32,
    subscribed: bool,
    worker: Option<String>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            extranonce1: NEXT_EXTRANONCE1.fetch_add(1, Ordering::Relaxed),
            subscribed: false,
            worker: None,
        }
    }

    /// Returns the JSON line to send back, if the message warrants one.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let req: Request = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(_) => return Some(error(Value::Null, ERR_OTHER, "Malformed request")),
        };
        let reply = match req.method.as_str() {
            "mining.subscribe" => self.subscribe(req.id),
            "mining.authorize" => self.authorize(req.id, &req.params),
            "mining.extranonce.subscribe" => ok(req.id, json!(true)),
            "mining.submit" if !self.subscribed => error(req.id, ERR_NOT_SUBSCRIBED, "Not subscribed"),
            "mining.submit" if self.worker.is_none() => error(req.id, ERR_UNAUTHORIZED, "Unauthorized worker"),
            // Share validation arrives with the job pipeline.
            "mining.submit" => error(req.id, ERR_OTHER, "No active job"),
            _ => error(req.id, ERR_OTHER, "Unknown method"),
        };
        Some(reply)
    }

    fn subscribe(&mut self, id: Value) -> String {
        self.subscribed = true;
        let extranonce1 = format!("{:08x}", self.extranonce1);
        ok(
            id,
            json!([
                [["mining.set_difficulty", extranonce1], ["mining.notify", extranonce1]],
                extranonce1,
                EXTRANONCE2_SIZE
            ]),
        )
    }

    fn authorize(&mut self, id: Value, params: &[Value]) -> String {
        // Username is "<payout address>[.<worker name>]"; the address is checked against the coin later.
        match params.first().and_then(Value::as_str).map(str::trim) {
            Some(user) if !user.is_empty() => {
                self.worker = Some(user.to_owned());
                ok(id, json!(true))
            }
            _ => error(id, ERR_UNAUTHORIZED, "Unauthorized worker"),
        }
    }
}

fn ok(id: Value, result: Value) -> String {
    json!({ "id": id, "result": result, "error": null }).to_string()
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({ "id": id, "result": null, "error": [code, message, null] }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(session: &mut Session, line: &str) -> Value {
        serde_json::from_str(&session.handle_line(line).unwrap()).unwrap()
    }

    #[test]
    fn subscribe_returns_extranonce() {
        let mut s = Session::new();
        let r = roundtrip(&mut s, r#"{"id":1,"method":"mining.subscribe","params":["cpuminer/2.5"]}"#);
        assert_eq!(r["id"], 1);
        assert!(r["error"].is_null());
        assert_eq!(r["result"][1].as_str().unwrap().len(), 8);
        assert_eq!(r["result"][2], EXTRANONCE2_SIZE);
    }

    #[test]
    fn sessions_get_distinct_extranonces() {
        let (mut a, mut b) = (Session::new(), Session::new());
        let line = r#"{"id":1,"method":"mining.subscribe","params":[]}"#;
        assert_ne!(roundtrip(&mut a, line)["result"][1], roundtrip(&mut b, line)["result"][1]);
    }

    #[test]
    fn authorize_needs_username() {
        let mut s = Session::new();
        let r = roundtrip(&mut s, r#"{"id":2,"method":"mining.authorize","params":["", "x"]}"#);
        assert_eq!(r["error"][0], ERR_UNAUTHORIZED);
        let r = roundtrip(&mut s, r#"{"id":3,"method":"mining.authorize","params":["addr.rig1", "x"]}"#);
        assert_eq!(r["result"], true);
    }

    #[test]
    fn submit_requires_subscribe_then_authorize() {
        let mut s = Session::new();
        let submit = r#"{"id":4,"method":"mining.submit","params":[]}"#;
        assert_eq!(roundtrip(&mut s, submit)["error"][0], ERR_NOT_SUBSCRIBED);
        roundtrip(&mut s, r#"{"id":1,"method":"mining.subscribe","params":[]}"#);
        assert_eq!(roundtrip(&mut s, submit)["error"][0], ERR_UNAUTHORIZED);
    }

    #[test]
    fn garbage_gets_an_error_not_a_panic() {
        let mut s = Session::new();
        assert_eq!(roundtrip(&mut s, "not json")["error"][0], ERR_OTHER);
        assert!(s.handle_line("   ").is_none());
    }
}
