//! Between a device and the server: TLS 1.3, and nothing else.
//!
//! - The server has a certificate it made itself. There is no certificate authority to
//!   trust or to fool: the pairing code carries the certificate's fingerprint, and the client
//!   talks only to a server that shows that certificate and proves it holds its key.
//! - Inside the encrypted connection the client gives the password from the pairing code;
//!   the server keeps only a hash of it. Wrong passwords are answered slowly, and an address
//!   that keeps sending them is refused for a while.
//! - One request per connection, everything length-limited and timed out, so that a
//!   stranger who finds the port can't make the server hold memory or wait for ever.
//!
//! What crosses is a sealed box (see `seal`): the connection hides it from the network, the
//! box itself hides it from the server.

use crate::store::BoxStore;
use crate::{seal, Error};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

/// Both sides must speak the same one.
pub const PROTOCOL: u32 = 1;

const HEADER_MAX: usize = 4096;
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// Connections served at once; more are turned away.
const MAX_ACTIVE: usize = 8;
/// Wrong passwords from one address before it is refused, and for how long at first
/// (doubling with each further one, up to an hour).
const TRIES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Request {
    v: u32,
    auth: String,
    /// "ping" | "get" | "put"
    op: String,
    /// put: the number of the box being replaced
    gen: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Response {
    ok: bool,
    error: String,
    gen: u64,
    time: i64,
    size: u64,
}

fn write_frame(w: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    w.write_all(&(bytes.len() as u32).to_be_bytes())?;
    w.write_all(bytes)
}

fn read_frame(r: &mut impl Read, max: usize) -> Result<Vec<u8>, Error> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(Error::TooLarge);
    }
    // (grown as the bytes arrive: a length alone doesn't get memory set aside)
    let mut out = Vec::with_capacity(len.min(1 << 20));
    r.take(len as u64).read_to_end(&mut out)?;
    if out.len() != len {
        return Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into()));
    }
    Ok(out)
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn tls(e: rustls::Error) -> Error {
    Error::Tls(e.to_string())
}

// ---------------------------------------------------------------- the server's certificate

/// A server's certificate and private key (DER).
#[derive(Clone)]
pub struct Identity {
    pub cert: Vec<u8>,
    pub key: Vec<u8>,
}

impl Identity {
    pub fn generate() -> Result<Identity, Error> {
        let made = rcgen::generate_simple_self_signed(vec!["hanzi-sync".to_string()]).map_err(|e| Error::Tls(e.to_string()))?;
        Ok(Identity { cert: made.cert.der().to_vec(), key: made.key_pair.serialize_der() })
    }

    pub fn fingerprint(&self) -> [u8; 16] {
        seal::fingerprint(&self.cert)
    }
}

// ---------------------------------------------------------------- server

struct Throttle {
    seen: HashMap<IpAddr, (u32, Instant)>,
}

impl Throttle {
    fn blocked(&self, ip: IpAddr) -> bool {
        self.seen.get(&ip).is_some_and(|(fails, until)| *fails >= TRIES && Instant::now() < *until)
    }

    fn failed(&mut self, ip: IpAddr) {
        if self.seen.len() > 4096 {
            let now = Instant::now();
            self.seen.retain(|_, (fails, until)| *fails >= TRIES && now < *until);
        }
        let entry = self.seen.entry(ip).or_insert((0, Instant::now()));
        entry.0 += 1;
        if entry.0 >= TRIES {
            let wait = LOCKOUT.saturating_mul(1 << (entry.0 - TRIES).min(7)).min(Duration::from_secs(3600));
            entry.1 = Instant::now() + wait;
        }
    }

    fn passed(&mut self, ip: IpAddr) {
        self.seen.remove(&ip);
    }
}

/// Called after a device has put a new box on the shelf (with its number).
pub type OnPut = Arc<dyn Fn(u64) + Send + Sync>;

struct Shared {
    tls: Arc<rustls::ServerConfig>,
    auth_hash: [u8; 32],
    store: Arc<BoxStore>,
    throttle: Mutex<Throttle>,
    active: AtomicUsize,
    on_put: Option<OnPut>,
}

/// A running server. Dropping it (or `stop`) closes the port.
pub struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub addr: SocketAddr,
}

