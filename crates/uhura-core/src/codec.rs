use sha2::{Digest as _, Sha256};

/// Minimal unsigned LEB128 for one non-negative length.
pub fn nat(value: usize) -> Vec<u8> {
    nat_u64(value as u64)
}

/// Minimal unsigned LEB128 for one non-negative semantic integer.
pub fn nat_u64(value: u64) -> Vec<u8> {
    let mut value = value;
    let mut bytes = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            return bytes;
        }
    }
}

pub fn part(bytes: &[u8]) -> Vec<u8> {
    let mut framed = nat(bytes.len());
    framed.extend_from_slice(bytes);
    framed
}

pub fn frame(tag: &str, parts: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = part(tag.as_bytes());
    bytes.extend_from_slice(&nat(parts.len()));
    for item in parts {
        bytes.extend_from_slice(&part(item));
    }
    bytes
}

pub fn hash(domain: &str, parts: &[Vec<u8>]) -> [u8; 32] {
    let mut framed = Vec::with_capacity(parts.len() + 1);
    framed.push(domain.as_bytes().to_vec());
    framed.extend_from_slice(parts);
    Sha256::digest(frame("uhura-semantic-hash", &framed)).into()
}

pub fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub fn decode_hex_32(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("must be exactly 64 lowercase hexadecimal characters".into());
    }
    let mut output = [0; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_digit(chunk[0]) << 4) | hex_digit(chunk[1]);
    }
    Ok(output)
}

fn hex_digit(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => unreachable!("decode_hex_32 validated lowercase hex"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leb128_is_minimal() {
        assert_eq!(nat(0), [0]);
        assert_eq!(nat(127), [127]);
        assert_eq!(nat(128), [0x80, 0x01]);
        assert_eq!(
            nat_u64(u64::MAX),
            [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]
        );
    }

    #[test]
    fn frame_is_unambiguous() {
        assert_ne!(
            frame("x", &[b"ab".to_vec(), b"c".to_vec()]),
            frame("x", &[b"a".to_vec(), b"bc".to_vec()])
        );
    }

    #[test]
    fn fixed_hash_hex_is_strict() {
        assert_eq!(decode_hex_32(&"00".repeat(32)).unwrap(), [0; 32]);
        assert!(decode_hex_32(&"AA".repeat(32)).is_err());
        assert!(decode_hex_32("00").is_err());
    }
}
