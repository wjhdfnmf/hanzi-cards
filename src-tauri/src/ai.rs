//! Claude API client: the API key (kept in the OS keychain) and streaming Messages
//! requests over plain HTTPS (there is no official Rust SDK).
//!
//! Streaming responses are rebuilt into full content blocks so that every block -
//! including thinking blocks with their signatures - can be sent back unchanged on the
//! next request, as the API requires.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const API: &str = "https://api.anthropic.com/v1";
const VERSION: &str = "2026-10-21";
const KEYRING_SERVICE: &str = "com.hanzicards.app";
const KEYRING_USER: &str = "anthropic-api-key";

/// Models the tutor can use: (id, label, input $/MTok, output $/MTok, cache read $/MTok).
pub const MODELS: [(&str, &str, f64, f64, f64); 2] = [
    ("claude-opus-5-5", "Claude Opus 5.5", 4.0, 20.0, 0.20),
    ("claude-haiku-5-5", "Claude Haiku 5.5", 0.10, 0.50, 0.01),
];
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

// ---------- API key ----------

fn entry() -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| AppError::msg(format!("Keychain error: {e}")))
}

/// The stored key, if any.
pub fn get_key() -> AppResult<Option<String>> {
    match entry()?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::msg(format!("Keychain error: {e}"))),
    }
}

pub fn set_key(key: &str) -> AppResult<()> {
    entry()?.set_password(key.trim()).map_err(|e| AppError::msg(format!("Keychain error: {e}")))
}

pub fn clear_key() -> AppResult<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::msg(format!("Keychain error: {e}"))),
    }
}

fn require_key() -> AppResult<String> {
    get_key()?.ok_or_else(|| AppError::msg("Add your Anthropic API key in Settings › AI tutor first."))
}

/// Check a key by listing models (free).
pub fn check_key(key: &str) -> AppResult<()> {
    match ureq::get(&format!("{API}/models?limit=1"))
        .set("x-api-key", key.trim())
        .set("anthropic-version", VERSION)
        .timeout(Duration::from_secs(20))
        .call()
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err(AppError::msg("That API key was not accepted. Check it at console.anthropic.com."))
        }
        Err(e) => Err(AppError::msg(format!("Couldn't reach the Claude API: {e}"))),
    }
}

// ---------- requests ----------

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, o: &Usage) {
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cache_read_tokens += o.cache_read_tokens;
        self.cache_write_tokens += o.cache_write_tokens;
    }

    /// Approximate cost in US dollars (cache writes at 1.25x the input price).
    pub fn cost(&self, model: &str) -> f64 {
        let (_, _, inp, out, read) = MODELS.iter().find(|m| m.0 == model).copied().unwrap_or(MODELS[0]);
        (self.input_tokens as f64 * inp
            + self.cache_write_tokens as f64 * inp * 1.25
            + self.cache_read_tokens as f64 * read
            + self.output_tokens as f64 * out)
            / 1_000_000.0
    }
}

/// A finished response: content blocks ready to echo back, plus metadata.
#[derive(Debug, Clone)]
pub struct Reply {
    pub content: Vec<Value>,
    pub stop_reason: String,
    pub usage: Usage,
    #[allow(dead_code)]
    pub model: String,
}

impl Reply {
    pub fn text(&self) -> String {
        self.content.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect()
    }

    pub fn tool_uses(&self) -> impl Iterator<Item = &Value> {
        self.content.iter().filter(|b| b["type"] == "tool_use")
    }
}

/// The request body shared by every call: model, effort, caching and (on models that
/// have it) server-side refusal fallback.
pub fn base_body(model: &str, system: &str, messages: &[Value], tools: &[Value], stream: bool) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": if stream { 64000 } else { 16000 },
        "system": [{ "type": "text", "text": system }],
        "messages": messages,
        // Caches the longest stable prefix (system prompt + conversation so far).
        "cache_control": { "type": "ephemeral" },
        // Conversation turns are short; low effort keeps replies quick.
        "output_config": { "effort": "low" },
    });
    if stream {
        body["stream"] = json!(true);
    }
    if !tools.is_empty() {
        body["tools"] = json!(tools);
    }
    if model != "claude-haiku-5-5" {
        body["fallbacks"] = json!("default");
    }
    body
}

/// Connect within 20 s; a stream may go quiet for a while (thinking) but not for 5 minutes.
fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(20)).timeout_read(Duration::from_secs(300)).build()
    })
}