impl Server {
    pub fn start(identity: &Identity, auth_hash: [u8; 32], store: Arc<BoxStore>, bind: SocketAddr, on_put: Option<OnPut>) -> Result<Server, Error> {
        let config = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(tls)?
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(identity.cert.clone())], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone())))
            .map_err(tls)?;
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            tls: Arc::new(config),
            auth_hash,
            store,
            throttle: Mutex::new(Throttle { seen: HashMap::new() }),
            active: AtomicUsize::new(0),
            on_put,
        });
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::Builder::new().name("hanzi-sync".into()).spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if shared.active.load(Ordering::Relaxed) >= MAX_ACTIVE {
                            continue; // (dropped: closed at once)
                        }
                        shared.active.fetch_add(1, Ordering::Relaxed);
                        let shared = shared.clone();
                        let spawned = std::thread::Builder::new().name("hanzi-sync-conn".into()).spawn({
                            let shared = shared.clone();
                            move || {
                                let _ = serve(&shared, stream, peer.ip());
                                shared.active.fetch_sub(1, Ordering::Relaxed);
                            }
                        });
                        if spawned.is_err() {
                            shared.active.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        })?;
        Ok(Server { stop, thread: Some(thread), addr })
    }

    pub fn stop(mut self) {
        self.close();
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.close();
    }
}

fn serve(shared: &Shared, stream: TcpStream, ip: IpAddr) -> Result<(), Error> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let _ = stream.set_nodelay(true);
    let conn = rustls::ServerConnection::new(shared.tls.clone()).map_err(tls)?;
    let mut tls = rustls::StreamOwned::new(conn, stream);
    let request: Request = serde_json::from_slice(&read_frame(&mut tls, HEADER_MAX)?).map_err(|e| Error::Corrupt(e.to_string()))?;

    let reply = |tls: &mut rustls::StreamOwned<rustls::ServerConnection, TcpStream>, r: Response, body: Option<&[u8]>| -> Result<(), Error> {
        let r = Response { time: crate::now(), ..r };
        write_frame(tls, &serde_json::to_vec(&r).map_err(|e| Error::Corrupt(e.to_string()))?)?;
        if let Some(body) = body {
            write_frame(tls, body)?;
        }
        tls.conn.send_close_notify();
        tls.flush()?;
        Ok(())
    };
    let refuse = |why: &str| Response { error: why.into(), ..Default::default() };

    if shared.throttle.lock().unwrap_or_else(|e| e.into_inner()).blocked(ip) {
        return reply(&mut tls, refuse("blocked"), None);
    }
    if request.v != PROTOCOL {
        return reply(&mut tls, refuse("protocol"), None);
    }
    // the password: hashed, then compared in a way that takes the same time right or wrong
    let given: Option<[u8; 16]> = crate::unhex(&request.auth).and_then(|b| b.try_into().ok());
    let right = given.is_some_and(|a| bool::from(seal::auth_hash(&a).ct_eq(&shared.auth_hash)));
    if !right {
        shared.throttle.lock().unwrap_or_else(|e| e.into_inner()).failed(ip);
        std::thread::sleep(Duration::from_millis(400));
        return reply(&mut tls, refuse("auth"), None);
    }
    shared.throttle.lock().unwrap_or_else(|e| e.into_inner()).passed(ip);

    match request.op.as_str() {
        "ping" => {
            let (gen, size) = shared.store.current()?;
            reply(&mut tls, Response { ok: true, gen, size, ..Default::default() }, None)
        }
        "get" => {
            let (gen, boxed) = shared.store.get()?;
            reply(&mut tls, Response { ok: true, gen, size: boxed.len() as u64, ..Default::default() }, Some(&boxed))
        }
        "put" => {
            let boxed = match read_frame(&mut tls, crate::MAX_BOX) {
                Ok(b) => b,
                Err(Error::TooLarge) => return reply(&mut tls, refuse("too_large"), None),
                Err(e) => return Err(e),
            };
            match shared.store.put(request.gen, &boxed) {
                Ok(gen) => {
                    reply(&mut tls, Response { ok: true, gen, size: boxed.len() as u64, ..Default::default() }, None)?;
                    if let Some(f) = &shared.on_put {
                        f(gen);
                    }
                    Ok(())
                }
                Err(Error::Conflict(gen)) => reply(&mut tls, Response { error: "conflict".into(), gen, ..Default::default() }, None),
                Err(Error::Corrupt(_)) => reply(&mut tls, refuse("bad_box"), None),
                Err(Error::TooLarge) => reply(&mut tls, refuse("too_large"), None),
                Err(e) => {
                    let _ = reply(&mut tls, refuse("error"), None);
                    Err(e)
                }
            }
        }
        _ => reply(&mut tls, refuse("protocol"), None),
    }
}

// ---------------------------------------------------------------- client

