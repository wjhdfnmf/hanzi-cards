//! Keeping this device the same as the learner's others (the console behind Settings).
//!
//! A device is a **client** of a server somewhere, or is itself the **server** (and still
//! studies like any other device). Either way a sync is the same few steps: read everything
//! here, fetch the sealed box, open it, merge the two, write the result here, seal it and put
//! it back. The server side only stores the box (`hanzi_sync::store`); a device that is the
//! server uses its own shelf directly.
//!
//! Nothing here can leave a database half-changed: the databases are checked first, backed
//! up before anything is written, written in one transaction each, and checked again. If
//! another device synced in between, the server says so and the steps are done again.
//!
//! The keys (the server's password, the key to the box, a server's private key) are kept in
//! the system keychain, never in a file.

use crate::data::{self, Kind, Made};
use crate::error::{AppError, AppResult};
use crate::sync_data::{apply_cards, apply_stories, export};
use crate::AppState;
use hanzi_sync::net::{Identity, Remote, Server};
use hanzi_sync::seal::{self, Code};
use hanzi_sync::store::BoxStore;
use hanzi_sync::{hex, merge, unhex, Snapshot};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

/// "Latest wins" compares clocks, so the two sides' clocks must roughly agree.
const MAX_SKEW: i64 = 300;

fn err(e: hanzi_sync::Error) -> AppError {
    AppError::msg(e.to_string())
}

// ---------------------------------------------------------------- one sync

/// Where the box is: a server elsewhere, or this device's own shelf.
pub trait Shelf {
    /// The box's number (0: none yet), the shelf's clock, the box.
    fn get(&self) -> Result<(u64, i64, Vec<u8>), hanzi_sync::Error>;
    fn put(&self, expected: u64, boxed: &[u8]) -> Result<u64, hanzi_sync::Error>;
}

impl Shelf for Remote {
    fn get(&self) -> Result<(u64, i64, Vec<u8>), hanzi_sync::Error> {
        let (info, boxed) = Remote::get(self)?;
        Ok((info.generation, info.time, boxed))
    }
    fn put(&self, expected: u64, boxed: &[u8]) -> Result<u64, hanzi_sync::Error> {
        Ok(Remote::put(self, expected, boxed)?.generation)
    }
}

impl Shelf for BoxStore {
    fn get(&self) -> Result<(u64, i64, Vec<u8>), hanzi_sync::Error> {
        let (generation, boxed) = BoxStore::get(self)?;
        Ok((generation, hanzi_sync::now(), boxed))
    }
    fn put(&self, expected: u64, boxed: &[u8]) -> Result<u64, hanzi_sync::Error> {
        BoxStore::put(self, expected, boxed)
    }
}

/// What a sync changed, one way.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub decks: usize,
    pub cards: usize,
    pub reviews: usize,
    pub stories: usize,
    pub removed: usize,
}

impl Changes {
    /// What is in `to` that `from` didn't have (or had differently), and what `from` had
    /// that `to` doesn't.
    fn between(from: &Snapshot, to: &Snapshot) -> Changes {
        fn count<T: PartialEq>(from: &[T], to: &[T], uid: impl Fn(&T) -> &str) -> (usize, usize) {
            let old: std::collections::HashMap<&str, &T> = from.iter().map(|x| (uid(x), x)).collect();
            let new: std::collections::HashSet<&str> = to.iter().map(&uid).collect();
            (to.iter().filter(|x| old.get(uid(x)).is_none_or(|o| *o != *x)).count(), from.iter().filter(|x| !new.contains(uid(x))).count())
        }
        let decks = count(&from.decks, &to.decks, |d| &d.uid);
        let folders = count(&from.folders, &to.folders, |f| &f.uid);
        let cards = count(&from.cards, &to.cards, |c| &c.uid);
        let stories = count(&from.stories, &to.stories, |s| &s.uid);
        let reviews = count(&from.log, &to.log, |r| &r.uid);
        Changes { decks: decks.0 + folders.0, cards: cards.0, reviews: reviews.0, stories: stories.0, removed: decks.1 + folders.1 + cards.1 + stories.1 + reviews.1 }
    }

    #[cfg(test)]
    fn any(&self) -> bool {
        *self != Changes::default()
    }
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// What came in to this device, and what went out to the box.
    pub received: Changes,
    pub sent: Changes,
    pub generation: u64,
    /// What this device holds now.
    pub cards: usize,
    pub reviews: usize,
    pub stories: usize,
}

