use std::{collections::HashMap, sync::Arc, time::Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use tracing::debug;

use crate::{
    accounts::WorkerIds,
    job::{EXTRANONCE1_SIZE, EXTRANONCE2_SIZE, FoundBlock, Job, ShareError},
    pool::Pool,
    vardiff::VarDiff,
};

// Stratum V1 error codes as used by Yiimp and most miners.
const ERR_OTHER: i64 = 20;
const ERR_JOB_NOT_FOUND: i64 = 21;
const ERR_DUPLICATE: i64 = 22;
const ERR_LOW_DIFFICULTY: i64 = 23;
const ERR_UNAUTHORIZED: i64 = 24;
const ERR_NOT_SUBSCRIBED: i64 = 25;

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Vec<Value>,
}

/// What the connection task has to do after feeding the session an event.
#[derive(Default)]
pub struct Output {
    pub lines: Vec<String>,
    pub share: Option<AcceptedShare>,
    /// The miner wants to log in; answer with [`Session::finish_authorize`].
    pub auth: Option<AuthRequest>,
}

pub struct AcceptedShare {
    pub worker: WorkerIds,
    /// The difficulty the share is credited with, in diff-1 units.
    pub difficulty: f64,
    pub block: Option<FoundBlock>,
}

pub struct AuthRequest {
    id: Value,
    pub user: String,
    pub password: String,
}

/// Per-connection Stratum V1 state. Pure request/response logic, no I/O.
pub struct Session {
    pool: Arc<Pool>,
    extranonce1: [u8; EXTRANONCE1_SIZE],
    subscribed: bool,
    worker: Option<WorkerIds>,
    vardiff: VarDiff,
    /// Difficulty each job was sent with; shares are judged against that, not the current one.
    job_diffs: HashMap<String, f64>,
}

impl Session {
    pub fn new(pool: Arc<Pool>, now: Instant) -> Self {
        Self {
            extranonce1: pool.next_extranonce1(),
            vardiff: VarDiff::new(pool.vardiff, now),
            pool,
            subscribed: false,
            worker: None,
            job_diffs: HashMap::new(),
        }
    }