/// Accepts exactly one certificate: the one whose fingerprint is in the pairing code. The
/// server must also prove it holds that certificate's key (the handshake signature).
#[derive(Debug)]
struct Pinned {
    fingerprint: [u8; 16],
    algorithms: WebPkiSupportedAlgorithms,
    mismatch: Arc<AtomicBool>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        if bool::from(seal::fingerprint(end_entity.as_ref()).ct_eq(&self.fingerprint)) {
            Ok(ServerCertVerified::assertion())
        } else {
            self.mismatch.store(true, Ordering::Relaxed);
            Err(rustls::Error::General("not the paired server's certificate".into()))
        }
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &rustls::DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.3 only".into()))
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &rustls::DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// What the server says about its shelf.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Info {
    /// The number of the current box (0: none yet).
    pub generation: u64,
    /// The server's clock (seconds).
    pub time: i64,
    pub size: u64,
    /// How long the exchange took.
    pub ms: u64,
}

/// A server somewhere else.
#[derive(Clone)]
pub struct Remote {
    /// "host", "host:port", "[v6]:port"
    pub address: String,
    pub fingerprint: [u8; 16],
    pub auth: [u8; 16],
}

/// "host" -> "host:47810"; an address that names its port is left alone.
pub fn with_port(address: &str) -> String {
    let a = address.trim();
    let has_port = match a.rfind(':') {
        None => false,
        Some(i) => !a[i + 1..].is_empty() && a[i + 1..].bytes().all(|b| b.is_ascii_digit()) && (a.starts_with('[') == a[..i].ends_with(']')) && (a.starts_with('[') || !a[..i].contains(':')),
    };
    if has_port {
        a.to_string()
    } else {
        format!("{a}:{}", crate::DEFAULT_PORT)
    }
}

impl Remote {
    fn call(&self, op: &str, gen: u64, body: Option<&[u8]>) -> Result<(Info, Vec<u8>), Error> {
        if body.is_some_and(|b| b.len() > crate::MAX_BOX) {
            return Err(Error::TooLarge);
        }
        let started = Instant::now();
        let mut last: Option<std::io::Error> = None;
        let mut stream = None;
        for addr in with_port(&self.address).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let stream = stream.ok_or_else(|| Error::Io(last.unwrap_or_else(|| std::io::ErrorKind::AddrNotAvailable.into())))?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        let _ = stream.set_nodelay(true);

        let mismatch = Arc::new(AtomicBool::new(false));
        let provider = provider();
        let verifier = Pinned { fingerprint: self.fingerprint, algorithms: provider.signature_verification_algorithms, mismatch: mismatch.clone() };
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(tls)?
            .dangerous() // (not the usual list of authorities: one known certificate, see `Pinned`)
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        let name = ServerName::try_from("hanzi-sync").map_err(|e| Error::Tls(e.to_string()))?;
        let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(tls)?;
        let mut tls = rustls::StreamOwned::new(conn, stream);

        let exchange = |tls: &mut rustls::StreamOwned<rustls::ClientConnection, TcpStream>| -> Result<(Response, Vec<u8>), Error> {
            let request = Request { v: PROTOCOL, auth: crate::hex(&self.auth), op: op.into(), gen };
            write_frame(tls, &serde_json::to_vec(&request).map_err(|e| Error::Corrupt(e.to_string()))?)?;
            if let Some(body) = body {
                write_frame(tls, body)?;
            }
            tls.flush()?;
            let response: Response = serde_json::from_slice(&read_frame(tls, HEADER_MAX)?).map_err(|e| Error::Corrupt(e.to_string()))?;
            let boxed = if response.ok && op == "get" { read_frame(tls, crate::MAX_BOX)? } else { Vec::new() };
            Ok((response, boxed))
        };
        let (response, boxed) = exchange(&mut tls).map_err(|e| if mismatch.load(Ordering::Relaxed) { Error::WrongServer } else { e })?;
        tls.conn.send_close_notify();
        let _ = tls.flush();
        if !response.ok {
            return Err(match response.error.as_str() {
                "auth" => Error::Auth,
                "blocked" => Error::Blocked,
                "conflict" => Error::Conflict(response.gen),
                "too_large" => Error::TooLarge,
                "bad_box" => Error::Corrupt("the server refused the box".into()),
                "protocol" => Error::Protocol,
                _ => Error::Busy,
            });
        }
        Ok((Info { generation: response.gen, time: response.time, size: response.size, ms: started.elapsed().as_millis() as u64 }, boxed))
    }

    pub fn ping(&self) -> Result<Info, Error> {
        Ok(self.call("ping", 0, None)?.0)
    }

    /// The current box (empty when there is none yet).
    pub fn get(&self) -> Result<(Info, Vec<u8>), Error> {
        self.call("get", 0, None)
    }

