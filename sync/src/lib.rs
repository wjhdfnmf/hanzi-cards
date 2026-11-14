//! Keeping one learner's cards, stories and study history the same on several devices.
//!
//! - `snapshot`: what travels (rows with permanent ids).
//! - `merge`: two snapshots into one, keeping the latest of both. No clock, no database: the
//!   same two snapshots always give the same result, in either order.
//! - `seal`: the snapshot is encrypted on the device, so the server only ever holds a locked
//!   box; and the pairing code that carries the keys to another device.
//! - `store`: the server's side: the latest box, a counter, and a few earlier ones.
//! - `net`: TLS 1.3 between them, the client accepting only the server it was paired with.

pub mod merge;
pub mod net;
pub mod seal;
pub mod snapshot;
pub mod store;

pub use merge::merge;
pub use snapshot::Snapshot;

/// The port the server listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 47810;

/// The most a box may weigh (encrypted, compressed).
pub const MAX_BOX: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The connection couldn't be made secure.
    Tls(String),
    /// The server showed a certificate other than the one in the pairing code.
    WrongServer,
    /// The server doesn't know this pairing code.
    Auth,
    /// Too many wrong codes from this address: try again later.
    Blocked,
    /// Someone else synced in the meantime (the server is now at this generation).
    Conflict(u64),
    /// The data was written by a newer version of the app.
    Newer,
    /// The other side speaks a different version of the protocol.
    Protocol,
    TooLarge,
    Busy,
    /// The box can't be opened with this key, or isn't one.
    Locked,
    Corrupt(String),
    /// A pairing code that isn't one (mistyped, cut short).
    BadCode,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "connection problem: {e}"),
            Error::Tls(e) => write!(f, "the connection couldn't be secured: {e}"),
            Error::WrongServer => write!(f, "that isn't the server this device was paired with (its certificate is different)"),
            Error::Auth => write!(f, "the server doesn't accept this pairing code"),
            Error::Blocked => write!(f, "too many wrong codes from here: the server is refusing for a while"),
            Error::Conflict(_) => write!(f, "another device synced at the same moment"),
            Error::Newer => write!(f, "the data on the server was written by a newer version of Hanzi Cards: update this device first"),
            Error::Protocol => write!(f, "the server speaks a different version of the sync protocol"),
            Error::TooLarge => write!(f, "too much data"),
            Error::Busy => write!(f, "the server is busy: try again in a moment"),
            Error::Locked => write!(f, "the data on the server can't be opened with this pairing code"),
            Error::Corrupt(e) => write!(f, "the data is damaged: {e}"),
            Error::BadCode => write!(f, "that isn't a pairing code (check for a typing mistake)"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Random bytes from the operating system.
pub fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::getrandom(&mut out).expect("the operating system's random numbers");
    out
}

/// A new permanent id (128 random bits, as 32 hex digits).
pub fn new_uid() -> String {
    hex(&random::<16>())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
