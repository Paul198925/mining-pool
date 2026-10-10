use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

pub fn sha256d(data: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(data)).into()
}

/// Bitcoin CompactSize.
pub fn write_varint(out: &mut Vec<u8>, n: u64) {
    match n {
        0..=0xfc => out.push(n as u8),
        0xfd..=0xffff => {
            out.push(0xfd);
            out.extend_from_slice(&(n as u16).to_le_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(0xfe);
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        _ => {
            out.push(0xff);
            out.extend_from_slice(&n.to_le_bytes());
        }
    }
}

/// Serializes `n` the way `CScript() << n` does; BIP34 requires exactly this for the coinbase height.
pub fn push_script_number(out: &mut Vec<u8>, n: u64) {
    match n {
        0 => out.push(0x00),
        1..=16 => out.push(0x50 + n as u8),
        _ => {
            let mut bytes: Vec<u8> = n.to_le_bytes().into_iter().collect();
            while bytes.last() == Some(&0) {
                bytes.pop();
            }
            // A set top bit would read as a negative number.
            if bytes.last().is_some_and(|b| b & 0x80 != 0) {
                bytes.push(0);
            }
            out.push(bytes.len() as u8);
            out.extend_from_slice(&bytes);
        }
    }
}

/// Block and transaction hashes are displayed byte-reversed relative to how they are hashed.
pub fn hash_from_display_hex(hex_str: &str) -> Result<[u8; 32]> {
    let mut bytes = hex32(hex_str)?;
    bytes.reverse();
    Ok(bytes)
}

pub fn hex32(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid hex: {hex_str}"))?;
    ensure!(bytes.len() == 32, "expected 32 bytes, got {}", bytes.len());
    Ok(bytes.try_into().unwrap())
}

/// Difficulty of a proof-of-work hash (little-endian number) relative to the Bitcoin diff-1 target.
pub fn hash_difficulty(hash_le: &[u8; 32]) -> f64 {
    let value = hash_le.iter().rev().fold(0f64, |acc, &b| acc * 256.0 + f64::from(b));
    let diff1 = 65535.0 * 2f64.powi(208);
    diff1 / value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script_number(n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        push_script_number(&mut out, n);
        out
    }

    #[test]
    fn script_numbers_match_cscript() {
        assert_eq!(script_number(0), [0x00]);
        assert_eq!(script_number(16), [0x60]);
        assert_eq!(script_number(17), [0x01, 0x11]);
        assert_eq!(script_number(127), [0x01, 0x7f]);
        assert_eq!(script_number(128), [0x02, 0x80, 0x00]);
        assert_eq!(script_number(500_000), [0x03, 0x20, 0xa1, 0x07]);
    }

    #[test]
    fn varints() {
        let enc = |n| {
            let mut out = Vec::new();
            write_varint(&mut out, n);
            out
        };
        assert_eq!(enc(252), [252]);
        assert_eq!(enc(253), [0xfd, 253, 0]);
        assert_eq!(enc(70_000), [0xfe, 0x70, 0x11, 0x01, 0x00]);
    }

    #[test]
    fn diff1_target_has_difficulty_one() {
        let mut target = [0u8; 32];
        target[26] = 0xff;
        target[27] = 0xff;
        assert!((hash_difficulty(&target) - 1.0).abs() < 1e-9);
    }
}