fn post(body: &Value, key: &str) -> AppResult<ureq::Response> {
    let model = body["model"].as_str().unwrap_or("");
    let mut attempt = 0;
    loop {
        let mut req = agent()
            .post(&format!("{API}/messages"))
            .set("x-api-key", key)
            .set("anthropic-version", VERSION)
            .set("content-type", "application/json");
        if model != "claude-haiku-5-5" {
            req = req.set("anthropic-beta", "server-side-fallback-2026-07-01");
        }
        match req.send_string(&body.to_string()) {
            Ok(resp) => return Ok(resp),
            Err(ureq::Error::Status(code, resp)) => {
                let retry_after = resp.header("retry-after").and_then(|v| v.parse::<u64>().ok());
                let text = resp.into_string().unwrap_or_default();
                let retryable = matches!(code, 429 | 500 | 502 | 503 | 504 | 529);
                if retryable && attempt < 2 {
                    attempt += 1;
                    std::thread::sleep(Duration::from_secs(retry_after.unwrap_or(2 * attempt as u64).min(20)));
                    continue;
                }
                let message = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["error"]["message"].as_str().map(String::from))
                    .unwrap_or(text);
                return Err(AppError::msg(match code {
                    401 | 403 => "The Claude API didn't accept the API key. Check it in Settings › AI tutor.".to_string(),
                    429 => format!("The Claude API is rate-limiting this key. Try again in a moment. ({message})"),
                    529 => "Claude is overloaded right now. Try again in a moment.".to_string(),
                    _ => format!("Claude API error {code}: {message}"),
                }));
            }
            Err(e) if attempt < 2 => {
                attempt += 1;
                std::thread::sleep(Duration::from_secs(2 * attempt as u64));
                let _ = e;
            }
            Err(e) => return Err(AppError::msg(format!("Couldn't reach the Claude API: {e}"))),
        }
    }
}

/// A non-streaming request; returns the reply.
pub fn complete(body: &Value) -> AppResult<Reply> {
    let key = require_key()?;
    let resp = post(body, &key)?;
    let v: Value = serde_json::from_reader(resp.into_reader()).map_err(|e| AppError::msg(e.to_string()))?;
    let u = &v["usage"];
    Ok(Reply {
        content: v["content"].as_array().cloned().unwrap_or_default(),
        stop_reason: v["stop_reason"].as_str().unwrap_or("").to_string(),
        usage: Usage {
            input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
            cache_read_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
            cache_write_tokens: u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        },
        model: v["model"].as_str().unwrap_or("").to_string(),
    })
}

/// A streaming request. `on_text` gets each piece of visible text as it arrives.
/// Returns None if `cancel` was set.
/// Stream a request. A cancelled stream returns what arrived, with stop reason "cancelled"
/// (its output tokens estimated, since the final count never arrives).
pub fn stream(body: &Value, cancel: &AtomicBool, on_text: impl FnMut(&str)) -> AppResult<Reply> {
    let key = require_key()?;
    let resp = post(body, &key)?;
    parse_stream(resp.into_reader(), cancel, on_text)
}

/// Rebuild a response from its server-sent events.
pub fn parse_stream(reader: impl Read, cancel: &AtomicBool, mut on_text: impl FnMut(&str)) -> AppResult<Reply> {
    let mut blocks: Vec<(Value, String)> = Vec::new(); // (block, accumulated tool input JSON)
    let mut usage = Usage::default();
    let mut stop_reason = String::new();
    let mut model = String::new();
    let mut data = String::new();
    for line in BufReader::new(reader).lines() {
        if cancel.load(Ordering::Relaxed) {
            if usage.output_tokens == 0 {
                // roughly one token per Chinese character, four letters per English token
                let chars: usize = blocks
                    .iter()
                    .map(|(b, p)| {
                        let t = b["text"].as_str().unwrap_or("").to_string() + b["thinking"].as_str().unwrap_or("") + p;
                        t.chars().map(|c| if c.is_ascii() { 1 } else { 4 }).sum::<usize>() / 4
                    })
                    .sum();
                usage.output_tokens = chars as u64;
            }
            return Ok(Reply { content: vec![], stop_reason: "cancelled".into(), usage, model });
        }
        let line = line.map_err(|e| AppError::msg(format!("The connection to Claude dropped: {e}")))?;
        if let Some(d) = line.strip_prefix("data:") {
            data.push_str(d.trim_start());
            continue;
        }
        if !line.is_empty() || data.is_empty() {
            continue; // "event:" lines; the type is also in the data
        }
        let ev: Value = serde_json::from_str(&std::mem::take(&mut data)).unwrap_or(Value::Null);
        match ev["type"].as_str().unwrap_or("") {
            "message_start" => {
                let m = &ev["message"];
                model = m["model"].as_str().unwrap_or("").to_string();
                let u = &m["usage"];
                usage.input_tokens += u["input_tokens"].as_u64().unwrap_or(0);
                usage.cache_read_tokens += u["cache_read_input_tokens"].as_u64().unwrap_or(0);
                usage.cache_write_tokens += u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
            }
            "content_block_start" => {
                let i = ev["index"].as_u64().unwrap_or(0) as usize;
                if blocks.len() <= i {
                    blocks.resize(i + 1, (Value::Null, String::new()));
                }
                blocks[i] = (ev["content_block"].clone(), String::new());
            }
            "content_block_delta" => {
                let i = ev["index"].as_u64().unwrap_or(0) as usize;
                let Some((block, partial)) = blocks.get_mut(i) else { continue };
                let d = &ev["delta"];
                let append = |block: &mut Value, field: &str, piece: &str| {
                    let cur = block[field].as_str().unwrap_or("").to_string();
                    block[field] = json!(cur + piece);
                };
                match d["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        let t = d["text"].as_str().unwrap_or("");
                        append(block, "text", t);
                        on_text(t);
                    }
                    "thinking_delta" => append(block, "thinking", d["thinking"].as_str().unwrap_or("")),
                    "signature_delta" => append(block, "signature", d["signature"].as_str().unwrap_or("")),
                    "input_json_delta" => partial.push_str(d["partial_json"].as_str().unwrap_or("")),
                    _ => {}
                }
            }
            "content_block_stop" => {
                let i = ev["index"].as_u64().unwrap_or(0) as usize;
                if let Some((block, partial)) = blocks.get_mut(i) {
                    if block["type"] == "tool_use" {
                        // Tool input streams without server-side validation: parse defensively.
                        block["input"] = if partial.trim().is_empty() {
                            json!({})
                        } else {
                            serde_json::from_str::<Value>(partial).unwrap_or_else(|_| json!({ "_invalid": true }))
                        };
                    }
                }
            }
            "message_delta" => {
                if let Some(r) = ev["delta"]["stop_reason"].as_str() {
                    stop_reason = r.to_string();
                }
                if let Some(o) = ev["usage"]["output_tokens"].as_u64() {
                    usage.output_tokens = o;
                }
            }
            "error" => {
                let msg = ev["error"]["message"].as_str().unwrap_or("unknown error");
                return Err(AppError::msg(format!("Claude API error: {msg}")));
            }
            _ => {}
        }
    }
    let content = echo_content(blocks.into_iter().map(|(b, _)| b).filter(|b| !b.is_null()).collect());
    Ok(Reply { content, stop_reason, usage, model })
}

