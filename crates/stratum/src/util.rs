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

/// Expands a compact target ("bits") to the full big-endian number.
pub fn target_from_bits(bits: u32) -> [u8; 32] {
    let mut target = [0u8; 32];
    let size = (bits >> 24) as usize;
    let mantissa = bits & 0x007f_ffff;
    for i in 0..3 {
        // The mantissa's most significant byte sits `size` bytes from the end.
        let position = 32usize.wrapping_sub(size).wrapping_add(i);
        if position < 32 {
            target[position] = (mantissa >> (8 * (2 - i))) as u8;
        }
    }
    target
}

/// The largest hash, as a big-endian number, that still has `difficulty`. For miners
/// that are told a target instead of a difficulty.
pub fn target_for_difficulty(difficulty: f64) -> [u8; 32] {
    // target = 65535 * 2^208 / difficulty, written as mantissa * 2^shift with a 53-bit mantissa.
    let quotient = 65535.0 / difficulty;
    let exponent = quotient.log2().floor() as i32;
    let mantissa = (quotient / 2f64.powi(exponent - 52)) as u64;
    let shift = 208 + exponent - 52;
    if shift + 53 > 256 || !quotient.is_finite() {
        return [0xff; 32];
    }
    let mut target = [0u8; 32];
    for bit in 0..64 {
        let position = shift + bit;
        if mantissa >> bit & 1 == 1 && position >= 0 {
            target[31 - (position / 8) as usize] |= 1 << (position % 8);
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_targets_expand() {
        assert_eq!(hex::encode(target_from_bits(0x1d00ffff)), format!("00000000ffff{}", "0".repeat(52)));
        assert_eq!(hex::encode(target_from_bits(0x207fffff)), format!("7fffff{}", "0".repeat(58)));
        assert_eq!(hex::encode(target_from_bits(0x03123456)), format!("{}123456", "0".repeat(58)));
    }

    #[test]
    fn targets_round_trip_through_difficulty() {
        for difficulty in [1.0, 0.001, 0.5, 3.7, 8192.0, 1e9] {
            let mut target = target_for_difficulty(difficulty);
            target.reverse();
            let back = hash_difficulty(&target);
            assert!((back / difficulty - 1.0).abs() < 1e-9, "{difficulty} came back as {back}");
        }
        assert_eq!(hex::encode(target_for_difficulty(1.0)), format!("00000000ffff{}", "0".repeat(52)));
        assert_eq!(target_for_difficulty(1e-80), [0xff; 32]);
    }

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
