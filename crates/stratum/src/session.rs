use std::{collections::HashMap, sync::Arc, time::Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use tracing::debug;

use crate::{
    accounts::WorkerIds,
    chain::Chain,
    job::{FoundBlock, Job, ShareError},
    pool::{Pool, Port},
    util::target_for_difficulty,
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
    /// The line was unreadable or a share that was not accepted.
    pub rejected: bool,
}

pub struct AcceptedShare {
    pub worker: WorkerIds,
    /// Who gets the credit on the merge-mined chain, if the coin has one.
    pub aux_worker: Option<WorkerIds>,
    /// The difficulty the share is credited with, in diff-1 units.
    pub difficulty: f64,
    pub block: Option<FoundBlock>,
    /// A block of the merge-mined chain; its `hex` is the proof to submit.
    pub aux_block: Option<FoundBlock>,
}

pub struct AuthRequest {
    id: Value,
    pub user: String,
    pub password: String,
}

/// A successful login: who the miner is and which coin's work they get.
pub struct Login {
    pub worker: WorkerIds,
    pub aux_worker: Option<WorkerIds>,
    pub pool: Arc<Pool>,
}

/// Per-connection Stratum V1 state. Pure request/response logic, no I/O.
///
/// A port can carry several coins, and which one a miner wants is only known at login.
/// Work therefore starts once the connection is both subscribed and authorized.
pub struct Session {
    port: Arc<Port>,
    extranonce1: Vec<u8>,
    subscribed: bool,
    login: Option<Login>,
    vardiff: VarDiff,
    /// Difficulty each job was sent with; shares are judged against that, not the current one.
    job_diffs: HashMap<String, f64>,
}

impl Session {
    pub fn new(port: Arc<Port>, now: Instant) -> Self {
        Self {
            extranonce1: port.next_extranonce1(),
            vardiff: VarDiff::new(port.vardiff, now),
            port,
            subscribed: false,
            login: None,
            job_diffs: HashMap::new(),
        }
    }

    /// `current` is the latest job of the coin this connection is logged in for, if any.
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
                out.rejected = true;
                return out;
            }
        };
        match req.method.as_str() {
            "mining.subscribe" => {
                let extranonce1 = hex::encode(&self.extranonce1);
                let result = match self.port.chain {
                    // KawPoW miners take a session id and the start of their nonce range.
                    Chain::Ravencoin => json!([null, extranonce1]),
                    Chain::Bitcoin | Chain::Xaya => json!([
                        [["mining.set_difficulty", extranonce1], ["mining.notify", extranonce1]],
                        extranonce1,
                        self.port.chain.extranonce2_size()
                    ]),
                };
                out.lines.push(ok(req.id, result));
                self.subscribed = true;
                out.lines.extend(self.start_work(current));
            }
            // Username is "<payout address>[.<worker name>]"; checking it needs the daemon and database.
            "mining.authorize" => match req.params.first().and_then(Value::as_str).map(str::trim) {
                Some(user) if !user.is_empty() => {
                    let password = req.params.get(1).and_then(Value::as_str).unwrap_or_default().to_owned();
                    out.auth = Some(AuthRequest { id: req.id, user: user.to_owned(), password });
                }
                _ => out.lines.push(error(req.id, ERR_UNAUTHORIZED, "Unauthorized worker")),
            },
            // Hashrate reports are for the miner's own display.
            "mining.extranonce.subscribe" | "eth_submitHashrate" | "mining.hashrate" => {
                out.lines.push(ok(req.id, json!(true)));
            }
            "mining.submit" if !self.subscribed => {
                out.lines.push(error(req.id, ERR_NOT_SUBSCRIBED, "Not subscribed"));
            }
            "mining.submit" => {
                let (reply, share) = self.submit(req.id, &req.params);
                out.lines.push(reply);
                out.rejected = share.is_none();
                out.share = share;
                out.lines.extend(self.retarget(current, now));
            }
            _ => out.lines.push(error(req.id, ERR_OTHER, "Unknown method")),
        }
        out
    }

    /// Completes a login. `current` is the latest job of the coin in `login`.
    pub fn finish_authorize(&mut self, request: AuthRequest, login: Option<Login>, current: Option<&Job>) -> Vec<String> {
        let Some(login) = login else {
            return vec![error(request.id, ERR_UNAUTHORIZED, "Unauthorized worker")];
        };
        self.login = Some(login);
        let mut lines = vec![ok(request.id, json!(true))];
        lines.extend(self.start_work(current));
        lines
    }

    /// Difficulty and first job, once the miner is ready for them.
    fn start_work(&mut self, current: Option<&Job>) -> Vec<String> {
        if !self.subscribed || self.login.is_none() {
            return Vec::new();
        }
        let mut lines = vec![self.difficulty_line(self.vardiff.diff())];
        lines.extend(current.and_then(|job| self.on_job(job)));
        lines
    }

    /// A new job was published. Returns the line to send, if this miner is ready for work.
    pub fn on_job(&mut self, job: &Job) -> Option<String> {
        if !self.subscribed || self.login.is_none() {
            return None;
        }
        if job.clean {
            self.job_diffs.clear();
        }
        self.job_diffs.insert(job.id.clone(), self.vardiff.diff());
        Some(job.notify(self.vardiff.diff()))
    }

    /// Periodic check so a miner that finds no shares at all still gets an easier difficulty.
    pub fn on_tick(&mut self, current: Option<&Job>, now: Instant) -> Vec<String> {
        if self.subscribed && self.login.is_some() { self.retarget(current, now) } else { Vec::new() }
    }

    fn retarget(&mut self, current: Option<&Job>, now: Instant) -> Vec<String> {
        let Some(diff) = self.vardiff.retarget(now) else {
            return Vec::new();
        };
        let mut lines = vec![self.difficulty_line(diff)];
        // Miners apply a new difficulty from the next notify on, so repeat the current job.
        // Shares for it may still arrive at the old difficulty; accept whichever is lower.
        if let Some(job) = current {
            let entry = self.job_diffs.entry(job.id.clone()).or_insert(diff);
            *entry = entry.min(diff);
            lines.push(job.notify(diff));
        }
        lines
    }

    fn difficulty_line(&self, diff: f64) -> String {
        let (method, param) = match self.port.chain {
            Chain::Ravencoin => ("mining.set_target", json!(hex::encode(target_for_difficulty(diff)))),
            Chain::Bitcoin | Chain::Xaya => ("mining.set_difficulty", json!(diff)),
        };
        json!({ "id": null, "method": method, "params": [param] }).to_string()
    }

    fn submit(&mut self, id: Value, params: &[Value]) -> (String, Option<AcceptedShare>) {
        let Some(Login { worker, aux_worker, pool }) = &self.login else {
            return (error(id, ERR_UNAUTHORIZED, "Unauthorized worker"), None);
        };
        // Some miners prefix hex with "0x".
        let field = |i: usize| params.get(i).and_then(Value::as_str).map(|hex| hex.trim_start_matches("0x"));
        let Some(job_id) = params.get(1).and_then(Value::as_str) else {
            return (error(id, ERR_OTHER, "Malformed submit"), None);
        };
        let (Some(&diff), Some(job)) = (self.job_diffs.get(job_id), pool.job(job_id)) else {
            return (error(id, ERR_JOB_NOT_FOUND, "Job not found"), None);
        };
        let hash32 = |hex_str: &str| -> Option<[u8; 32]> { hex::decode(hex_str).ok()?.try_into().ok() };
        // What still counts as meeting the difficulty. A target sent as hex is a rounded number.
        let mut floor = diff;
        let checked = match self.port.chain {
            // params: [worker, job id, nonce, header hash, mix hash]
            Chain::Ravencoin => (|| {
                let nonce = field(2).filter(|hex| hex.len() == 16)?;
                let nonce = u64::from_str_radix(nonce, 16).ok()?;
                floor = diff * (1.0 - 1e-6);
                Some(job.check_kawpow(&self.extranonce1, nonce, &hash32(field(3)?)?, &hash32(field(4)?)?, floor))
            })(),
            // params: [worker, job id, extranonce2, ntime, nonce]
            Chain::Bitcoin | Chain::Xaya => (|| {
                let extranonce2 = hex::decode(field(2)?).ok()?;
                let ntime = u32::from_str_radix(field(3)?, 16).ok()?;
                let nonce = u32::from_str_radix(field(4)?, 16).ok()?;
                Some(job.check_share(self.port.algo, &self.extranonce1, &extranonce2, ntime, nonce))
            })(),
        };
        let Some(checked) = checked else {
            return (error(id, ERR_OTHER, "Malformed submit"), None);
        };

        match checked {
            Err(ShareError::Malformed) => (error(id, ERR_OTHER, "Malformed submit"), None),
            Err(ShareError::BadNtime) => (error(id, ERR_OTHER, "Invalid ntime"), None),
            Err(ShareError::Duplicate) => (error(id, ERR_DUPLICATE, "Duplicate share"), None),
            Err(ShareError::NonceOutOfRange) => (error(id, ERR_OTHER, "Nonce out of range"), None),
            Err(ShareError::BadMixHash) => (error(id, ERR_OTHER, "Invalid mix hash"), None),
            // A block is a block even if vardiff has pushed the miner above network difficulty.
            Ok(share) if share.block.is_none() && share.aux_block.is_none() && share.difficulty < floor => {
                (error(id, ERR_LOW_DIFFICULTY, "Low difficulty share"), None)
            }
            Ok(share) => {
                let (worker, aux_worker) = (*worker, *aux_worker);
                self.vardiff.on_share();
                debug!(worker = worker.worker_id, diff, actual = share.difficulty, "share accepted");
                let difficulty = diff / self.port.algo.diff_multiplier();
                let accepted =
                    AcceptedShare { worker, aux_worker, difficulty, block: share.block, aux_block: share.aux_block };
                (ok(id, json!(true)), Some(accepted))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{algo::Algo, chain::Chain, job::tests::easy_template, vardiff::VarDiffConfig};

    const SUBSCRIBE: &str = r#"{"id":1,"method":"mining.subscribe","params":["cpuminer/2.5"]}"#;
    const AUTHORIZE: &str = r#"{"id":2,"method":"mining.authorize","params":["addr.rig1","c=DOGE"]}"#;
    const WORKER: WorkerIds = WorkerIds { account_id: 7, worker_id: 8, solo: false, party_id: None };

    fn session(start_diff: f64) -> Session {
        let vardiff = VarDiffConfig { start: start_diff, min: 1e-9, max: 1e12, share_target_secs: 15.0 };
        Session::new(Arc::new(Port::new(Algo::Scrypt, Chain::Bitcoin, vardiff, 5)), Instant::now())
    }

    fn publish(pool: &Pool, id: &str, clean: bool) -> Arc<Job> {
        let job = Arc::new(Job::new(id.into(), Chain::Bitcoin, 0, &easy_template(1), &[0x51], clean).unwrap());
        pool.add_job(job.clone());
        job
    }

    fn authorize(s: &mut Session, pool: &Arc<Pool>, current: Option<&Job>) -> Vec<String> {
        let request = s.handle_line(AUTHORIZE, None, Instant::now()).auth.expect("auth request");
        assert_eq!((request.user.as_str(), request.password.as_str()), ("addr.rig1", "c=DOGE"));
        s.finish_authorize(request, Some(Login { worker: WORKER, aux_worker: None, pool: pool.clone() }), current)
    }

    fn submit(job_id: &str, nonce: u32) -> String {
        json!({ "id": 9, "method": "mining.submit", "params": ["addr.rig1", job_id, "00000000", "6553f164", format!("{nonce:08x}")] })
            .to_string()
    }

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
    }

    fn methods(lines: &[String]) -> Vec<Value> {
        lines.iter().map(|line| parse(line)["method"].clone()).collect()
    }

    #[test]
    fn kawpow_miners_get_a_target_and_submit_nonce_and_mix_hash() {
        use crate::job::tests::solve_kawpow;
        // Low enough that a handful of hashes finds a share.
        let vardiff = VarDiffConfig { start: 1e-8, min: 1e-9, max: 1e12, share_target_secs: 15.0 };
        let mut s = Session::new(Arc::new(Port::new(Algo::Kawpow, Chain::Ravencoin, vardiff, 5)), Instant::now());
        let pool = Arc::new(Pool::default());
        let job = Arc::new(Job::new("a".into(), Chain::Ravencoin, 5, &easy_template(1), &[0x51], true).unwrap());
        pool.add_job(job.clone());

        let subscribed = parse(&s.handle_line(SUBSCRIBE, None, Instant::now()).lines[0]);
        assert_eq!(subscribed["result"], json!([null, "0001"]), "no session id, then the nonce prefix");
        let lines = authorize(&mut s, &pool, Some(&job));
        assert_eq!(methods(&lines[1..]), [json!("mining.set_target"), json!("mining.notify")]);
        let target = parse(&lines[1])["params"][0].clone();
        assert_eq!(target.as_str().unwrap().len(), 64);
        assert_eq!(parse(&lines[2])["params"][3], target, "the job repeats the miner's target");

        let (nonce, header_hash, mix) = solve_kawpow(&job, [0, 1], 1e-8);
        let submit = |nonce: u64, mix: &[u8; 32]| {
            let params = json!(["addr.rig1", "a", format!("0x{nonce:016x}"), format!("0x{}", hex::encode(header_hash)), format!("0x{}", hex::encode(mix))]);
            json!({ "id": 9, "method": "mining.submit", "params": params }).to_string()
        };
        let out = s.handle_line(&submit(nonce, &mix), Some(&job), Instant::now());
        assert_eq!(parse(&out.lines[0])["result"], true);
        assert_eq!(out.share.expect("accepted").difficulty, 1e-8);
        let forged = s.handle_line(&submit(nonce + 1, &mix), Some(&job), Instant::now());
        assert!(forged.share.is_none() && parse(&forged.lines[0])["error"][0] != Value::Null);
    }

    #[test]
    fn work_starts_at_login_because_only_then_the_coin_is_known() {
        let pool = Arc::new(Pool::default());
        let job = publish(&pool, "a", true);
        let mut s = session(0.5);

        let subscribed = s.handle_line(SUBSCRIBE, None, Instant::now()).lines;
        assert_eq!(subscribed.len(), 1);
        let reply = parse(&subscribed[0]);
        assert_eq!(reply["result"][1], "05000001", "instance id, then the connection counter");
        assert_eq!(reply["result"][2], 4);

        let lines = authorize(&mut s, &pool, Some(&job));
        assert_eq!(parse(&lines[0])["result"], true);
        assert_eq!(parse(&lines[1])["params"][0], 0.5);
        assert_eq!(methods(&lines[1..]), [json!("mining.set_difficulty"), json!("mining.notify")]);
    }

    #[test]
    fn login_before_subscribe_also_works() {
        let pool = Arc::new(Pool::default());
        let job = publish(&pool, "a", true);
        let mut s = session(0.5);
        assert_eq!(authorize(&mut s, &pool, Some(&job)).len(), 1);
        let lines = s.handle_line(SUBSCRIBE, Some(&job), Instant::now()).lines;
        assert_eq!(methods(&lines[1..]), [json!("mining.set_difficulty"), json!("mining.notify")]);
    }

    #[test]
    fn sessions_get_distinct_extranonces() {
        let vardiff = VarDiffConfig { start: 1.0, min: 1e-9, max: 1e12, share_target_secs: 15.0 };
        let port = Arc::new(Port::new(Algo::Neoscrypt, Chain::Xaya, vardiff, 5));
        let (a, b) = (Session::new(port.clone(), Instant::now()), Session::new(port, Instant::now()));
        assert_ne!(a.extranonce1, b.extranonce1);
        assert_eq!((a.extranonce1.as_slice(), b.extranonce1.as_slice()), (&[0, 1][..], &[0, 2][..]));
    }

    #[test]
    fn submit_requires_subscribe_then_authorize() {
        let mut s = session(1.0);
        let now = Instant::now();
        assert_eq!(parse(&s.handle_line(&submit("a", 1), None, now).lines[0])["error"][0], ERR_NOT_SUBSCRIBED);
        s.handle_line(SUBSCRIBE, None, now);
        assert_eq!(parse(&s.handle_line(&submit("a", 1), None, now).lines[0])["error"][0], ERR_UNAUTHORIZED);
        let denied = s.handle_line(r#"{"id":2,"method":"mining.authorize","params":[""]}"#, None, now);
        assert_eq!(parse(&denied.lines[0])["error"][0], ERR_UNAUTHORIZED);
        let request = s.handle_line(AUTHORIZE, None, now).auth.unwrap();
        assert_eq!(parse(&s.finish_authorize(request, None, None)[0])["error"][0], ERR_UNAUTHORIZED);
        assert_eq!(parse(&s.handle_line(&submit("a", 1), None, now).lines[0])["error"][0], ERR_UNAUTHORIZED);
    }

    #[test]
    fn share_lifecycle() {
        let pool = Arc::new(Pool::default());
        let job = publish(&pool, "a", true);
        let now = Instant::now();
        let mut s = session(1e-9);
        s.handle_line(SUBSCRIBE, None, now);
        authorize(&mut s, &pool, Some(&job));

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
    fn job_that_was_never_sent_to_this_session_is_not_accepted() {
        let pool = Arc::new(Pool::default());
        let now = Instant::now();
        let mut s = session(1e-9);
        s.handle_line(SUBSCRIBE, None, now);
        authorize(&mut s, &pool, None);
        let job = publish(&pool, "a", true);
        assert_eq!(parse(&s.handle_line(&submit("a", 1), Some(&job), now).lines[0])["error"][0], ERR_JOB_NOT_FOUND);
    }

    #[test]
    fn work_of_another_coin_is_not_accepted() {
        let (mine, other) = (Arc::new(Pool::default()), Arc::new(Pool::default()));
        let job = publish(&mine, "a", true);
        publish(&other, "b", true);
        let now = Instant::now();
        let mut s = session(1e-9);
        s.handle_line(SUBSCRIBE, None, now);
        authorize(&mut s, &mine, Some(&job));
        assert_eq!(parse(&s.handle_line(&submit("b", 1), Some(&job), now).lines[0])["error"][0], ERR_JOB_NOT_FOUND);
    }

    #[test]
    fn burst_of_shares_raises_difficulty_and_resends_the_job() {
        let pool = Arc::new(Pool::default());
        let job = publish(&pool, "a", true);
        let now = Instant::now();
        let mut s = session(1e-9);
        s.handle_line(SUBSCRIBE, None, now);
        authorize(&mut s, &pool, Some(&job));
        let lines: Vec<String> = (0..12).flat_map(|n| s.handle_line(&submit("a", n), Some(&job), now).lines).collect();
        let methods = methods(&lines);
        assert!(methods.contains(&json!("mining.set_difficulty")));
        assert_eq!(methods.last(), Some(&json!("mining.notify")));
    }

    #[test]
    fn garbage_gets_an_error_not_a_panic() {
        let mut s = session(1.0);
        let now = Instant::now();
        assert_eq!(parse(&s.handle_line("not json", None, now).lines[0])["error"][0], ERR_OTHER);
        assert!(s.handle_line("   ", None, now).lines.is_empty());
    }
}