/// Content to send back as the assistant turn. After a server-side fallback, blocks of
/// the declined attempt (thinking, tool calls) before the last `fallback` marker are
/// dropped; text and everything after the marker are kept. The markers themselves stay
/// where they were: the API reads their position to check the thinking blocks around them.
pub fn echo_content(content: Vec<Value>) -> Vec<Value> {
    let Some(last) = content.iter().rposition(|b| b["type"] == "fallback") else { return content };
    content
        .into_iter()
        .enumerate()
        .filter(|(i, b)| *i >= last || matches!(b["type"].as_str(), Some("text" | "fallback")))
        .map(|(_, b)| b)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SSE: &str = "event: message_start
data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-opus-5-5\",\"usage\":{\"input_tokens\":12,\"cache_read_input_tokens\":100,\"cache_creation_input_tokens\":0}}}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig123\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"你好！\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"你叫什么名字？\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":1}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"grade_word\",\"input\":{}}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"hanzi\\\": \\\"你好\\\", \"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"correct\\\": true}\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":2}

event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":42}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

    #[test]
    fn rebuilds_a_streamed_reply() {
        let cancel = AtomicBool::new(false);
        let mut seen = String::new();
        let r = parse_stream(SSE.as_bytes(), &cancel, |t| seen.push_str(t)).unwrap();
        assert_eq!(seen, "你好！你叫什么名字？");
        assert_eq!(r.text(), "你好！你叫什么名字？");
        assert_eq!(r.content[0]["type"], "thinking");
        assert_eq!(r.content[0]["signature"], "sig123", "thinking blocks keep their signature");
        let tool = r.tool_uses().next().unwrap();
        assert_eq!(tool["input"]["hanzi"], "你好");
        assert_eq!(tool["input"]["correct"], true);
        assert_eq!(r.stop_reason, "tool_use");
        assert_eq!((r.usage.input_tokens, r.usage.cache_read_tokens, r.usage.output_tokens), (12, 100, 42));
        assert_eq!(r.model, "claude-opus-5-5");
    }

    #[test]
    fn drops_declined_attempt_blocks_after_fallback() {
        let content = vec![
            json!({"type": "thinking", "thinking": "", "signature": "a"}),
            json!({"type": "text", "text": "partial "}),
            json!({"type": "tool_use", "id": "t0", "name": "x", "input": {}}),
            json!({"type": "fallback", "from": {"model": "claude-opus-5-5"}, "to": {"model": "claude-opus-5"}}),
            json!({"type": "thinking", "thinking": "", "signature": "b"}),
            json!({"type": "text", "text": "rest"}),
        ];
        let once = echo_content(content);
        assert_eq!(echo_content(once.clone()), once, "echoing again changes nothing");
        let kept: Vec<String> = once
            .iter()
            .map(|b| format!("{}:{}", b["type"].as_str().unwrap(), b["signature"].as_str().or(b["text"].as_str()).unwrap_or("")))
            .collect();
        assert_eq!(kept, ["text:partial ", "fallback:", "thinking:b", "text:rest"]);
    }

    #[test]
    fn cancel_stops_reading() {
        let cancel = AtomicBool::new(true);
        let r = parse_stream(SSE.as_bytes(), &cancel, |_| {}).unwrap();
        assert_eq!(r.stop_reason, "cancelled");
        assert!(r.content.is_empty());
    }

    #[test]
    fn costs() {
        let u = Usage { input_tokens: 1_000_000, output_tokens: 100_000, cache_read_tokens: 0, cache_write_tokens: 0 };
        assert!((u.cost("claude-opus-5-5") - 6.0).abs() < 1e-9);
    }
}
