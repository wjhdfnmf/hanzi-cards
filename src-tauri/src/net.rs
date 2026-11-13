//! Downloads (the dictionary, Whisper models, fonts, NVIDIA's libraries) share one HTTP agent
//! with time limits, so a connection that stalls ends in an error the page can show instead
//! of a progress bar that never moves again.

use std::sync::OnceLock;
use std::time::Duration;

/// Connect within 20 s. Once connected, each read may wait up to a minute for more data:
/// a slow download is fine, a silent one is not. (There's no limit on the whole download.)
pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .user_agent(concat!("hanzi-cards/", env!("CARGO_PKG_VERSION")))
            .build()
    })
}

pub fn get(url: &str) -> ureq::Request {
    agent().get(url)
}

/// The first answer to a HEAD request, without following a redirect: some servers (Hugging
/// Face) say what a file is (its size and checksum) there, before sending the client on to
/// wherever the bytes are kept.
pub fn head_first(url: &str) -> Result<ureq::Response, ureq::Error> {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    let agent = AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(60))
            .redirects(0)
            .user_agent(concat!("hanzi-cards/", env!("CARGO_PKG_VERSION")))
            .build()
    });
    match agent.head(url).call() {
        // (a redirect that isn't followed may come back either way)
        Err(ureq::Error::Status(code, resp)) if (300..400).contains(&code) => Ok(resp),
        other => other,
    }
}
