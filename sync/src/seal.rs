//! The locked box and the pairing code.
//!
//! A snapshot is encrypted on the device before it goes anywhere (XChaCha20-Poly1305, a fresh
//! random nonce each time). The server stores and hands out the box without being able to
//! open it; a box that was altered, or made by someone without the key, doesn't open at all.
//!
//! The pairing code carries what a device needs to join: which server to trust (a fingerprint
//! of its certificate), the password the server asks for, and the seed of the key to the box.
//! A standalone server's own code has only the first two: it never learns the key.

use crate::Error;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 4] = b"HZS1";
const NONCE: usize = 24;
const TAG: usize = 16;

pub fn seal(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>, Error> {
    let nonce = crate::random::<NONCE>();
    let sealed = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad: MAGIC })
        .map_err(|_| Error::Corrupt("couldn't be encrypted".into()))?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

pub fn open(key: &[u8; 32], boxed: &[u8]) -> Result<Vec<u8>, Error> {
    if !looks_sealed(boxed) {
        return Err(Error::Locked);
    }
    let (nonce, sealed) = boxed[MAGIC.len()..].split_at(NONCE);
    XChaCha20Poly1305::new(key.into()).decrypt(XNonce::from_slice(nonce), Payload { msg: sealed, aad: MAGIC }).map_err(|_| Error::Locked)
}

/// All a server can tell about a box: that it has the shape of one.
pub fn looks_sealed(boxed: &[u8]) -> bool {
    boxed.len() >= MAGIC.len() + NONCE + TAG && &boxed[..MAGIC.len()] == MAGIC
}

/// The first 16 bytes of a certificate's SHA-256: what a client checks the server against.
pub fn fingerprint(cert_der: &[u8]) -> [u8; 16] {
    Sha256::digest(cert_der)[..16].try_into().expect("16 bytes")
}

/// What the server keeps of the password: enough to check it, not enough to know it.
pub fn auth_hash(auth: &[u8; 16]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"hanzi-sync auth v1");
    h.update(auth);
    h.finalize().into()
}

/// The key to the box, from the seed in the pairing code.
pub fn data_key(seed: &[u8; 16]) -> [u8; 32] {
    let mut key = [0u8; 32];
    hkdf::Hkdf::<Sha256>::new(Some(b"hanzi-sync"), seed).expand(b"box key v1", &mut key).expect("32 bytes is a valid length");
    key
}

#[derive(Clone, PartialEq, Eq)]
pub struct Code {
    pub fingerprint: [u8; 16],
    pub auth: [u8; 16],
    /// Missing in a standalone server's own code: the first device to join makes one.
    pub data: Option<[u8; 16]>,
}

// (never printed whole by accident)
impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Code(server {}…)", crate::hex(&self.fingerprint[..4]))
    }
}

const CODE_VERSION: u8 = 1;
/// Crockford's base 32: no I, L, O or U, so a code can be read out and typed.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

impl Code {
    pub fn new(fingerprint: [u8; 16]) -> Code {
        Code { fingerprint, auth: crate::random(), data: Some(crate::random()) }
    }

    /// As text: groups of five letters and digits, with two check bytes at the end so that a
    /// mistyped code is refused instead of tried.
    pub fn text(&self) -> String {
        let mut bytes = vec![CODE_VERSION];
        bytes.extend_from_slice(&self.fingerprint);
        bytes.extend_from_slice(&self.auth);
        if let Some(d) = &self.data {
            bytes.extend_from_slice(d);
        }
        let check = Sha256::digest(&bytes);
        bytes.extend_from_slice(&check[..2]);
        let mut out = String::new();
        let (mut acc, mut bits) = (0u32, 0u32);
        let mut push = |c: u8| {
            if out.len() % 6 == 5 {
                out.push('-');
            }
            out.push(c as char);
        };
        for b in bytes {
            acc = (acc << 8) | b as u32;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                push(ALPHABET[((acc >> bits) & 31) as usize]);
            }
        }
        if bits > 0 {
            push(ALPHABET[((acc << (5 - bits)) & 31) as usize]);
        }
        out
    }

    pub fn parse(text: &str) -> Result<Code, Error> {
        let mut bytes = Vec::new();
        let (mut acc, mut bits) = (0u32, 0u32);
        for c in text.chars().filter(|c| c.is_ascii_alphanumeric()) {
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            };
            let v = ALPHABET.iter().position(|a| *a as char == c).ok_or(Error::BadCode)? as u32;
            acc = ((acc << 5) | v) & 0xFFFF;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                bytes.push((acc >> bits) as u8);
            }
        }
        // 1 + 16 + 16 (+ 16) + 2
        if bytes.len() != 35 && bytes.len() != 51 {
            return Err(Error::BadCode);
        }
        let (body, check) = bytes.split_at(bytes.len() - 2);
        if body[0] != CODE_VERSION || Sha256::digest(body)[..2] != *check {
            return Err(Error::BadCode);
        }
        Ok(Code {
            fingerprint: body[1..17].try_into().expect("16 bytes"),
            auth: body[17..33].try_into().expect("16 bytes"),
            data: (body.len() == 49).then(|| body[33..49].try_into().expect("16 bytes")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_opens_only_with_its_key_and_only_untouched() {
        let key = crate::random::<32>();
        let boxed = seal(&key, b"the cards").unwrap();
        assert_eq!(open(&key, &boxed).unwrap(), b"the cards");
        assert!(looks_sealed(&boxed));
        // the same content never looks the same twice
        assert_ne!(boxed, seal(&key, b"the cards").unwrap());
        assert!(matches!(open(&crate::random::<32>(), &boxed), Err(Error::Locked)));
        for i in [0, 5, 30, boxed.len() - 1] {
            let mut bad = boxed.clone();
            bad[i] ^= 1;
            assert!(matches!(open(&key, &bad), Err(Error::Locked)), "a change at byte {i} must be noticed");
        }
        assert!(matches!(open(&key, &boxed[..boxed.len() - 1]), Err(Error::Locked)));
        assert!(matches!(open(&key, b"HZS1"), Err(Error::Locked)));
    }

    #[test]
    fn pairing_codes() {
        let code = Code::new(crate::random());
        let text = code.text();
        assert_eq!(Code::parse(&text).unwrap(), code);
        // typed in lower case, without the dashes, with the usual look-alikes
        let sloppy = text.replace('-', " ").to_lowercase().replace('0', "o").replace('1', "l");
        assert_eq!(Code::parse(&sloppy).unwrap(), code);
        // a server's own code has no key to the box
        let server = Code { data: None, ..code.clone() };
        assert_eq!(server.text().replace('-', "").len(), 56);
        assert_eq!(Code::parse(&server.text()).unwrap(), server);
        // one wrong character is refused, as is a code cut short
        let mut wrong: Vec<char> = text.chars().collect();
        wrong[3] = if wrong[3] == 'A' { 'B' } else { 'A' };
        assert!(matches!(Code::parse(&wrong.into_iter().collect::<String>()), Err(Error::BadCode)));
        assert!(matches!(Code::parse(&text[..text.len() - 4]), Err(Error::BadCode)));
        assert!(matches!(Code::parse("hello"), Err(Error::BadCode)));
        assert_ne!(data_key(&code.data.unwrap()), data_key(&crate::random()));
    }
}