fn lock<'a>(m: &'a Mutex<Connection>) -> std::sync::MutexGuard<'a, Connection> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sync once with the box on `shelf` (see the top of this file).
pub fn sync_once(db: &Mutex<Connection>, stories: &Mutex<Connection>, app_data: &Path, shelf: &dyn Shelf, key: &[u8; 32]) -> AppResult<Report> {
    let mut backed_up = false;
    for _ in 0..5 {
        // what this device has (both databases sound, or nothing goes anywhere)
        let (local, mark) = {
            let (c, s) = (lock(db), lock(stories));
            data::check(&c)?;
            data::check(&s)?;
            (export(&c, &s)?, (c.total_changes(), s.total_changes()))
        };
        // the box
        let (generation, their_time, boxed) = shelf.get().map_err(err)?;
        let skew = their_time - hanzi_sync::now();
        if skew.abs() > MAX_SKEW {
            return Err(AppError::msg(format!(
                "This device's clock and the server's differ by {} minutes. Sync keeps whatever was changed last, so the clocks must agree: set the date and time, then try again. Nothing was changed.",
                (skew.abs() + 30) / 60
            )));
        }
        let remote = if generation == 0 { Snapshot::default() } else { Snapshot::unpack(&seal::open(key, &boxed).map_err(err)?).map_err(err)? };
        remote.check(crate::db::SCHEMA_VERSION, crate::stories::SCHEMA_VERSION).map_err(err)?;

        let merged = merge(&local, &remote);

        // in: only if there is something to take, and nothing was done here meanwhile
        if merged != local {
            let (mut c, mut s) = (lock(db), lock(stories));
            if (c.total_changes(), s.total_changes()) != mark {
                continue;
            }
            if !backed_up {
                data::backup(&c, app_data, Kind::Cards, Made::Sync)?;
                data::backup(&s, app_data, Kind::Reading, Made::Sync)?;
                backed_up = true;
            }
            apply_cards(&mut c, &merged)?;
            apply_stories(&mut s, &merged)?;
            data::check(&c)?;
            data::check(&s)?;
        }
        // out: only if the box lacks something; someone else got there first -> again
        let generation = if merged != remote {
            let sealed = seal::seal(key, &merged.pack().map_err(err)?).map_err(err)?;
            match shelf.put(generation, &sealed) {
                Ok(g) => g,
                Err(hanzi_sync::Error::Conflict(_)) => continue,
                Err(e) => return Err(err(e)),
            }
        } else {
            generation
        };
        return Ok(Report {
            received: Changes::between(&local, &merged),
            sent: Changes::between(&remote, &merged),
            generation,
            cards: merged.cards.len(),
            reviews: merged.log.len(),
            stories: merged.stories.len(),
        });
    }
    Err(AppError::msg("Other devices kept syncing at the same moment. Try again."))
}

// ---------------------------------------------------------------- what is set up

/// What is set up, without the secrets (those are in the keychain).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// "" (nothing set up) | "client" | "server"
    pub role: String,
    /// client: where the server is
    pub address: String,
    /// server: the port it listens on, its certificate, and whether it should be listening
    pub port: u16,
    pub cert: String,
    pub online: bool,
    /// the fingerprint of the server's certificate (a client's only knowledge of it)
    pub fingerprint: String,
    pub last_sync: i64,
    pub last_generation: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Secrets {
    auth: String,
    data: String,
    /// server: its private key
    key: String,
}

fn sync_dir(app_data: &Path) -> PathBuf {
    app_data.join("sync")
}

pub fn load_config(app_data: &Path) -> Config {
    std::fs::read(sync_dir(app_data).join("config.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_config(app_data: &Path, config: &Config) -> AppResult<()> {
    let dir = sync_dir(app_data);
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("config.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(config).map_err(|e| AppError::msg(e.to_string()))?)?;
    std::fs::rename(&tmp, dir.join("config.json"))?;
    Ok(())
}

const KEYRING_SERVICE: &str = "com.hanzicards.app";
const KEYRING_USER: &str = "sync";

fn keychain() -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| AppError::msg(format!("Keychain error: {e}")))
}

fn load_secrets() -> AppResult<Secrets> {
    match keychain()?.get_password() {
        Ok(text) => serde_json::from_str(&text).map_err(|_| AppError::msg("The sync keys in the keychain can't be read. Type `forget`, then set sync up again.")),
        Err(keyring::Error::NoEntry) => Err(AppError::msg("The sync keys are missing from the keychain. Type `forget`, then set sync up again.")),
        Err(e) => Err(AppError::msg(format!("Keychain error: {e}"))),
    }
}

fn save_secrets(s: &Secrets) -> AppResult<()> {
    keychain()?.set_password(&serde_json::to_string(s).map_err(|e| AppError::msg(e.to_string()))?).map_err(|e| AppError::msg(format!("Keychain error: {e}")))
}

fn clear_secrets() -> AppResult<()> {
    match keychain()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::msg(format!("Keychain error: {e}"))),
    }
}

