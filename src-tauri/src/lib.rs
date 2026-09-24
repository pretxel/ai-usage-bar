//! Token Tracker — parses local AI coding-assistant transcripts and aggregates
//! token usage + estimated cost by provider, model, day, and project.
//!
//! Sources:
//!  - Claude Code: `~/.claude/projects/**/*.jsonl`
//!  - Codex CLI:   `~/.codex/sessions/**/rollout-*.jsonl`

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use chrono::Utc;
use notify::{EventKind, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use rayon::prelude::*;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

const CLAUDE: &str = "claude";
const CODEX: &str = "codex";
const OPENCODE: &str = "opencode";

// ---------------------------------------------------------------------------
// Pricing (USD per 1M tokens): (input, cache_write, cache_read, output)
// ---------------------------------------------------------------------------
/// Family token and `(major, minor)` version parsed from a Claude model id:
/// `claude-opus-4-8` -> `("opus", 4, 8)`, `claude-opus-5` -> `("opus", 5, 0)`.
/// A trailing release date (`claude-haiku-4-5-20251001`) is not read as a
/// version component — only one- and two-digit tokens qualify.
fn claude_family(model: &str) -> Option<(&'static str, u32, u32)> {
    let m = model.to_lowercase();
    // Drop a context-window tag (`claude-opus-5-5[1m]`) so it isn't glued to
    // the last version token.
    let m = m.split('[').next().unwrap_or("");
    let tokens: Vec<&str> = m.split('-').filter(|t| !t.is_empty()).collect();
    let (family, fam_idx) = ["fable", "mythos", "opus", "sonnet", "haiku"]
        .iter()
        .find_map(|f| tokens.iter().position(|t| t == f).map(|i| (*f, i)))?;
    let version_of = |slice: &[&str]| -> Vec<u32> {
        slice
            .iter()
            .filter(|t| t.len() <= 2 && t.chars().all(|c| c.is_ascii_digit()))
            .take(2)
            .filter_map(|t| t.parse::<u32>().ok())
            .collect()
    };
    // Current ids put the version after the family (`claude-opus-4-8`); the
    // Claude 3 generation put it before (`claude-3-5-haiku-20241022`).
    let mut ver = version_of(&tokens[fam_idx + 1..]);
    if ver.is_empty() {
        ver = version_of(&tokens[..fam_idx]);
    }
    Some((
        family,
        ver.first().copied().unwrap_or(0),
        ver.get(1).copied().unwrap_or(0),
    ))
}

/// Claude pricing. `date` (YYYY-MM-DD) selects time-boxed rates, so historical
/// entries keep the rate that applied when they were logged.
fn claude_pricing(model: &str, _date: &str) -> (f64, f64, f64, f64) {
    let Some((family, major, minor)) = claude_family(model) else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    match family {
        // Mythos is priced as its Fable counterpart. Fable 5.1 cut cache reads
        // to $0.25/MTok; 5.0 and earlier stay at $1.
        "fable" | "mythos" if major > 5 || (major == 5 && minor >= 1) => (10.0, 12.5, 0.25, 50.0),
        "fable" | "mythos" => (10.0, 12.5, 1.0, 50.0),
        // Opus 4.5 dropped to a third of the Opus 4.1 / 4.0 / 3 rates; Opus 5.5
        // cut again to $4/$20 with cache reads at 0.05x ($0.20).
        "opus" if major < 4 || (major == 4 && minor < 5) => (15.0, 18.75, 1.50, 75.0),
        "opus" if major > 5 || (major == 5 && minor >= 5) => (4.0, 5.0, 0.20, 20.0),
        "opus" => (5.0, 6.25, 0.50, 25.0),
        // Sonnet 5's launch pricing ($2/$10) became permanent; the scheduled
        // 2026-09-01 increase to $3/$15 was cancelled.
        "sonnet" if major >= 5 => (2.0, 2.5, 0.20, 10.0),
        "sonnet" => (3.0, 3.75, 0.30, 15.0),
        "haiku" if major < 4 => (0.80, 1.0, 0.08, 4.0),
        _ => (1.0, 1.25, 0.10, 5.0),
    }
}

/// OpenAI / Codex pricing. Cache-write is unused (Codex reports no cache
/// creation), so it is returned as 0; cache-read carries the cached-input rate.
/// `date` (YYYY-MM-DD) selects the rate in effect when the entry was logged;
/// an empty date gets the current rate. Pro models have no cached-input
/// discount, so cached tokens bill at the full input rate.
fn openai_pricing(model: &str, date: &str) -> (f64, f64, f64, f64) {
    let m = model.to_lowercase();
    let since = |day: &str| date.is_empty() || date >= day;
    // (input, cache_write=0, cached_input, output)
    if m.contains("gpt-6-astra") {
        (10.0, 0.0, 1.0, 50.0)
    } else if m.contains("gpt-6-sol") {
        (2.0, 0.0, 0.20, 10.0)
    } else if m.contains("gpt-6-luna") {
        (0.10, 0.0, 0.01, 0.50)
    } else if m.contains("gpt-5.6-cyber") {
        (12.50, 0.0, 1.25, 75.0)
    // GPT-5.6 Luna and Terra were cut on 2026-07-30; Sol went on promotional
    // pricing on 2026-08-21. The bare `gpt-5.6` alias routes to Sol.
    } else if m.contains("gpt-5.6-luna") {
        if since("2026-07-30") { (0.20, 0.0, 0.02, 1.20) } else { (1.0, 0.0, 0.10, 6.0) }
    } else if m.contains("gpt-5.6-terra") {
        if since("2026-07-30") { (2.0, 0.0, 0.20, 12.0) } else { (2.50, 0.0, 0.25, 15.0) }
    } else if m.contains("gpt-5.6") {
        if since("2026-08-21") { (4.0, 0.0, 0.40, 20.0) } else { (5.0, 0.0, 0.50, 30.0) }
    } else if m.contains("gpt-5.5-pro") || m.contains("gpt-5.4-pro") {
        (30.0, 0.0, 30.0, 180.0)
    } else if m.contains("gpt-5.5") {
        (5.0, 0.0, 0.50, 30.0)
    } else if m.contains("gpt-5.4-nano") {
        (0.20, 0.0, 0.02, 1.25)
    } else if m.contains("gpt-5.4-mini") {
        (0.75, 0.0, 0.075, 4.50)
    } else if m.contains("gpt-5.4") {
        (2.50, 0.0, 0.25, 15.0)
    } else if m.contains("gpt-5.2-pro") {
        (21.0, 0.0, 21.0, 168.0)
    } else if m.contains("gpt-5.2-instant") {
        (1.50, 0.0, 0.15, 6.0)
    } else if m.contains("gpt-5.3") || m.contains("gpt-5.2") {
        (1.75, 0.0, 0.175, 14.0)
    } else if m.contains("gpt-5.1") {
        (1.25, 0.0, 0.125, 10.0)
    } else if m.contains("gpt-5-pro") {
        (15.0, 0.0, 15.0, 120.0)
    } else if m.contains("gpt-5-nano") || m.contains("gpt-5nano") {
        (0.05, 0.0, 0.005, 0.40)
    } else if m.contains("gpt-5-mini") || m.contains("gpt-5mini") {
        (0.25, 0.0, 0.025, 2.0)
    } else if m.contains("gpt-5") || m.contains("codex") {
        (1.25, 0.0, 0.125, 10.0)
    } else if m.contains("o4-mini") {
        (1.10, 0.0, 0.275, 4.40)
    } else if m.contains("o3-mini") {
        (1.10, 0.0, 0.55, 4.40)
    } else if m.contains("o3-pro") {
        (20.0, 0.0, 20.0, 80.0)
    } else if m.contains("o3") {
        (2.0, 0.0, 0.50, 8.0)
    } else if m.contains("o1-pro") {
        (150.0, 0.0, 150.0, 600.0)
    } else if m.starts_with("o1") || m.contains("-o1") {
        (15.0, 0.0, 7.50, 60.0)
    } else if m.contains("gpt-4.1-nano") {
        (0.10, 0.0, 0.025, 0.40)
    } else if m.contains("gpt-4.1-mini") {
        (0.40, 0.0, 0.10, 1.60)
    } else if m.contains("gpt-4.1") {
        (2.0, 0.0, 0.50, 8.0)
    } else if m.contains("gpt-4o-mini") {
        (0.15, 0.0, 0.0375, 0.60)
    } else if m.contains("gpt-4o") {
        (2.5, 0.0, 1.25, 10.0)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    }
}

/// OpenCode pricing: extracts `providerID` from the `<providerID>/<model>` format
/// and dispatches to the appropriate pricing function.
fn opencode_pricing(model: &str, date: &str) -> (f64, f64, f64, f64) {
    let slash = model.find('/');
    let provider_id = slash.map(|i| &model[..i]).unwrap_or("");
    let model_id = slash.map(|i| &model[i + 1..]).unwrap_or(model);
    match provider_id {
        "opencode" => (0.0, 0.0, 0.0, 0.0),
        "anthropic" => claude_pricing(model_id, date),
        pid if pid.contains("openai") => openai_pricing(model_id, date),
        _ => (0.0, 0.0, 0.0, 0.0),
    }
}

fn pricing(provider: &str, model: &str, date: &str) -> (f64, f64, f64, f64) {
    match provider {
        CODEX => openai_pricing(model, date),
        OPENCODE => opencode_pricing(model, date),
        _ => claude_pricing(model, date),
    }
}

fn entry_cost(e: &Entry) -> f64 {
    let (pi, pcw, pcr, po) = pricing(&e.provider, &e.model, &e.date);
    e.input as f64 / 1e6 * pi
        + e.cache_creation as f64 / 1e6 * pcw
        + e.cache_read as f64 / 1e6 * pcr
        + e.output as f64 / 1e6 * po
}

fn claude_display_name(model: &str) -> String {
    if model == "<synthetic>" {
        return "Synthetic".into();
    }
    let Some((family, major, minor)) = claude_family(model) else {
        return model.to_string();
    };
    let mut name = String::from(family);
    // Capitalize the family token: "opus" -> "Opus".
    name.replace_range(..1, &family[..1].to_uppercase());
    // "claude-opus-4-8" -> "Opus 4.8"; "claude-opus-5" -> "Opus 5".
    match (major, minor) {
        (0, _) => name,
        (maj, 0) => format!("{name} {maj}"),
        (maj, min) => format!("{name} {maj}.{min}"),
    }
}

fn codex_display_name(model: &str) -> String {
    let m = model.to_lowercase();
    let Some(rest) = m.strip_prefix("gpt-") else {
        return model.to_string();
    };
    // "5.6-sol" -> version "5.6", suffix "sol". Suffixes are tier or variant
    // names (sol/terra/luna/mini/nano/pro/codex/instant) shown as-is except
    // "codex", which is title-cased to match OpenAI's own naming.
    let (ver, suffix) = match rest.find('-') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    match suffix {
        "" => format!("GPT-{ver}"),
        "codex" => format!("GPT-{ver} Codex"),
        s => format!("GPT-{ver} {s}"),
    }
}

fn opencode_display_name(model: &str) -> String {
    let slash = model.find('/');
    let provider_id = slash.map(|i| &model[..i]).unwrap_or("");
    let model_id = slash.map(|i| &model[i + 1..]).unwrap_or(model);
    if provider_id == "opencode" {
        "OpenCode (default)".into()
    } else if provider_id == "anthropic" {
        format!("OpenCode — {}", claude_display_name(model_id))
    } else if provider_id.contains("openai") {
        format!("OpenCode — {}", codex_display_name(model_id))
    } else {
        format!("OpenCode — {model_id}")
    }
}

fn display_name(provider: &str, model: &str) -> String {
    match provider {
        CODEX => codex_display_name(model),
        OPENCODE => opencode_display_name(model),
        _ => claude_display_name(model),
    }
}

fn provider_label(provider: &str) -> String {
    match provider {
        CODEX => "Codex".into(),
        CLAUDE => "Claude Code".into(),
        OPENCODE => "OpenCode".into(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Parsed line
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct Entry {
    provider: String,
    dedup_key: String,
    model: String,
    date: String,
    timestamp: String,
    session_id: String,
    project_path: String,
    input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
}

// ---------------------------------------------------------------------------
// Serializable report
// ---------------------------------------------------------------------------
#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct Tokens {
    input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
    total: u64,
}
impl Tokens {
    fn add(&mut self, e: &Entry) {
        self.input += e.input;
        self.output += e.output;
        self.cache_creation += e.cache_creation;
        self.cache_read += e.cache_read;
        self.total = self.input + self.output + self.cache_creation + self.cache_read;
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelUsage {
    provider: String,
    model: String,
    display_name: String,
    messages: u64,
    tokens: Tokens,
    cost: f64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DayUsage {
    date: String,
    cost: f64,
    messages: u64,
    tokens: Tokens,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectUsage {
    project: String,
    path: String,
    messages: u64,
    sessions: u64,
    cost: f64,
    tokens: Tokens,
    last_used: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProviderUsage {
    provider: String,
    display_name: String,
    cost: f64,
    messages: u64,
    sessions: u64,
    tokens: Tokens,
    available: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UsageReport {
    generated_at: String,
    filter: String,
    range: String,
    data_dirs: Vec<String>,
    file_count: usize,
    total_cost: f64,
    today_cost: f64,
    avg_cost_per_day: f64,
    active_days: u64,
    total_messages: u64,
    total_sessions: u64,
    total_tokens: Tokens,
    providers: Vec<ProviderUsage>,
    models: Vec<ModelUsage>,
    by_day: Vec<DayUsage>,
    by_project: Vec<ProjectUsage>,
}

// ---------------------------------------------------------------------------
// Filesystem
// ---------------------------------------------------------------------------
fn claude_dir() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(custom).join("projects");
        if p.is_dir() {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

fn codex_dir() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("CODEX_HOME") {
        let p = PathBuf::from(custom).join("sessions");
        if p.is_dir() {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join(".codex").join("sessions"))
}

fn opencode_db_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let p = PathBuf::from(xdg).join("opencode").join("opencode.db");
        if p.is_file() {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join(".local").join("share").join("opencode").join("opencode.db"))
}

fn parse_opencode_db() -> Vec<Entry> {
    let db_path = match opencode_db_path() {
        Some(p) if p.is_file() => p,
        _ => return Vec::new(),
    };
    let conn = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let _ = conn.execute_batch("PRAGMA busy_timeout = 500");

    let mut stmt = match conn.prepare(
        "SELECT s.id, s.model, s.time_created,
                s.tokens_input, s.tokens_output, s.tokens_reasoning,
                s.tokens_cache_read, s.tokens_cache_write,
                COALESCE(p.worktree, 'unknown')
         FROM session s
         LEFT JOIN project p ON s.project_id = p.id",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let Ok(rows) = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let model_json: String = row.get(1)?;
        let time_created_ms: i64 = row.get(2)?;
        let tokens_input: i64 = row.get(3)?;
        let tokens_output: i64 = row.get(4)?;
        let tokens_reasoning: i64 = row.get(5)?;
        let tokens_cache_read: i64 = row.get(6)?;
        let tokens_cache_write: i64 = row.get(7)?;
        let worktree: String = row.get(8)?;

        let input = tokens_input.max(0) as u64;
        let output = (tokens_output.max(0) + tokens_reasoning.max(0)) as u64;
        let cache_read = tokens_cache_read.max(0) as u64;
        let cache_creation = tokens_cache_write.max(0) as u64;

        let (provider_id, model_id) = serde_json::from_str::<Value>(&model_json)
            .ok()
            .and_then(|v| {
                let pid = v.get("providerID").and_then(|x| x.as_str()).unwrap_or("");
                let mid = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                if pid.is_empty() || mid.is_empty() {
                    None
                } else {
                    Some((pid.to_string(), mid.to_string()))
                }
            })
            .unwrap_or_else(|| ("unknown".into(), "unknown".into()));

        let model = format!("{}/{}", provider_id, model_id);
        let date = if time_created_ms > 0 {
            let secs = (time_created_ms / 1000) as i64;
            let naive = chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".into());
            naive
        } else {
            "unknown".into()
        };
        let timestamp = if time_created_ms > 0 {
            let secs = (time_created_ms / 1000) as i64;
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| String::new())
        } else {
            String::new()
        };

        let dedup_key = format!("o:{id}");

        Ok(Entry {
            provider: OPENCODE.into(),
            dedup_key,
            model,
            date,
            timestamp,
            session_id: id,
            project_path: worktree,
            input,
            output,
            cache_creation,
            cache_read,
        })
    }) else {
        return Vec::new();
    };

    rows.filter_map(|r| {
        let e = r.ok()?;
        // Skip zero-usage sessions
        if e.input == 0 && e.output == 0 && e.cache_creation == 0 && e.cache_read == 0 {
            return None;
        }
        Some(e)
    }).collect()
}

fn collect_jsonl(root: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

fn u64_at(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0)
}

// ---- Claude Code transcript ----
fn parse_claude_file(path: &PathBuf) -> Vec<Entry> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(msg) = v.get("message") else { continue };
        let Some(usage) = msg.get("usage") else {
            continue;
        };
        let model = msg
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        if model.is_empty() {
            continue;
        }
        let input = u64_at(usage, "input_tokens");
        let output = u64_at(usage, "output_tokens");
        let cache_creation = u64_at(usage, "cache_creation_input_tokens");
        let cache_read = u64_at(usage, "cache_read_input_tokens");
        if input == 0 && output == 0 && cache_creation == 0 && cache_read == 0 {
            continue;
        }

        let timestamp = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let date = date_of(&timestamp);
        let session_id = v
            .get("sessionId")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let project_path = v
            .get("cwd")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown")
            .to_string();

        let msg_id = msg.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let req_id = v.get("requestId").and_then(|i| i.as_str()).unwrap_or("");
        let dedup_key = if !msg_id.is_empty() {
            format!("c:{req_id}:{msg_id}")
        } else {
            format!("c:{}#{idx}", path.display())
        };

        out.push(Entry {
            provider: CLAUDE.into(),
            dedup_key,
            model,
            date,
            timestamp,
            session_id,
            project_path,
            input,
            output,
            cache_creation,
            cache_read,
        });
    }
    out
}

// ---- Codex CLI rollout ----
fn parse_codex_file(path: &PathBuf) -> Vec<Entry> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    let mut cur_model = String::new();
    let mut session_id = String::new();
    let mut session_cwd = String::from("unknown");

    for (idx, line) in reader.lines().enumerate() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let payload = v.get("payload");

        match ty {
            "session_meta" => {
                if let Some(p) = payload {
                    if let Some(id) = p.get("id").and_then(|x| x.as_str()) {
                        session_id = id.to_string();
                    }
                    if let Some(c) = p.get("cwd").and_then(|x| x.as_str()) {
                        session_cwd = c.to_string();
                    }
                }
            }
            "turn_context" => {
                if let Some(m) = payload.and_then(|p| p.get("model")).and_then(|x| x.as_str()) {
                    if !m.is_empty() {
                        cur_model = m.to_string();
                    }
                }
                if let Some(c) = payload.and_then(|p| p.get("cwd")).and_then(|x| x.as_str()) {
                    if session_cwd == "unknown" && !c.is_empty() {
                        session_cwd = c.to_string();
                    }
                }
            }
            "event_msg" => {
                let Some(p) = payload else { continue };
                if p.get("type").and_then(|t| t.as_str()) != Some("token_count") {
                    continue;
                }
                let Some(last) = p.get("info").and_then(|i| i.get("last_token_usage")) else {
                    continue;
                };
                let input_total = u64_at(last, "input_tokens");
                let cached = u64_at(last, "cached_input_tokens");
                let output = u64_at(last, "output_tokens");
                let uncached = input_total.saturating_sub(cached);
                if input_total == 0 && output == 0 {
                    continue;
                }
                let timestamp = v
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let model = if cur_model.is_empty() {
                    "unknown".to_string()
                } else {
                    cur_model.clone()
                };
                out.push(Entry {
                    provider: CODEX.into(),
                    dedup_key: format!("x:{}#{idx}", path.display()),
                    model,
                    date: date_of(&timestamp),
                    timestamp,
                    session_id: if session_id.is_empty() {
                        path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string()
                    } else {
                        session_id.clone()
                    },
                    project_path: session_cwd.clone(),
                    input: uncached,
                    output,
                    cache_creation: 0,
                    cache_read: cached,
                });
            }
            _ => {}
        }
    }
    out
}

fn date_of(ts: &str) -> String {
    if ts.len() >= 10 {
        ts[..10].to_string()
    } else {
        "unknown".to_string()
    }
}

fn project_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------
struct ModelAgg {
    provider: String,
    model: String,
    display_name: String,
    messages: u64,
    tokens: Tokens,
    cost: f64,
}
struct DayAgg {
    cost: f64,
    messages: u64,
    tokens: Tokens,
}
struct ProjAgg {
    path: String,
    messages: u64,
    sessions: HashSet<String>,
    cost: f64,
    tokens: Tokens,
    last_used: String,
}
struct ProvAgg {
    cost: f64,
    messages: u64,
    sessions: HashSet<String>,
    tokens: Tokens,
}

/// Collect every entry from every available source, deduped.
fn opencode_available() -> bool {
    opencode_db_path().is_some_and(|p| p.is_file())
}

fn collect_entries() -> (Vec<Entry>, Vec<String>, usize) {
    let mut files: Vec<(PathBuf, bool)> = Vec::new(); // (path, is_codex)
    let mut dirs: Vec<String> = Vec::new();

    if let Some(d) = claude_dir() {
        if d.is_dir() {
            dirs.push(d.display().to_string());
            let mut fs = Vec::new();
            collect_jsonl(&d, &mut fs);
            files.extend(fs.into_iter().map(|p| (p, false)));
        }
    }
    if let Some(d) = codex_dir() {
        if d.is_dir() {
            dirs.push(d.display().to_string());
            let mut fs = Vec::new();
            collect_jsonl(&d, &mut fs);
            files.extend(fs.into_iter().map(|p| (p, true)));
        }
    }
    if opencode_available() {
        if let Some(p) = opencode_db_path() {
            dirs.push(p.display().to_string());
        }
    }

    let file_count = files.len();
    let parsed: Vec<Entry> = files
        .par_iter()
        .flat_map(|(p, is_codex)| {
            if *is_codex {
                parse_codex_file(p)
            } else {
                parse_claude_file(p)
            }
        })
        .collect();

    let opencode_entries = parse_opencode_db();

    // Dedup (Claude retries/sidechains share keys; Codex keys are line-unique;
    // OpenCode keys are o:{session-id}).
    let mut seen = HashSet::new();
    let deduped: Vec<Entry> = parsed
        .into_iter()
        .chain(opencode_entries)
        .filter(|e| seen.insert(e.dedup_key.clone()))
        .collect();

    (deduped, dirs, file_count + if opencode_available() { 1 } else { 0 })
}

fn build_report(filter: Option<String>, days: Option<u32>) -> Result<UsageReport, String> {
    let filter = filter.filter(|f| f != "all" && !f.is_empty());
    // Finite window (7/15/30); None or 0 means all-time. Anchored to the current
    // UTC date: keep entries dated on or after (today - (days - 1)).
    let days = days.filter(|d| *d > 0);
    let window_start: Option<String> = days.map(|d| {
        (Utc::now().date_naive() - chrono::Duration::days((d - 1) as i64))
            .format("%Y-%m-%d")
            .to_string()
    });
    let range = days.map(|d| d.to_string()).unwrap_or_else(|| "all".into());

    let claude = claude_dir();
    let codex = codex_dir();
    let claude_present = claude.as_ref().map(|d| d.is_dir()).unwrap_or(false);
    let codex_present = codex.as_ref().map(|d| d.is_dir()).unwrap_or(false);
    let opencode_present = opencode_available();
    if !claude_present && !codex_present && !opencode_present {
        return Err("No Claude Code, Codex, or OpenCode usage found. Use one of them, then refresh.".into());
    }

    let (entries, data_dirs, file_count) = collect_entries();

    // Provider summary is always computed over ALL entries (ignores the filter)
    // so the UI can show stable per-provider totals as filter chips.
    let mut prov: HashMap<String, ProvAgg> = HashMap::new();
    for e in &entries {
        let p = prov.entry(e.provider.clone()).or_insert_with(|| ProvAgg {
            cost: 0.0,
            messages: 0,
            sessions: HashSet::new(),
            tokens: Tokens::default(),
        });
        p.cost += entry_cost(e);
        p.messages += 1;
        p.tokens.add(e);
        if !e.session_id.is_empty() {
            p.sessions.insert(e.session_id.clone());
        }
    }
    let mut providers: Vec<ProviderUsage> = [CLAUDE, CODEX, OPENCODE]
        .iter()
        .map(|&id| {
            let present = if id == CLAUDE {
                claude_present
            } else if id == CODEX {
                codex_present
            } else {
                opencode_present
            };
            match prov.get(id) {
                Some(a) => ProviderUsage {
                    provider: id.into(),
                    display_name: provider_label(id),
                    cost: a.cost,
                    messages: a.messages,
                    sessions: a.sessions.len() as u64,
                    tokens: a.tokens.clone(),
                    available: present,
                },
                None => ProviderUsage {
                    provider: id.into(),
                    display_name: provider_label(id),
                    cost: 0.0,
                    messages: 0,
                    sessions: 0,
                    tokens: Tokens::default(),
                    available: present,
                },
            }
        })
        .collect();
    providers.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));

    let today = Utc::now().format("%Y-%m-%d").to_string();

    let mut models: HashMap<String, ModelAgg> = HashMap::new();
    let mut days: HashMap<String, DayAgg> = HashMap::new();
    let mut projects: HashMap<String, ProjAgg> = HashMap::new();
    let mut sessions: HashSet<String> = HashSet::new();

    let mut total_cost = 0.0;
    let mut today_cost = 0.0;
    let mut total_messages: u64 = 0;
    let mut total_tokens = Tokens::default();

    for e in &entries {
        if let Some(f) = &filter {
            if &e.provider != f {
                continue;
            }
        }
        if let Some(start) = &window_start {
            if e.date == "unknown" || e.date.as_str() < start.as_str() {
                continue;
            }
        }
        let cost = entry_cost(e);
        total_cost += cost;
        total_messages += 1;
        total_tokens.add(e);
        if !e.session_id.is_empty() {
            sessions.insert(e.session_id.clone());
        }
        if e.date == today {
            today_cost += cost;
        }

        let mkey = format!("{}|{}", e.provider, e.model);
        let m = models.entry(mkey).or_insert_with(|| ModelAgg {
            provider: e.provider.clone(),
            model: e.model.clone(),
            display_name: display_name(&e.provider, &e.model),
            messages: 0,
            tokens: Tokens::default(),
            cost: 0.0,
        });
        m.messages += 1;
        m.tokens.add(e);
        m.cost += cost;

        let d = days.entry(e.date.clone()).or_insert_with(|| DayAgg {
            cost: 0.0,
            messages: 0,
            tokens: Tokens::default(),
        });
        d.cost += cost;
        d.messages += 1;
        d.tokens.add(e);

        let p = projects
            .entry(e.project_path.clone())
            .or_insert_with(|| ProjAgg {
                path: e.project_path.clone(),
                messages: 0,
                sessions: HashSet::new(),
                cost: 0.0,
                tokens: Tokens::default(),
                last_used: String::new(),
            });
        p.messages += 1;
        p.cost += cost;
        p.tokens.add(e);
        if !e.session_id.is_empty() {
            p.sessions.insert(e.session_id.clone());
        }
        if e.timestamp > p.last_used {
            p.last_used = e.timestamp.clone();
        }
    }

    let mut models: Vec<ModelUsage> = models
        .into_iter()
        .map(|(_, a)| ModelUsage {
            provider: a.provider,
            model: a.model,
            display_name: a.display_name,
            messages: a.messages,
            tokens: a.tokens,
            cost: a.cost,
        })
        .collect();
    models.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));

    let mut by_day: Vec<DayUsage> = days
        .into_iter()
        .map(|(date, a)| DayUsage {
            date,
            cost: a.cost,
            messages: a.messages,
            tokens: a.tokens,
        })
        .collect();
    by_day.sort_by(|a, b| a.date.cmp(&b.date));

    // Average spend per active (dated) day — days with no usage aren't counted.
    let active_days = by_day.iter().filter(|d| d.date != "unknown").count() as u64;
    let avg_cost_per_day = if active_days > 0 {
        total_cost / active_days as f64
    } else {
        0.0
    };

    let mut by_project: Vec<ProjectUsage> = projects
        .into_iter()
        .map(|(_, a)| ProjectUsage {
            project: project_name(&a.path),
            path: a.path,
            messages: a.messages,
            sessions: a.sessions.len() as u64,
            cost: a.cost,
            tokens: a.tokens,
            last_used: a.last_used,
        })
        .collect();
    by_project.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));

    Ok(UsageReport {
        generated_at: Utc::now().to_rfc3339(),
        filter: filter.unwrap_or_else(|| "all".into()),
        range,
        data_dirs,
        file_count,
        total_cost,
        today_cost,
        avg_cost_per_day,
        active_days,
        total_messages,
        total_sessions: sessions.len() as u64,
        total_tokens,
        providers,
        models,
        by_day,
        by_project,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------
#[tauri::command]
fn get_usage(provider: Option<String>, days: Option<u32>) -> Result<UsageReport, String> {
    build_report(provider, days)
}

/// Tray title = today's estimated cost across all sources (always all-time).
fn tray_title() -> String {
    build_report(None, None)
        .map(|r| format!("${:.2}", r.today_cost))
        .unwrap_or_else(|_| "AI".to_string())
}

/// Watch the transcript directories. On any debounced change, refresh the tray
/// title and signal the frontend to reload (it re-fetches with its current
/// provider filter). The debouncer is stored in app state to keep it alive for
/// the lifetime of the app.
fn start_watching(app: &AppHandle) {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(d) = claude_dir() {
        if d.is_dir() {
            paths.push(d);
        }
    }
    if let Some(d) = codex_dir() {
        if d.is_dir() {
            paths.push(d);
        }
    }
    if let Some(db) = opencode_db_path() {
        if db.is_file() {
            paths.push(db);
        }
    }
    if paths.is_empty() {
        return;
    }

    let handle = app.clone();
    let debouncer = new_debouncer(
        Duration::from_millis(800),
        None,
        move |res: DebounceEventResult| match res {
            Ok(events) => {
                // Only react to content-changing events; ignore pure access/metadata.
                let relevant = events.iter().any(|e| {
                    matches!(
                        e.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    )
                });
                if !relevant {
                    return;
                }
                if let Some(tray) = handle.tray_by_id("main-tray") {
                    let _ = tray.set_title(Some(tray_title()));
                }
                let _ = handle.emit("usage-changed", ());
            }
            Err(errors) => eprintln!("watch error: {errors:?}"),
        },
    );

    match debouncer {
        Ok(mut d) => {
            for p in &paths {
                let mode = if p.is_dir() {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                if let Err(e) = d.watcher().watch(p, mode) {
                    eprintln!("failed to watch {}: {e}", p.display());
                }
            }
            app.manage(Mutex::new(d));
        }
        Err(e) => eprintln!("failed to start watcher: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-07-25";

    #[test]
    fn fable_pricing() {
        assert_eq!(
            claude_pricing("claude-fable-5", TODAY),
            (10.0, 12.5, 1.0, 50.0)
        );
        assert_eq!(
            claude_pricing("claude-fable-5[1m]", TODAY),
            (10.0, 12.5, 1.0, 50.0)
        );
        // 1M of each token type -> 10 + 12.5 + 1 + 50 = 73.50
        let (pi, pcw, pcr, po) = claude_pricing("claude-fable-5", TODAY);
        assert_eq!(pi + pcw + pcr + po, 73.5);
        assert_eq!(claude_display_name("claude-fable-5"), "Fable 5");
        // Fable 5.1 keeps the tier rates but reads cache at $0.25.
        assert_eq!(
            claude_pricing("claude-fable-5-1", TODAY),
            (10.0, 12.5, 0.25, 50.0)
        );
        assert_eq!(claude_display_name("claude-fable-5-1"), "Fable 5.1");
        // Unknown models still price at $0
        assert_eq!(claude_pricing("<synthetic>", TODAY), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn mythos_prices_as_fable() {
        assert_eq!(
            claude_pricing("claude-mythos-5", TODAY),
            (10.0, 12.5, 1.0, 50.0)
        );
        assert_eq!(
            claude_pricing("claude-mythos-5-1", TODAY),
            (10.0, 12.5, 0.25, 50.0)
        );
        assert_eq!(claude_display_name("claude-mythos-5-1"), "Mythos 5.1");
    }

    #[test]
    fn claude_pricing_by_version() {
        // Opus 4.5 and later are a third of the legacy Opus rates.
        assert_eq!(
            claude_pricing("claude-opus-5", TODAY),
            (5.0, 6.25, 0.50, 25.0)
        );
        assert_eq!(
            claude_pricing("claude-opus-4-8", TODAY),
            (5.0, 6.25, 0.50, 25.0)
        );
        assert_eq!(
            claude_pricing("claude-opus-4-5-20251101", TODAY),
            (5.0, 6.25, 0.50, 25.0)
        );
        assert_eq!(
            claude_pricing("claude-opus-4-1-20250805", TODAY),
            (15.0, 18.75, 1.50, 75.0)
        );
        assert_eq!(
            claude_pricing("claude-3-opus-20240229", TODAY),
            (15.0, 18.75, 1.50, 75.0)
        );

        assert_eq!(
            claude_pricing("claude-sonnet-4-6", TODAY),
            (3.0, 3.75, 0.30, 15.0)
        );
        assert_eq!(
            claude_pricing("claude-haiku-4-5-20251001", TODAY),
            (1.0, 1.25, 0.10, 5.0)
        );
        assert_eq!(
            claude_pricing("claude-3-5-haiku-20241022", TODAY),
            (0.80, 1.0, 0.08, 4.0)
        );
    }

    #[test]
    fn sonnet_5_launch_pricing_is_permanent() {
        assert_eq!(
            claude_pricing("claude-sonnet-5", "2026-08-31"),
            (2.0, 2.5, 0.20, 10.0)
        );
        assert_eq!(
            claude_pricing("claude-sonnet-5", "2026-09-24"),
            (2.0, 2.5, 0.20, 10.0)
        );
    }

    #[test]
    fn opus_5_5_pricing() {
        assert_eq!(
            claude_pricing("claude-opus-5-5", TODAY),
            (4.0, 5.0, 0.20, 20.0)
        );
        assert_eq!(
            claude_pricing("claude-opus-5-5[1m]", TODAY),
            (4.0, 5.0, 0.20, 20.0)
        );
        assert_eq!(claude_display_name("claude-opus-5-5"), "Opus 5.5");
    }

    #[test]
    fn claude_display_names() {
        assert_eq!(claude_display_name("claude-opus-4-8"), "Opus 4.8");
        assert_eq!(claude_display_name("claude-opus-5"), "Opus 5");
        assert_eq!(claude_display_name("claude-sonnet-5"), "Sonnet 5");
        assert_eq!(claude_display_name("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(claude_display_name("claude-3-opus-20240229"), "Opus 3");
        assert_eq!(claude_display_name("<synthetic>"), "Synthetic");
        assert_eq!(claude_display_name("mystery-model"), "mystery-model");
    }

    #[test]
    fn openai_pricing_by_tier() {
        const NOW: &str = "2026-09-24";
        assert_eq!(openai_pricing("gpt-6-astra", NOW), (10.0, 0.0, 1.0, 50.0));
        assert_eq!(openai_pricing("gpt-6-sol", NOW), (2.0, 0.0, 0.20, 10.0));
        assert_eq!(openai_pricing("gpt-6-luna", NOW), (0.10, 0.0, 0.01, 0.50));
        assert_eq!(openai_pricing("gpt-5.6-sol", NOW), (4.0, 0.0, 0.40, 20.0));
        assert_eq!(openai_pricing("gpt-5.6-terra", NOW), (2.0, 0.0, 0.20, 12.0));
        assert_eq!(openai_pricing("gpt-5.6-luna", NOW), (0.20, 0.0, 0.02, 1.20));
        assert_eq!(openai_pricing("gpt-5.6-cyber", NOW), (12.50, 0.0, 1.25, 75.0));
        // The bare 5.6 alias routes to Sol.
        assert_eq!(openai_pricing("gpt-5.6", NOW), (4.0, 0.0, 0.40, 20.0));
        assert_eq!(openai_pricing("gpt-5.4-mini", NOW), (0.75, 0.0, 0.075, 4.50));
        assert_eq!(openai_pricing("gpt-5.3-codex", NOW), (1.75, 0.0, 0.175, 14.0));
        // Pro models have no cached-input discount.
        assert_eq!(openai_pricing("gpt-5-pro", NOW), (15.0, 0.0, 15.0, 120.0));
        assert_eq!(openai_pricing("gpt-5.2-pro", NOW), (21.0, 0.0, 21.0, 168.0));
        // Legacy models keep their rates.
        assert_eq!(openai_pricing("gpt-5-codex", NOW), (1.25, 0.0, 0.125, 10.0));
        assert_eq!(openai_pricing("o3-mini", NOW), (1.10, 0.0, 0.55, 4.40));
        assert_eq!(openai_pricing("gpt-4o", NOW), (2.5, 0.0, 1.25, 10.0));
        assert_eq!(openai_pricing("mystery-model", NOW), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn gpt_5_6_price_cuts_apply_by_date() {
        // Luna and Terra cut on 2026-07-30.
        assert_eq!(openai_pricing("gpt-5.6-luna", "2026-07-29"), (1.0, 0.0, 0.10, 6.0));
        assert_eq!(openai_pricing("gpt-5.6-luna", "2026-07-30"), (0.20, 0.0, 0.02, 1.20));
        assert_eq!(openai_pricing("gpt-5.6-terra", "2026-07-29"), (2.50, 0.0, 0.25, 15.0));
        assert_eq!(openai_pricing("gpt-5.6-terra", "2026-07-30"), (2.0, 0.0, 0.20, 12.0));
        // Sol cut on 2026-08-21.
        assert_eq!(openai_pricing("gpt-5.6-sol", "2026-08-20"), (5.0, 0.0, 0.50, 30.0));
        assert_eq!(openai_pricing("gpt-5.6-sol", "2026-08-21"), (4.0, 0.0, 0.40, 20.0));
        // No date -> current rate.
        assert_eq!(openai_pricing("gpt-5.6-sol", ""), (4.0, 0.0, 0.40, 20.0));
    }

    #[test]
    fn codex_display_names() {
        assert_eq!(codex_display_name("gpt-5.6-sol"), "GPT-5.6 sol");
        assert_eq!(codex_display_name("gpt-5.4-mini"), "GPT-5.4 mini");
        assert_eq!(codex_display_name("gpt-5-codex"), "GPT-5 Codex");
        assert_eq!(codex_display_name("gpt-5"), "GPT-5");
        assert_eq!(codex_display_name("o3"), "o3");
    }

    #[test]
    fn report_smoke() {
        let r = build_report(None, None).expect("report should build against local data");
        println!(
            "files={} messages={} sessions={}",
            r.file_count, r.total_messages, r.total_sessions
        );
        println!("TOTAL ${:.2}  TODAY ${:.2}", r.total_cost, r.today_cost);
        for p in &r.providers {
            println!(
                "  [{}] available={} ${:.2}  {} msg  {} sessions  tok={}",
                p.provider, p.available, p.cost, p.messages, p.sessions, p.tokens.total
            );
        }
        for m in &r.models {
            println!("  {:8} {:18} {:6} msg  ${:.2}", m.provider, m.display_name, m.messages, m.cost);
        }
        assert!(r.total_messages > 0);
        assert_eq!(
            r.total_tokens.total,
            r.total_tokens.input
                + r.total_tokens.output
                + r.total_tokens.cache_creation
                + r.total_tokens.cache_read
        );
    }

    #[test]
    fn codex_only_filter() {
        let r = build_report(Some("codex".into()), None).expect("codex report");
        println!("codex-only: ${:.2}  {} msg", r.total_cost, r.total_messages);
        assert!(r.models.iter().all(|m| m.provider == "codex"));
    }

    #[test]
    fn opencode_pricing_fn() {
        // Built-in opencode model is free
        assert_eq!(
            opencode_pricing("opencode/opencode", TODAY),
            (0.0, 0.0, 0.0, 0.0)
        );
        // Anthropic model dispatches to claude_pricing
        assert_eq!(
            opencode_pricing("anthropic/claude-sonnet-4-20250514", TODAY),
            (3.0, 3.75, 0.30, 15.0)
        );
        // OpenAI model dispatches to openai_pricing
        assert_eq!(
            opencode_pricing("openai/gpt-4o", TODAY),
            (2.5, 0.0, 1.25, 10.0)
        );
        // Unknown provider is free
        assert_eq!(
            opencode_pricing("unknown/some-model", TODAY),
            (0.0, 0.0, 0.0, 0.0)
        );
        // Display names
        assert_eq!(opencode_display_name("opencode/opencode"), "OpenCode (default)");
        assert_eq!(
            opencode_display_name("anthropic/claude-sonnet-4"),
            "OpenCode — Sonnet 4"
        );
        assert_eq!(
            opencode_display_name("openai/gpt-4o"),
            "OpenCode — GPT-4o"
        );
    }

    #[test]
    fn report_has_opencode_provider() {
        let r = build_report(None, None).expect("report should build");
        let oc = r.providers.iter().find(|p| p.provider == "opencode");
        if opencode_available() {
            assert!(oc.is_some(), "opencode provider should be present when DB exists");
            if let Some(p) = oc {
                println!("opencode: available={} ${:.2} {} msg", p.available, p.cost, p.messages);
            }
        } else {
            println!("opencode DB not found — opencode provider may be absent (ok)");
        }
        // Provider label
        assert_eq!(provider_label("opencode"), "OpenCode");
    }

    #[test]
    fn opencode_only_filter() {
        let r = build_report(Some("opencode".into()), None);
        if opencode_available() {
            let r = r.expect("opencode report");
            assert!(r.models.iter().all(|m| m.provider == "opencode"));
        } else {
            // If no opencode DB, the filter would still succeed (empty result)
            assert!(r.is_ok(), "opencode filter should not error when DB is missing");
        }
    }

    #[test]
    fn day_range_filter() {
        let all = build_report(None, None).expect("all-time report");
        let win = build_report(None, Some(7)).expect("7-day report");

        let cutoff = (Utc::now().date_naive() - chrono::Duration::days(6))
            .format("%Y-%m-%d")
            .to_string();
        println!(
            "all ${:.2} ({} days) vs last-7 ${:.2} ({} days), cutoff {}",
            all.total_cost, all.active_days, win.total_cost, win.active_days, cutoff
        );

        assert_eq!(win.range, "7");
        assert_eq!(all.range, "all");
        // A trailing window is a subset of all-time.
        assert!(win.total_cost <= all.total_cost + 1e-9);
        assert!(win.total_messages <= all.total_messages);
        // No dated day in the window predates the cutoff.
        assert!(win
            .by_day
            .iter()
            .all(|d| d.date == "unknown" || d.date.as_str() >= cutoff.as_str()));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let title = tray_title();

            let show = MenuItem::with_id(app, "show", "Open Token Tracker", true, None::<&str>)?;
            let note = MenuItem::with_id(app, "note", "Today's cost shown above", false, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &note, &quit])?;

            let mut builder = TrayIconBuilder::with_id("main-tray")
                .tooltip("Token Tracker — today's estimated cost")
                .title(title)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }
            builder.build(app)?;

            start_watching(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_usage])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
