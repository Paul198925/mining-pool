//! Keeps a single address from exhausting the server: a cap on connections per address
//! and in total, and a short ban for connections that only send rubbish.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// A mining farm behind one address can have many rigs; a flood has far more.
const MAX_PER_ADDRESS: u32 = 200;
const MAX_TOTAL: usize = 50_000;
const BAN: Duration = Duration::from_secs(600);

pub static GUARD: LazyLock<Guard> = LazyLock::new(Guard::default);

#[derive(Default)]
pub struct Guard {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    connections: HashMap<IpAddr, u32>,
    total: usize,
    banned: HashMap<IpAddr, Instant>,
}

/// Why a connection was turned away.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    Banned,
    TooManyFromAddress,
    ServerFull,
}

/// One admitted connection; gives its place back when dropped.
pub struct Permit {
    guard: &'static Guard,
    address: IpAddr,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.guard.state.lock().unwrap();
        state.total = state.total.saturating_sub(1);
        if let Some(count) = state.connections.get_mut(&self.address) {
            *count -= 1;
            if *count == 0 {
                state.connections.remove(&self.address);
            }
        }
    }
}

impl Guard {
    pub fn admit(&'static self, address: IpAddr, now: Instant) -> Result<Permit, Refused> {
        let mut state = self.state.lock().unwrap();
        state.banned.retain(|_, until| *until > now);
        if state.banned.contains_key(&address) {
            return Err(Refused::Banned);
        }
        if state.total >= MAX_TOTAL {
            return Err(Refused::ServerFull);
        }
        let count = state.connections.entry(address).or_default();
        if *count >= MAX_PER_ADDRESS {
            return Err(Refused::TooManyFromAddress);
        }
        *count += 1;
        state.total += 1;
        Ok(Permit { guard: self, address })
    }

    /// Turns the address away for a while. Its open connections are not touched.
    pub fn ban(&self, address: IpAddr, now: Instant) {
        self.state.lock().unwrap().banned.insert(address, now + BAN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaked() -> &'static Guard {
        Box::leak(Box::default())
    }

    #[test]
    fn caps_connections_per_address_and_frees_them_again() {
        let guard = leaked();
        let (a, b): (IpAddr, IpAddr) = ("192.0.2.1".parse().unwrap(), "192.0.2.2".parse().unwrap());
        let now = Instant::now();
        let mut permits: Vec<Permit> = (0..MAX_PER_ADDRESS).map(|_| guard.admit(a, now).unwrap()).collect();
        assert_eq!(guard.admit(a, now).err(), Some(Refused::TooManyFromAddress));
        assert!(guard.admit(b, now).is_ok(), "another address is not affected");
        permits.pop();
        assert!(guard.admit(a, now).is_ok(), "a closed connection frees its place");
    }

    #[test]
    fn a_ban_ends_by_itself() {
        let guard = leaked();
        let address: IpAddr = "192.0.2.9".parse().unwrap();
        let now = Instant::now();
        guard.ban(address, now);
        assert_eq!(guard.admit(address, now).err(), Some(Refused::Banned));
        assert_eq!(guard.admit(address, now + BAN - Duration::from_secs(1)).err(), Some(Refused::Banned));
        assert!(guard.admit(address, now + BAN + Duration::from_secs(1)).is_ok());
    }
}
