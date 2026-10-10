//! A slow CPU miner for KawPoW, only for testing the pool against a regtest node:
//!
//!   cargo run --release -p pool-stratum --example kawpow_miner -- 127.0.0.1:3336 <address>.rig1 [password] [shares]
//!
//! It speaks the stratum dialect of kawpowminer and stops after the given number of
//! accepted shares (default 5). It hashes the way the pool verifies, a few hundred
//! hashes per second; real mining needs a GPU miner.

#[allow(dead_code)]
#[path = "../src/kawpow.rs"]
mod kawpow;

use std::{
    env,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    sync::mpsc,
    thread,
    time::Instant,
};

use kawpow::Epoch;
use serde_json::{Value, json};

struct Job {
    id: String,
    header_hash: [u8; 32],
    target: [u8; 32],
    height: u64,
}

fn hash32(value: &Value) -> Option<[u8; 32]> {
    let hex_str = value.as_str()?.trim_start_matches("0x");
    hex::decode(format!("{hex_str:0>64}")).ok()?.try_into().ok()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let (Some(pool), Some(user)) = (args.first(), args.get(1)) else {
        eprintln!("usage: kawpow_miner <host:port> <address>[.worker] [password] [shares]");
        std::process::exit(2);
    };
    let password = args.get(2).map_or("x", String::as_str);
    let wanted: u32 = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(5);

    let mut stream = TcpStream::connect(pool).expect("connect to the pool");
    let reader = BufReader::new(stream.try_clone().expect("clone socket"));
    let (tx, rx) = mpsc::channel::<Value>();
    thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(message) = serde_json::from_str(&line) {
                let _ = tx.send(message);
            }
        }
    });
    let mut send = |message: Value| writeln!(stream, "{message}").expect("send to the pool");
    send(json!({ "id": 1, "method": "mining.subscribe", "params": ["kawpow_miner/test", "EthereumStratum/1.0.0"] }));
    send(json!({ "id": 2, "method": "mining.authorize", "params": [user, password] }));

    let mut prefix: Option<u64> = None;
    let mut job: Option<Job> = None;
    let mut epoch: Option<Epoch> = None;
    let mut nonce = 0u64;
    let (mut accepted, mut hashes, started) = (0u32, 0u64, Instant::now());

    while accepted < wanted {
        // Take in everything the pool has sent; wait only while there is nothing to hash.
        loop {
            let message = if job.is_some() && prefix.is_some() { rx.try_recv().ok() } else { rx.recv().ok() };
            let Some(message) = message else { break };
            match (message["id"].as_u64(), message["method"].as_str()) {
                (Some(1), _) => {
                    let extranonce = message["result"][1].as_str().expect("extranonce in the subscribe reply");
                    let bits = 4 * extranonce.len() as u32;
                    prefix = Some(u64::from_str_radix(extranonce, 16).expect("hex extranonce") << (64 - bits));
                    println!("subscribed, nonce prefix {extranonce}");
                }
                (Some(2), _) => assert_eq!(message["result"], true, "login refused: {message}"),
                (Some(_), _) => {
                    if message["result"] == true {
                        accepted += 1;
                        println!("share {accepted}/{wanted} accepted");
                    } else {
                        println!("share rejected: {}", message["error"]);
                    }
                }
                (None, Some("mining.set_target")) => {
                    if let (Some(job), Some(target)) = (&mut job, hash32(&message["params"][0])) {
                        job.target = target;
                    }
                }
                (None, Some("mining.notify")) => {
                    let p = &message["params"];
                    let new = Job {
                        id: p[0].as_str().expect("job id").to_owned(),
                        header_hash: hash32(&p[1]).expect("header hash"),
                        target: hash32(&p[3]).expect("target"),
                        height: p[5].as_u64().expect("height"),
                    };
                    let number = Epoch::of_height(new.height);
                    assert_eq!(hash32(&p[2]), Some(Epoch::seed(number)), "seed hash does not match the height");
                    if epoch.as_ref().is_none_or(|epoch| epoch.number != number) {
                        println!("building epoch {number}...");
                        epoch = Some(Epoch::build(number).expect("epoch"));
                    }
                    println!("job {} at height {}", new.id, new.height);
                    job = Some(new);
                }
                _ => {}
            }
        }
        let (Some(job), Some(prefix), Some(epoch)) = (&job, prefix, &epoch) else { continue };
        for _ in 0..50 {
            nonce += 1;
            hashes += 1;
            let pow = epoch.hash(job.height, &job.header_hash, prefix | nonce);
            if pow.final_hash <= job.target {
                send(json!({
                    "id": 4,
                    "method": "mining.submit",
                    "params": [
                        user,
                        job.id,
                        format!("0x{:016x}", prefix | nonce),
                        format!("0x{}", hex::encode(job.header_hash)),
                        format!("0x{}", hex::encode(pow.mix_hash)),
                    ],
                }));
            }
        }
    }
    println!("done: {hashes} hashes, {:.0} H/s", hashes as f64 / started.elapsed().as_secs_f64());
}