fn bytes<const N: usize>(hex_text: &str) -> AppResult<[u8; N]> {
    unhex(hex_text).and_then(|b| b.try_into().ok()).ok_or_else(|| AppError::msg("The sync setup is damaged. Type `forget`, then set sync up again."))
}

/// What runs while the app does: the server (when this device is one and is online), and
/// whether a device has put a box on the shelf that hasn't been merged in here yet.
#[derive(Default)]
pub struct Runtime {
    server: Mutex<Option<Server>>,
    /// One shelf for the server and for this device's own syncs: its lock is what keeps two
    /// boxes from being put at once.
    store: Mutex<Option<Arc<BoxStore>>>,
    pending: Arc<AtomicBool>,
    /// One sync at a time.
    busy: Mutex<()>,
}

impl Runtime {
    fn store(&self, app_data: &Path) -> AppResult<Arc<BoxStore>> {
        let mut slot = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = slot.as_ref() {
            return Ok(s.clone());
        }
        let store = Arc::new(BoxStore::open(&sync_dir(app_data).join("boxes")).map_err(err)?);
        *slot = Some(store.clone());
        Ok(store)
    }

    fn listening(&self) -> bool {
        self.server.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    fn listen(&self, app_data: &Path, config: &Config, secrets: &Secrets) -> AppResult<()> {
        let mut slot = self.server.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Ok(());
        }
        let identity = Identity { cert: unhex(&config.cert).ok_or_else(|| AppError::msg("The sync setup is damaged."))?, key: unhex(&secrets.key).ok_or_else(|| AppError::msg("The sync setup is damaged."))? };
        let pending = self.pending.clone();
        let on_put: hanzi_sync::net::OnPut = Arc::new(move |_| pending.store(true, Ordering::Relaxed));
        let bind = std::net::SocketAddr::from(([0, 0, 0, 0], config.port));
        let server = Server::start(&identity, seal::auth_hash(&bytes(&secrets.auth)?), self.store(app_data)?, bind, Some(on_put))
            .map_err(|e| AppError::msg(format!("Couldn't open port {} ({e}).", config.port)))?;
        *slot = Some(server);
        Ok(())
    }

    fn hang_up(&self) {
        self.server.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

/// At startup: a server that was online goes online again.
pub fn start(runtime: &Runtime, app_data: &Path) {
    let config = load_config(app_data);
    if config.role == "server" && config.online {
        if let Err(e) = load_secrets().and_then(|s| runtime.listen(app_data, &config, &s)) {
            eprintln!("sync server: {e}");
        }
    }
}

/// One of this computer's addresses, and what sort of network it is on.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalAddress {
    pub address: String,
    /// The network adapter's name ("Wi-Fi", "Ethernet", "NordLynx").
    pub name: String,
    /// "home": an ordinary network other devices in the house are on. "vpn": a tunnel (the
    /// other devices can't reach this address). "other": a virtual or unusual adapter.
    pub kind: &'static str,
}

/// What sort of network an adapter is on, from its name, its address and whether it is a
/// point-to-point link. A guess: the system doesn't say "this one is a VPN".
fn kind_of(name: &str, ip: std::net::Ipv4Addr, point_to_point: bool) -> &'static str {
    let n = name.to_lowercase();
    let o = ip.octets();
    let tunnel_names = [
        "vpn", "wireguard", "nordlynx", "nord", "proton", "tailscale", "zerotier", "mullvad", "expressvpn", "surfshark", "wintun", "hamachi", "anyconnect", "globalprotect",
        "fortinet", "forticlient", "ipsec", "pptp", "l2tp", "tunnel", "warp",
    ];
    let tunnel_starts = ["tun", "tap", "wg", "utun", "ppp", "ipsec"];
    // 100.64.0.0/10 is what Tailscale and carrier networks hand out: not the house's network
    if point_to_point || tunnel_names.iter().any(|t| n.contains(t)) || tunnel_starts.iter().any(|t| n.starts_with(t)) || (o[0] == 100 && (64..128).contains(&o[1])) {
        return "vpn";
    }
    let virtual_names = ["vethernet", "virtualbox", "vmware", "docker", "wsl", "hyper-v", "vboxnet", "veth", "virbr", "bluetooth", "loopback"];
    if virtual_names.iter().any(|v| n.contains(v)) || n.starts_with("br-") || !ip.is_private() {
        return "other";
    }
    "home"
}