    pub fn handle_line(&mut self, line: &str, current: Option<&Job>, now: Instant) -> Output {
        let mut out = Output::default();
        let line = line.trim();
        if line.is_empty() {
            return out;
        }
        let req: Request = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(_) => {
                out.lines.push(error(Value::Null, ERR_OTHER, "Malformed request"));
                return out;
            }
        };
        match req.method.as_str() {
            "mining.subscribe" => {
                self.subscribed = true;
                let extranonce1 = hex::encode(self.extranonce1);
                out.lines.push(ok(
                    req.id,
                    json!([
                        [["mining.set_difficulty", extranonce1], ["mining.notify", extranonce1]],
                        extranonce1,
                        EXTRANONCE2_SIZE
                    ]),
                ));
                out.lines.push(set_difficulty(self.vardiff.diff()));
                out.lines.extend(current.and_then(|job| self.on_job(job)));
            }
            // Username is "<payout address>[.<worker name>]"; checking it needs the daemon and database.
            "mining.authorize" => match req.params.first().and_then(Value::as_str).map(str::trim) {
                Some(user) if !user.is_empty() => {
                    let password = req.params.get(1).and_then(Value::as_str).unwrap_or_default().to_owned();
                    out.auth = Some(AuthRequest { id: req.id, user: user.to_owned(), password });
                }
                _ => out.lines.push(error(req.id, ERR_UNAUTHORIZED, "Unauthorized worker")),
            },
            "mining.extranonce.subscribe" => out.lines.push(ok(req.id, json!(true))),
            "mining.submit" if !self.subscribed => {
                out.lines.push(error(req.id, ERR_NOT_SUBSCRIBED, "Not subscribed"));
            }
            "mining.submit" if self.worker.is_none() => {
                out.lines.push(error(req.id, ERR_UNAUTHORIZED, "Unauthorized worker"));
            }
            "mining.submit" => {
                let (reply, share) = self.submit(req.id, &req.params);
                out.lines.push(reply);
                out.share = share;
                out.lines.extend(self.retarget(current, now));
            }
            _ => out.lines.push(error(req.id, ERR_OTHER, "Unknown method")),
        }
        out
    }

    /// A new job was published. Returns the line to send, if this miner is ready for work.
    pub fn on_job(&mut self, job: &Job) -> Option<String> {
        if !self.subscribed {
            return None;
        }
        if job.clean {
            self.job_diffs.clear();
        }
        self.job_diffs.insert(job.id.clone(), self.vardiff.diff());
        Some(job.notify.clone())
    }

    /// Periodic check so a miner that finds no shares at all still gets an easier difficulty.
    pub fn on_tick(&mut self, current: Option<&Job>, now: Instant) -> Vec<String> {
        if self.subscribed { self.retarget(current, now) } else { Vec::new() }
    }

    fn retarget(&mut self, current: Option<&Job>, now: Instant) -> Vec<String> {
        let Some(diff) = self.vardiff.retarget(now) else {
            return Vec::new();
        };
        let mut lines = vec![set_difficulty(diff)];
        // Miners apply a new difficulty from the next notify on, so repeat the current job.
        // Shares for it may still arrive at the old difficulty; accept whichever is lower.
        if let Some(job) = current {
            let entry = self.job_diffs.entry(job.id.clone()).or_insert(diff);
            *entry = entry.min(diff);
            lines.push(job.notify.clone());
        }
        lines
    }

    pub fn finish_authorize(&mut self, request: AuthRequest, worker: Option<WorkerIds>) -> String {
        match worker {
            Some(worker) => {
                self.worker = Some(worker);
                ok(request.id, json!(true))
            }
            None => error(request.id, ERR_UNAUTHORIZED, "Unauthorized worker"),
        }
    }

    fn submit(&mut self, id: Value, params: &[Value]) -> (String, Option<AcceptedShare>) {
        let Some(worker) = self.worker else {
            return (error(id, ERR_UNAUTHORIZED, "Unauthorized worker"), None);
        };
        // params: [worker, job id, extranonce2, ntime, nonce]
        let field = |i: usize| params.get(i).and_then(Value::as_str);
        let parsed = (|| {
            let extranonce2 = hex::decode(field(2)?).ok()?;
            let ntime = u32::from_str_radix(field(3)?, 16).ok()?;
            let nonce = u32::from_str_radix(field(4)?, 16).ok()?;
            Some((field(1)?, extranonce2, ntime, nonce))
        })();
        let Some((job_id, extranonce2, ntime, nonce)) = parsed else {
            return (error(id, ERR_OTHER, "Malformed submit"), None);
        };
        let (Some(&diff), Some(job)) = (self.job_diffs.get(job_id), self.pool.job(job_id)) else {
            return (error(id, ERR_JOB_NOT_FOUND, "Job not found"), None);
        };

        match job.check_share(self.pool.algo, &self.extranonce1, &extranonce2, ntime, nonce) {
            Err(ShareError::Malformed) => (error(id, ERR_OTHER, "Malformed submit"), None),
            Err(ShareError::BadNtime) => (error(id, ERR_OTHER, "Invalid ntime"), None),
            Err(ShareError::Duplicate) => (error(id, ERR_DUPLICATE, "Duplicate share"), None),
            // A block is a block even if vardiff has pushed the miner above network difficulty.
            Ok(share) if share.block.is_none() && share.difficulty < diff => {
                (error(id, ERR_LOW_DIFFICULTY, "Low difficulty share"), None)
            }
            Ok(share) => {
                self.vardiff.on_share();
                debug!(worker = worker.worker_id, diff, actual = share.difficulty, "share accepted");
                let difficulty = diff / self.pool.algo.diff_multiplier();
                (ok(id, json!(true)), Some(AcceptedShare { worker, difficulty, block: share.block }))
            }
        }
    }
}

fn ok(id: Value, result: Value) -> String {
    json!({ "id": id, "result": result, "error": null }).to_string()
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({ "id": id, "result": null, "error": [code, message, null] }).to_string()
}