    /// Replace the box numbered `expected`.
    pub fn put(&self, expected: u64, boxed: &[u8]) -> Result<Info, Error> {
        Ok(self.call("put", expected, Some(boxed))?.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::Code;

    struct Rig {
        server: Option<Server>,
        remote: Remote,
        identity: Identity,
        store: Arc<BoxStore>,
        dir: std::path::PathBuf,
        puts: Arc<Mutex<Vec<u64>>>,
    }

    fn rig() -> Rig {
        let dir = crate::store::temp_dir("net");
        let identity = Identity::generate().unwrap();
        let code = Code::new(identity.fingerprint());
        let store = Arc::new(BoxStore::open(&dir).unwrap());
        let puts = Arc::new(Mutex::new(Vec::new()));
        let seen = puts.clone();
        let server = Server::start(&identity, seal::auth_hash(&code.auth), store.clone(), "127.0.0.1:0".parse().unwrap(), Some(Arc::new(move |g| seen.lock().unwrap().push(g)))).unwrap();
        let remote = Remote { address: server.addr.to_string(), fingerprint: code.fingerprint, auth: code.auth };
        Rig { server: Some(server), remote, identity, store, dir, puts }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            self.server.take();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_box_goes_there_and_back() {
        let rig = rig();
        let key = crate::random::<32>();
        let info = rig.remote.ping().unwrap();
        assert_eq!(info.generation, 0);
        assert!((info.time - crate::now()).abs() < 5);
        assert_eq!(rig.remote.get().unwrap().1, Vec::<u8>::new());

        let first = seal::seal(&key, &vec![7u8; 300_000]).unwrap();
        assert_eq!(rig.remote.put(0, &first).unwrap().generation, 1);
        let (info, back) = rig.remote.get().unwrap();
        assert_eq!((info.generation, &back), (1, &first));
        assert_eq!(seal::open(&key, &back).unwrap(), vec![7u8; 300_000]);

        // a device that hasn't seen box 1 is told to look again
        assert!(matches!(rig.remote.put(0, &seal::seal(&key, b"late").unwrap()), Err(Error::Conflict(1))));
        assert_eq!(rig.remote.put(1, &seal::seal(&key, b"next").unwrap()).unwrap().generation, 2);
        // the server takes only things shaped like a box
        assert!(matches!(rig.remote.put(2, b"just some bytes, not sealed at all......................"), Err(Error::Corrupt(_))));
        assert_eq!(rig.remote.ping().unwrap().generation, 2);
        assert_eq!(*rig.puts.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn only_the_paired_server_is_talked_to() {
        let rig = rig();
        // someone else's server at the same address: another certificate
        let wrong = Remote { fingerprint: crate::random(), ..rig.remote.clone() };
        assert!(matches!(wrong.ping(), Err(Error::WrongServer)));
        assert!(matches!(wrong.put(0, &seal::seal(&[1; 32], b"x").unwrap()), Err(Error::WrongServer)));
        // an impostor showing the right certificate without holding its key can't finish the
        // handshake: here, a server with the same certificate and another key
        let other = Identity::generate().unwrap();
        let fake = Identity { cert: rig.identity.cert.clone(), key: other.key };
        let impostor = Server::start(&fake, [0; 32], rig.store.clone(), "127.0.0.1:0".parse().unwrap(), None);
        if let Ok(impostor) = impostor {
            let r = Remote { address: impostor.addr.to_string(), ..rig.remote.clone() };
            assert!(r.ping().is_err());
        }
        assert!(rig.remote.ping().is_ok());
    }

    #[test]
    fn wrong_passwords_are_refused_then_locked_out() {
        let rig = rig();
        let wrong = Remote { auth: crate::random(), ..rig.remote.clone() };
        for _ in 0..TRIES {
            assert!(matches!(wrong.ping(), Err(Error::Auth)));
        }
        assert!(matches!(wrong.ping(), Err(Error::Blocked)));
        // (from this address even the right password waits now)
        assert!(matches!(rig.remote.ping(), Err(Error::Blocked)));
        assert_eq!(rig.store.current().unwrap().0, 0);
    }

    #[test]
    fn strangers_on_the_port_dont_hurt() {
        let rig = rig();
        let addr = rig.server.as_ref().unwrap().addr;
        // not TLS at all
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut sink = Vec::new();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let _ = s.read_to_end(&mut sink);
        // connects and says nothing, then leaves
        drop(TcpStream::connect(addr).unwrap());
        // another protocol version
        assert!(rig.remote.ping().is_ok());
        assert_eq!(rig.store.current().unwrap().0, 0);
    }

    #[test]
    fn addresses() {
        assert_eq!(with_port("192.168.1.20"), "192.168.1.20:47810");
        assert_eq!(with_port("192.168.1.20:5000"), "192.168.1.20:5000");
        assert_eq!(with_port(" nas.local "), "nas.local:47810");
        assert_eq!(with_port("[fe80::1]"), "[fe80::1]:47810");
        assert_eq!(with_port("[fe80::1]:5000"), "[fe80::1]:5000");
    }
}