/// The addresses in the order to try them: the house's network first (192.168 before the
/// rest), then anything unusual, then tunnels.
fn in_order(mut found: Vec<LocalAddress>) -> Vec<LocalAddress> {
    let rank = |a: &LocalAddress| {
        let kind = match a.kind {
            "home" => 0,
            "other" => 1,
            _ => 2,
        };
        let range = if a.address.starts_with("192.168.") { 0 } else if a.address.starts_with("10.") { 1 } else { 2 };
        (kind, range, a.address.clone())
    };
    found.sort_by_key(rank);
    found.dedup_by(|b, a| a.address == b.address);
    found
}

/// Every address this computer has on a network that is up (IPv4; not the loopback, not the
/// self-assigned 169.254 ones). The system's list of its adapters: nothing is sent anywhere.
fn local_addresses() -> Vec<LocalAddress> {
    let found = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback() && i.is_oper_up())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_link_local() && !ip.is_unspecified() => {
                Some(LocalAddress { address: ip.to_string(), kind: kind_of(&i.name, ip, i.is_p2p()), name: i.name })
            }
            _ => None,
        })
        .collect();
    in_order(found)
}

/// The address to tell the other devices in the house: the first one that looks like the
/// house's network, else whatever there is.
fn local_address() -> Option<String> {
    local_addresses().into_iter().next().map(|a| a.address)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub role: String,
    pub address: String,
    pub port: u16,
    /// server: listening now
    pub online: bool,
    /// server: a device has left a box that hasn't been merged in here
    pub pending: bool,
    pub last_sync: i64,
    pub generation: u64,
    pub local_address: Option<String>,
}

fn status_of(runtime: &Runtime, app_data: &Path) -> Status {
    let config = load_config(app_data);
    Status {
        online: runtime.listening(),
        pending: config.role == "server" && runtime.pending.load(Ordering::Relaxed),
        local_address: if config.role == "server" { local_address() } else { None },
        role: config.role,
        address: config.address,
        port: config.port,
        last_sync: config.last_sync,
        generation: config.last_generation,
    }
}

fn remote_of(config: &Config, secrets: &Secrets) -> AppResult<Remote> {
    Ok(Remote { address: config.address.clone(), fingerprint: bytes(&config.fingerprint)?, auth: bytes(&secrets.auth)? })
}

fn not_set_up() -> AppError {
    AppError::msg("Sync isn't set up on this device. Type `host` to make this device the server, or `join <address> <code>`.")
}

// ---------------------------------------------------------------- commands (the console and the sidebar button)