fn set_difficulty(diff: f64) -> String {
    json!({ "id": null, "method": "mining.set_difficulty", "params": [diff] }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{algo::Algo, job::tests::easy_template, vardiff::VarDiffConfig};

    const SUBSCRIBE: &str = r#"{"id":1,"method":"mining.subscribe","params":["cpuminer/2.5"]}"#;
    const AUTHORIZE: &str = r#"{"id":2,"method":"mining.authorize","params":["addr.rig1","x"]}"#;
    const WORKER: WorkerIds = WorkerIds { account_id: 7, worker_id: 8, solo: false, party_id: None };

    fn authorize(s: &mut Session) {
        let request = s.handle_line(AUTHORIZE, None, Instant::now()).auth.expect("auth request");
        assert_eq!((request.user.as_str(), request.password.as_str()), ("addr.rig1", "x"));
        assert_eq!(parse(&s.finish_authorize(request, Some(WORKER)))["result"], true);
    }

    fn pool(start_diff: f64) -> Arc<Pool> {
        let vardiff = VarDiffConfig { start: start_diff, min: 1e-9, max: 1e12, share_target_secs: 15.0 };
        Arc::new(Pool::new(Algo::Scrypt, vardiff))
    }

    fn publish(pool: &Pool, id: &str, clean: bool) -> Arc<Job> {
        let job = Arc::new(Job::new(id.into(), &easy_template(1), &[0x51], clean).unwrap());
        pool.add_job(job.clone());
        job
    }

    fn submit(job_id: &str, nonce: u32) -> String {
        json!({ "id": 9, "method": "mining.submit", "params": ["addr.rig1", job_id, "00000000", "6553f164", format!("{nonce:08x}")] })
            .to_string()
    }

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
    }

    #[test]
    fn subscribe_sends_extranonce_difficulty_and_job() {
        let pool = pool(0.5);
        let job = publish(&pool, "a", true);
        let mut s = Session::new(pool, Instant::now());
        let out = s.handle_line(SUBSCRIBE, Some(&job), Instant::now());
        assert_eq!(out.lines.len(), 3);
        let reply = parse(&out.lines[0]);
        assert_eq!(reply["result"][1], "00000001");
        assert_eq!(reply["result"][2], EXTRANONCE2_SIZE);
        assert_eq!(parse(&out.lines[1])["params"][0], 0.5);
        assert_eq!(parse(&out.lines[2])["method"], "mining.notify");
    }

    #[test]
    fn sessions_get_distinct_extranonces() {
        let pool = pool(1.0);
        let (a, b) = (Session::new(pool.clone(), Instant::now()), Session::new(pool, Instant::now()));
        assert_ne!(a.extranonce1, b.extranonce1);
    }

    #[test]
    fn submit_requires_subscribe_then_authorize() {
        let mut s = Session::new(pool(1.0), Instant::now());
        let now = Instant::now();
        assert_eq!(parse(&s.handle_line(&submit("a", 1), None, now).lines[0])["error"][0], ERR_NOT_SUBSCRIBED);
        s.handle_line(SUBSCRIBE, None, now);
        assert_eq!(parse(&s.handle_line(&submit("a", 1), None, now).lines[0])["error"][0], ERR_UNAUTHORIZED);
        let denied = s.handle_line(r#"{"id":2,"method":"mining.authorize","params":[""]}"#, None, now);
        assert_eq!(parse(&denied.lines[0])["error"][0], ERR_UNAUTHORIZED);
        let request = s.handle_line(AUTHORIZE, None, now).auth.unwrap();
        assert_eq!(parse(&s.finish_authorize(request, None))["error"][0], ERR_UNAUTHORIZED);
        assert_eq!(parse(&s.handle_line(&submit("a", 1), None, now).lines[0])["error"][0], ERR_UNAUTHORIZED);
    }

    #[test]
    fn share_lifecycle() {
        let pool = pool(1e-9);
        let job = publish(&pool, "a", true);
        let now = Instant::now();
        let mut s = Session::new(pool.clone(), now);
        s.handle_line(SUBSCRIBE, Some(&job), now);
        authorize(&mut s);

        let accepted = s.handle_line(&submit("a", 1), Some(&job), now);
        assert_eq!(parse(&accepted.lines[0])["result"], true);
        let share = accepted.share.expect("accepted share");
        assert_eq!(share.worker, WORKER);
        assert!((share.difficulty - 1e-9 / 65536.0).abs() < 1e-20, "credited in diff-1 units");
        assert!(share.block.is_some(), "easy template target accepts every hash");

        let duplicate = s.handle_line(&submit("a", 1), Some(&job), now);
        assert_eq!(parse(&duplicate.lines[0])["error"][0], ERR_DUPLICATE);
        assert_eq!(parse(&s.handle_line(&submit("zz", 1), Some(&job), now).lines[0])["error"][0], ERR_JOB_NOT_FOUND);

        // A new chain tip invalidates the old job.
        let next = publish(&pool, "b", true);
        s.on_job(&next);
        assert_eq!(parse(&s.handle_line(&submit("a", 2), Some(&next), now).lines[0])["error"][0], ERR_JOB_NOT_FOUND);
        assert_eq!(parse(&s.handle_line(&submit("b", 2), Some(&next), now).lines[0])["result"], true);
    }

    #[test]
    fn job_from_another_session_is_not_accepted() {
        let pool = pool(1e-9);
        let now = Instant::now();
        let mut s = Session::new(pool.clone(), now);
        s.handle_line(SUBSCRIBE, None, now);
        authorize(&mut s);
        let job = publish(&pool, "a", true);
        assert_eq!(parse(&s.handle_line(&submit("a", 1), Some(&job), now).lines[0])["error"][0], ERR_JOB_NOT_FOUND);
    }

    #[test]
    fn burst_of_shares_raises_difficulty_and_resends_the_job() {
        let pool = pool(1e-9);
        let job = publish(&pool, "a", true);
        let now = Instant::now();
        let mut s = Session::new(pool, now);
        s.handle_line(SUBSCRIBE, Some(&job), now);
        authorize(&mut s);
        let lines: Vec<String> = (0..12).flat_map(|n| s.handle_line(&submit("a", n), Some(&job), now).lines).collect();
        let methods: Vec<Value> = lines.iter().map(|l| parse(l)["method"].clone()).collect();
        assert!(methods.contains(&json!("mining.set_difficulty")));
        assert_eq!(methods.last(), Some(&json!("mining.notify")));
    }

    #[test]
    fn garbage_gets_an_error_not_a_panic() {
        let mut s = Session::new(pool(1.0), Instant::now());
        let now = Instant::now();
        assert_eq!(parse(&s.handle_line("not json", None, now).lines[0])["error"][0], ERR_OTHER);
        assert!(s.handle_line("   ", None, now).lines.is_empty());
    }
}
