//! A server on its own: a small program for a computer that is always on.
//!
//! It keeps the latest sealed box and hands it to devices that know the pairing code. It
//! never has the key to the box, so it cannot read what it stores.
//!
//!     hanzi-sync-server [--dir <folder>] [--port <n>] [--bind <address>] [--new-code]
//!
//! The first run makes the server's certificate and prints its code, once. Give that code to
//! the first device (`join <address> <code>` in the app's console); that device then shows the
//! full code for the others. `--new-code` throws the old code away and prints a new one.

use hanzi_sync::net::{Identity, Server};
use hanzi_sync::seal::{self, Code};
use hanzi_sync::store::BoxStore;
use hanzi_sync::{hex, unhex, DEFAULT_PORT};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
struct Config {
    cert: String,
    key: String,
    auth_hash: String,
}

fn save(path: &Path, config: &Config) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(config)?)?;
    // the private key: readable by this user only
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

fn show_code(identity: &Identity, auth: [u8; 16]) {
    let code = Code { fingerprint: identity.fingerprint(), auth, data: None };
    println!("\nThis server's code (shown once: keep it somewhere safe, treat it like a password):\n\n    {}\n", code.text());
    println!("On the first device: open the console and type   join <this computer's address> <the code>");
    println!("That device then shows the full code for your other devices.\n");
}

fn main() {
    if let Err(e) = run() {
        eprintln!("hanzi-sync-server: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut dir = PathBuf::from("hanzi-sync-data");
    let mut port = DEFAULT_PORT;
    let mut bind = "0.0.0.0".to_string();
    let mut new_code = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--dir" => dir = PathBuf::from(value("--dir")?),
            "--port" => port = value("--port")?.parse().map_err(|_| "--port needs a number")?,
            "--bind" => bind = value("--bind")?,
            "--new-code" => new_code = true,
            "-h" | "--help" => {
                println!("hanzi-sync-server [--dir <folder>] [--port <n>] [--bind <address>] [--new-code]");
                return Ok(());
            }
            other => return Err(format!("unknown option {other} (try --help)").into()),
        }
    }
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("server.json");
    let config = match std::fs::read(&path) {
        Ok(bytes) => {
            let mut config: Config = serde_json::from_slice(&bytes)?;
            if new_code {
                let auth: [u8; 16] = hanzi_sync::random();
                config.auth_hash = hex(&seal::auth_hash(&auth));
                save(&path, &config)?;
                let identity = Identity { cert: unhex(&config.cert).ok_or("server.json is damaged")?, key: Vec::new() };
                show_code(&identity, auth);
                println!("Devices paired with the old code must join again with the new one.");
            }
            config
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let identity = Identity::generate()?;
            let auth: [u8; 16] = hanzi_sync::random();
            let config = Config { cert: hex(&identity.cert), key: hex(&identity.key), auth_hash: hex(&seal::auth_hash(&auth)) };
            save(&path, &config)?;
            show_code(&identity, auth);
            config
        }
        Err(e) => return Err(e.into()),
    };
    let damaged = "server.json is damaged";
    let identity = Identity { cert: unhex(&config.cert).ok_or(damaged)?, key: unhex(&config.key).ok_or(damaged)? };
    let auth_hash: [u8; 32] = unhex(&config.auth_hash).and_then(|b| b.try_into().ok()).ok_or(damaged)?;
    let store = Arc::new(BoxStore::open(&dir.join("boxes"))?);
    let (generation, size) = store.current()?;
    let on_put: hanzi_sync::net::OnPut = Arc::new(|generation| println!("box {generation} received"));
    let server = Server::start(&identity, auth_hash, store, format!("{bind}:{port}").parse()?, Some(on_put))?;
    println!("listening on {} (certificate {}…); box {generation}, {size} bytes", server.addr, hex(&identity.fingerprint()[..4]));
    println!("Ctrl+C to stop.");
    loop {
        std::thread::park();
    }
}