fn dir_of(app: &AppHandle) -> AppResult<PathBuf> {
    app.path().app_data_dir().map_err(|e| AppError::msg(e.to_string()))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn sync_status(app: AppHandle) -> AppResult<Status> {
    Ok(status_of(&app.state::<AppState>().sync, &dir_of(&app)?))
}

/// Make this device the server. Returns the pairing code for the other devices.
#[tauri::command]
pub async fn sync_host(app: AppHandle, port: Option<u16>) -> AppResult<String> {
    blocking(move || {
        let dir = dir_of(&app)?;
        if !load_config(&dir).role.is_empty() {
            return Err(AppError::msg("Sync is already set up on this device. Type `forget` first to start over."));
        }
        let identity = Identity::generate().map_err(err)?;
        let code = Code::new(identity.fingerprint());
        let secrets = Secrets { auth: hex(&code.auth), data: hex(&code.data.expect("a new code has a key")), key: hex(&identity.key) };
        let config = Config {
            role: "server".into(),
            port: port.unwrap_or(hanzi_sync::DEFAULT_PORT),
            cert: hex(&identity.cert),
            fingerprint: hex(&code.fingerprint),
            online: true,
            ..Default::default()
        };
        let state = app.state::<AppState>();
        // the port first: if it can't be opened, nothing is set up
        state.sync.listen(&dir, &config, &secrets)?;
        if let Err(e) = save_secrets(&secrets).and_then(|_| save_config(&dir, &config)) {
            state.sync.hang_up();
            let _ = clear_secrets();
            return Err(e);
        }
        Ok(code.text())
    })
    .await
}

/// Pair this device with a server. With a standalone server's own code (which has no key to
/// the box) on an empty server, this device makes the key: `sync_code` then shows the full
/// code for the other devices.
#[tauri::command]
pub async fn sync_join(app: AppHandle, address: String, code: String) -> AppResult<Status> {
    blocking(move || {
        let dir = dir_of(&app)?;
        if !load_config(&dir).role.is_empty() {
            return Err(AppError::msg("Sync is already set up on this device. Type `forget` first to start over."));
        }
        let code = Code::parse(&code).map_err(err)?;
        let address = address.trim().to_string();
        if address.is_empty() || address.len() > 255 || address.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(AppError::msg("That isn't an address (for example 192.168.1.20 or nas.local:47810)."));
        }
        let remote = Remote { address: address.clone(), fingerprint: code.fingerprint, auth: code.auth };
        let info = remote.ping().map_err(err)?;
        let data = match code.data {
            Some(d) => d,
            None if info.generation == 0 => hanzi_sync::random(),
            None => {
                return Err(AppError::msg(
                    "That is the server's own code, and the server already holds data from another device. Use the full code instead: type `code` on a device that is already paired.",
                ))
            }
        };
        save_secrets(&Secrets { auth: hex(&code.auth), data: hex(&data), key: String::new() })?;
        save_config(&dir, &Config { role: "client".into(), address, fingerprint: hex(&code.fingerprint), ..Default::default() })?;
        Ok(status_of(&app.state::<AppState>().sync, &dir))
    })
    .await
}

/// The full pairing code, to type into another device.
#[tauri::command]
pub async fn sync_code(app: AppHandle) -> AppResult<String> {
    blocking(move || {
        let config = load_config(&dir_of(&app)?);
        if config.role.is_empty() {
            return Err(not_set_up());
        }
        let secrets = load_secrets()?;
        Ok(Code { fingerprint: bytes(&config.fingerprint)?, auth: bytes(&secrets.auth)?, data: Some(bytes(&secrets.data)?) }.text())
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addresses {
    /// On the local network (what devices at home use): every address this computer has,
    /// the likeliest first, each with its adapter's name and whether it looks like a VPN.
    pub local: Vec<LocalAddress>,
    /// As seen from the internet (what a device away from home would use, with the port
    /// forwarded to this computer).
    pub public: Option<String>,
    /// Why the public address isn't known, when it isn't.
    pub public_error: String,
    pub port: u16,
}

/// The address this computer appears from on the internet. The only way to know is to ask
/// someone outside: two services that answer with the caller's address and nothing else.
/// Only asked for when the learner types `ip`; the request says nothing about the app.
fn public_address() -> Result<String, String> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(8)).build();
    let mut last = String::from("no answer");
    for url in ["https://api.ipify.org", "https://icanhazip.com"] {
        match agent.get(url).call().map_err(|e| e.to_string()).and_then(|r| r.into_string().map_err(|e| e.to_string())) {
            // (only ever an address: anything else a service sends back is not shown)
            Ok(text) => match text.trim().parse::<std::net::IpAddr>() {
                Ok(ip) => return Ok(ip.to_string()),
                Err(_) => last = "an answer that wasn't an address".into(),
            },
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// This computer's addresses: on the local network and as seen from outside.
#[tauri::command]
pub async fn sync_addresses(app: AppHandle) -> AppResult<Addresses> {
    blocking(move || {
        let config = load_config(&dir_of(&app)?);
        let (public, public_error) = match public_address() {
            Ok(ip) => (Some(ip), String::new()),
            Err(e) => (None, e),
        };
        Ok(Addresses { local: local_addresses(), public, public_error, port: if config.port == 0 { hanzi_sync::DEFAULT_PORT } else { config.port } })
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pong {
    pub ok: bool,
    pub ms: u64,
    pub generation: u64,
    pub error: String,
}

/// Is the server there (client), or is this server listening (server)?
#[tauri::command]
pub async fn sync_ping(app: AppHandle) -> AppResult<Pong> {
    blocking(move || {
        let dir = dir_of(&app)?;
        let config = load_config(&dir);
        let state = app.state::<AppState>();
        let pong = |r: Result<(u64, u64), String>| match r {
            Ok((ms, generation)) => Pong { ok: true, ms, generation, error: String::new() },
            Err(error) => Pong { ok: false, ms: 0, generation: 0, error },
        };
        Ok(match config.role.as_str() {
            "client" => pong(load_secrets().and_then(|s| remote_of(&config, &s)).map_err(|e| e.to_string()).and_then(|r| r.ping().map(|i| (i.ms, i.generation)).map_err(|e| e.to_string()))),
            "server" => {
                if state.sync.listening() {
                    pong(state.sync.store(&dir).and_then(|s| s.current().map_err(err)).map(|(g, _)| (0, g)).map_err(|e| e.to_string()))
                } else {
                    pong(Err("offline (type `online` to listen for your devices)".into()))
                }
            }
            _ => return Err(not_set_up()),
        })
    })
    .await
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> AppResult<Report> {
    blocking(move || {
        let dir = dir_of(&app)?;
        let mut config = load_config(&dir);
        if config.role.is_empty() {
            return Err(not_set_up());
        }
        let state = app.state::<AppState>();
        if state.startup_problem.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return Err(AppError::msg("A database couldn't be opened when the app started. Sort that out first; nothing was synced."));
        }
        let Ok(_one) = state.sync.busy.try_lock() else {
            return Err(AppError::msg("A sync is already running."));
        };
        let secrets = load_secrets()?;
        let key = seal::data_key(&bytes(&secrets.data)?);
        let report = if config.role == "server" {
            let store = state.sync.store(&dir)?;
            // (cleared first: a box that arrives while this runs counts as new)
            state.sync.pending.store(false, Ordering::Relaxed);
            sync_once(&state.db, &state.stories, &dir, store.as_ref(), &key).inspect_err(|_| state.sync.pending.store(true, Ordering::Relaxed))?
        } else {
            sync_once(&state.db, &state.stories, &dir, &remote_of(&config, &secrets)?, &key)?
        };
        config.last_sync = hanzi_sync::now();
        config.last_generation = report.generation;
        save_config(&dir, &config)?;
        Ok(report)
    })
    .await
}

/// Server: listen for the other devices, or stop.
#[tauri::command]
pub async fn sync_online(app: AppHandle, on: bool) -> AppResult<Status> {
    blocking(move || {
        let dir = dir_of(&app)?;
        let mut config = load_config(&dir);
        if config.role != "server" {
            return Err(AppError::msg("Only the server goes online or offline. This device is a client: `ping` checks the server."));
        }
        let state = app.state::<AppState>();
        if on {
            state.sync.listen(&dir, &config, &load_secrets()?)?;
        } else {
            state.sync.hang_up();
        }
        config.online = on;
        save_config(&dir, &config)?;
        Ok(status_of(&state.sync, &dir))
    })
    .await
}

/// Undo the setup on this device: the keys, the server and its shelf. The cards and stories
/// stay as they are.
#[tauri::command]
pub async fn sync_forget(app: AppHandle) -> AppResult<()> {
    blocking(move || {
        let dir = dir_of(&app)?;
        let state = app.state::<AppState>();
        state.sync.hang_up();
        *state.sync.store.lock().unwrap_or_else(|e| e.into_inner()) = None;
        state.sync.pending.store(false, Ordering::Relaxed);
        clear_secrets()?;
        let sync = sync_dir(&dir);
        if sync.exists() {
            std::fs::remove_dir_all(&sync)?;
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, stories};

    struct Device {
        db: Mutex<Connection>,
        stories: Mutex<Connection>,
        dir: PathBuf,
    }

    fn device(name: &str) -> Device {
        let dir = std::env::temp_dir().join(format!("hanzi-sync-app-{name}-{}-{}", std::process::id(), hex(&hanzi_sync::random::<4>())));
        std::fs::create_dir_all(&dir).unwrap();
        Device { db: Mutex::new(db::open(&dir.join(data::DB_FILE)).unwrap()), stories: Mutex::new(stories::open(&dir.join(stories::DB_FILE)).unwrap()), dir }
    }

    impl Device {
        fn sync(&self, shelf: &dyn Shelf, key: &[u8; 32]) -> AppResult<Report> {
            sync_once(&self.db, &self.stories, &self.dir, shelf, key)
        }
        fn count(&self, sql: &str) -> i64 {
            lock(&self.db).query_row(sql, [], |r| r.get(0)).unwrap()
        }
        fn everything(&self) -> Snapshot {
            export(&lock(&self.db), &lock(&self.stories)).unwrap()
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Two devices and a real server on this computer: the whole path, encrypted both ways.
    #[test]
    fn two_devices_and_a_server() {
        let (a, b) = (device("a"), device("b"));
        let shelf_dir = a.dir.join("server-shelf");
        let identity = Identity::generate().unwrap();
        let code = Code::new(identity.fingerprint());
        let store = Arc::new(BoxStore::open(&shelf_dir).unwrap());
        let server = Server::start(&identity, seal::auth_hash(&code.auth), store.clone(), "127.0.0.1:0".parse().unwrap(), None).unwrap();
        let remote = Remote { address: server.addr.to_string(), fingerprint: code.fingerprint, auth: code.auth };
        let key = seal::data_key(&code.data.unwrap());

        // a first: everything it has goes out, nothing comes in, no backup needed
        let r = a.sync(&remote, &key).unwrap();
        assert_eq!((r.generation, r.sent.cards, r.received.any()), (1, 12, false));
        assert!(!data::auto_backup_dir(&a.dir, Kind::Cards).exists());
        // again: nothing to do, the box isn't replaced
        let r = a.sync(&remote, &key).unwrap();
        assert_eq!((r.generation, r.sent.any(), r.received.any()), (1, false, false));

        // both study and edit, then sync: each gets the other's work
        let word = a.count("SELECT id FROM cards WHERE hanzi = '你好'");
        crate::study::answer(&mut lock(&a.db), word, "recognition", 3, 1200).unwrap();
        lock(&b.db).execute("UPDATE cards SET notes = 'from b' WHERE hanzi = '水'", []).unwrap();
        stories::create_story(&lock(&b.stories), None, "月亮", "从前……").unwrap();
        a.sync(&remote, &key).unwrap();
        let r = b.sync(&remote, &key).unwrap();
        assert_eq!(r.received.reviews, 1);
        assert_eq!(r.sent.stories, 1);
        let r = a.sync(&remote, &key).unwrap();
        assert_eq!(r.received.stories, 1);
        assert_eq!(a.everything(), b.everything());
        assert_eq!(a.count("SELECT COUNT(*) FROM cards WHERE notes = 'from b'"), 1);
        assert_eq!(b.count("SELECT COUNT(*) FROM review_log"), 1);
        // a backup was made before another device's data was written in
        assert_eq!(std::fs::read_dir(data::auto_backup_dir(&b.dir, Kind::Cards)).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with("-before-sync")).count(), 1);

        // the server cannot read what it stores, and a device with another key is refused
        let (_, boxed) = store.get().unwrap();
        assert!(!boxed.windows(6).any(|w| w == "你好".as_bytes()));
        assert!(!boxed.windows(6).any(|w| w == b"hanzi\""));
        let before = b.everything();
        assert!(b.sync(&remote, &[9; 32]).unwrap_err().to_string().contains("can't be opened"));
        assert_eq!(b.everything(), before);
        drop(server);
        assert!(b.sync(&remote, &key).is_err(), "no server: an error, not a hang");
        assert_eq!(b.everything(), before);
    }

    /// With a VPN on, the address to give the other devices is still the house network's.
    #[test]
    fn the_house_network_comes_before_a_vpn() {
        let a = |name: &str, ip: [u8; 4], p2p: bool| {
            let ip = std::net::Ipv4Addr::from(ip);
            LocalAddress { address: ip.to_string(), name: name.into(), kind: kind_of(name, ip, p2p) }
        };
        let found = in_order(vec![
            a("NordLynx", [10, 5, 0, 2], false),
            a("vEthernet (WSL)", [172, 29, 64, 1], false),
            a("Wi-Fi", [192, 168, 1, 7], false),
            a("Tailscale", [100, 101, 102, 103], false),
            a("Ethernet 2", [10, 0, 0, 14], false),
            a("OpenVPN TAP-Windows6", [10, 8, 0, 6], false),
            a("ppp0", [10, 64, 0, 3], true),
        ]);
        let list: Vec<(&str, &str)> = found.iter().map(|x| (x.address.as_str(), x.kind)).collect();
        assert_eq!(
            list,
            [("192.168.1.7", "home"), ("10.0.0.14", "home"), ("172.29.64.1", "other"), ("10.5.0.2", "vpn"), ("10.64.0.3", "vpn"), ("10.8.0.6", "vpn"), ("100.101.102.103", "vpn")]
        );
        // an adapter with an everyday name is taken at its word
        assert_eq!(kind_of("en0", [192, 168, 0, 20].into(), false), "home");
        assert_eq!(kind_of("wlan0", [10, 0, 0, 5].into(), false), "home");
        assert_eq!(kind_of("utun3", [10, 7, 0, 2].into(), false), "vpn");
        // and what this computer really has can at least be listed
        for x in local_addresses() {
            println!("{:<16} {:<6} {}", x.address, x.kind, x.name);
            assert!(x.address.parse::<std::net::Ipv4Addr>().is_ok());
        }
    }

    /// The box was written by a newer version of the app: this device leaves everything alone.
    #[test]
    fn a_box_from_a_newer_version_is_refused() {
        let a = device("newer");
        let store = BoxStore::open(&a.dir.join("shelf")).unwrap();
        let key = [3u8; 32];
        let newer = Snapshot { format: hanzi_sync::snapshot::FORMAT, cards_schema: db::SCHEMA_VERSION + 1, ..a.everything() };
        store.put(0, &seal::seal(&key, &newer.pack().unwrap()).unwrap()).unwrap();
        let before = a.everything();
        let e = a.sync(&store, &key).unwrap_err().to_string();
        assert!(e.contains("newer version"), "{e}");
        assert_eq!(a.everything(), before);
        assert_eq!(store.current().unwrap().0, 1);
    }

    /// A server whose clock is far off: refused, since "latest" would mean nothing.
    #[test]
    fn clocks_that_disagree_are_refused() {
        struct Late(BoxStore);
        impl Shelf for Late {
            fn get(&self) -> Result<(u64, i64, Vec<u8>), hanzi_sync::Error> {
                let (g, b) = self.0.get()?;
                Ok((g, hanzi_sync::now() + 3600, b))
            }
            fn put(&self, expected: u64, boxed: &[u8]) -> Result<u64, hanzi_sync::Error> {
                self.0.put(expected, boxed)
            }
        }
        let a = device("clock");
        let shelf = Late(BoxStore::open(&a.dir.join("shelf")).unwrap());
        let e = a.sync(&shelf, &[1; 32]).unwrap_err().to_string();
        assert!(e.contains("clock"), "{e}");
        assert_eq!(shelf.0.current().unwrap().0, 0);
    }

    /// Another device put a box between this one's fetch and its put: it merges again.
    #[test]
    fn a_sync_that_loses_the_race_goes_round_again() {
        struct Racy<'a> {
            store: &'a BoxStore,
            other: &'a Device,
            key: [u8; 32],
            raced: AtomicBool,
        }
        impl Shelf for Racy<'_> {
            fn get(&self) -> Result<(u64, i64, Vec<u8>), hanzi_sync::Error> {
                Shelf::get(self.store)
            }
            fn put(&self, expected: u64, boxed: &[u8]) -> Result<u64, hanzi_sync::Error> {
                if !self.raced.swap(true, Ordering::Relaxed) {
                    self.other.sync(self.store, &self.key).unwrap();
                }
                self.store.put(expected, boxed)
            }
        }
        let (a, b) = (device("race-a"), device("race-b"));
        let store = BoxStore::open(&a.dir.join("shelf")).unwrap();
        let key = [5u8; 32];
        lock(&b.db).execute("UPDATE cards SET notes = 'from b' WHERE hanzi = '茶' OR hanzi = '水'", []).unwrap();
        lock(&a.db).execute("UPDATE cards SET notes = 'from a' WHERE hanzi = '我'", []).unwrap();
        let r = a.sync(&Racy { store: &store, other: &b, key, raced: AtomicBool::new(false) }, &key).unwrap();
        assert_eq!(r.generation, 2);
        assert_eq!(a.count("SELECT COUNT(*) FROM cards WHERE notes = 'from b'"), 1);
        b.sync(&store, &key).unwrap();
        assert_eq!(a.everything(), b.everything());
    }
}
