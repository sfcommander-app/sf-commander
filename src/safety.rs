//! Central safety boundary for commands that can mutate game state.

use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::Write,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sf_api::{
    command::Command, error::SFError, response::Response, session::Session,
};

static KILLED: AtomicBool = AtomicBool::new(true);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static AUDIT_LOCK: Mutex<()> = Mutex::new(());
static PENDING: OnceLock<Mutex<HashMap<(String, String), String>>> =
    OnceLock::new();
const BLOCKED_PREFIX: &str = "SAFETY_BLOCKED:";

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Source {
    Manual,
    Automation,
    Unspecified,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Started,
    VerifiedSuccess,
    VerifiedFailure,
    Inconclusive,
    Blocked,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditEntry {
    at: String,
    action_id: String,
    character: String,
    action: String,
    source: Source,
    outcome: Outcome,
    detail: Option<String>,
}

#[must_use]
pub fn killed() -> bool {
    KILLED.load(Ordering::SeqCst)
}

pub fn set_killed(killed: bool) {
    KILLED.store(killed, Ordering::SeqCst);
}

fn next_id() -> String {
    format!(
        "{}-{}",
        Utc::now().timestamp_millis(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn pending() -> &'static Mutex<HashMap<(String, String), String>> {
    PENDING.get_or_init(Default::default)
}

fn remember_pending(character: &str, action: &str, action_id: &str) {
    if let Ok(mut pending) = pending().lock() {
        pending.insert(
            (character.to_string(), action.to_string()),
            action_id.to_string(),
        );
    }
}

fn take_pending(character: &str, action: &str) -> Option<String> {
    pending()
        .lock()
        .ok()?
        .remove(&(character.to_string(), action.to_string()))
}

fn redact(input: &str) -> String {
    const KEYS: [&str; 7] = [
        "authorization", "cookie", "password", "pw_hash", "secret", "session",
        "token",
    ];
    let lower = input.to_lowercase();
    if KEYS.iter().any(|key| lower.contains(key)) {
        "[REDACTED SENSITIVE ERROR]".to_string()
    } else {
        input.chars().take(500).collect()
    }
}

fn append(entry: &AuditEntry) -> Result<(), String> {
    let _guard = AUDIT_LOCK
        .lock()
        .map_err(|_| "the action audit lock is unavailable".to_string())?;
    let path = crate::app_path("action-audit.jsonl");
    let line = serde_json::to_string(entry)
        .map_err(|_| "could not serialize the action audit".to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| "could not open the action audit".to_string())?;
    writeln!(file, "{line}")
        .map_err(|_| "could not write the action audit".to_string())?;
    file.sync_data()
        .map_err(|_| "could not flush the action audit".to_string())
}

fn audit(
    action_id: &str,
    character: &str,
    action: &str,
    source: Source,
    outcome: Outcome,
    detail: Option<&str>,
) -> Result<(), String> {
    append(&AuditEntry {
        at: Utc::now().to_rfc3339(),
        action_id: action_id.to_string(),
        character: character.to_string(),
        action: action.to_string(),
        source,
        outcome,
        detail: detail.map(redact),
    })
}

fn blocked(message: impl AsRef<str>) -> SFError {
    SFError::ServerError(format!("{BLOCKED_PREFIX} {}", message.as_ref()))
}

#[must_use]
pub fn is_blocked_error(error: &SFError) -> bool {
    matches!(error, SFError::ServerError(message) if message.starts_with(BLOCKED_PREFIX))
}

/// Executes one mutating command exactly once.
///
/// A successful HTTP/game response is deliberately recorded as
/// `INCONCLUSIVE`: the feature-specific response handler must first update
/// and inspect GameState before it can call `verified_success`. Transport
/// ambiguity is never converted into success and this function never retries.
pub async fn execute_write(
    session: &mut Session,
    command: &Command,
    character: &str,
    action: &str,
    source: Source,
) -> Result<Response, SFError> {
    let action_id = next_id();
    if killed() {
        let _ = audit(
            &action_id,
            character,
            action,
            source,
            Outcome::Blocked,
            Some("master Kill Switch is active"),
        );
        return Err(blocked("master Kill Switch is active"));
    }
    if let Err(error) = audit(
        &action_id,
        character,
        action,
        source,
        Outcome::Started,
        None,
    ) {
        set_killed(true);
        return Err(blocked(error));
    }

    let result = session.send_command(command).await;
    match &result {
        Ok(_) => {
            remember_pending(character, action, &action_id);
            if audit(
                &action_id,
                character,
                action,
                source,
                Outcome::Inconclusive,
                Some(
                    "server answered; awaiting feature-specific state verification",
                ),
            )
            .is_err()
            {
                // The write may already have happened. If its outcome cannot
                // be durably recorded, continuing would make the audit trail
                // and any automatic retry unsafe.
                set_killed(true);
            }
        }
        Err(SFError::ServerError(_)) => {
            if audit(
                &action_id,
                character,
                action,
                source,
                Outcome::VerifiedFailure,
                Some("game server rejected the command"),
            )
            .is_err()
            {
                set_killed(true);
            }
        }
        Err(_) => {
            let _ = audit(
                &action_id,
                character,
                action,
                source,
                Outcome::Inconclusive,
                Some("transport failed after send; outcome unknown"),
            );
            // A connection error after sending is ambiguous: the server may
            // have applied the command even though no response arrived. Stop
            // every later write until the user explicitly releases the Kill
            // Switch; never turn an ambiguous transport into an auto-retry.
            set_killed(true);
        }
    }
    result
}

pub fn verified_success(character: &str, action: &str, source: Source) {
    let id = take_pending(character, action).unwrap_or_else(next_id);
    if audit(
        &id,
        character,
        action,
        source,
        Outcome::VerifiedSuccess,
        Some("validated by the feature response handler"),
    )
    .is_err()
    {
        set_killed(true);
    }
}

fn csv_cell(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Exports the durable action journal without including command payloads or
/// credentials. A fixed sibling file makes the result easy to find and lets
/// crash-safe storage replace an older export atomically.
pub fn export_csv() -> Result<std::path::PathBuf, String> {
    let input = crate::app_path("action-audit.jsonl");
    let output = crate::app_path("action-audit.csv");
    let text = match std::fs::read_to_string(&input) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            String::new()
        }
        Err(_) => return Err("could not read the action audit".to_string()),
    };
    let mut csv = String::from(
        "timestamp,action_id,character,action,source,outcome,detail\r\n",
    );
    for (line_no, line) in text.lines().enumerate() {
        let entry: AuditEntry = serde_json::from_str(line).map_err(|_| {
            format!("action audit line {} is invalid", line_no + 1)
        })?;
        csv.push_str(
            &[
                csv_cell(&entry.at),
                csv_cell(&entry.action_id),
                csv_cell(&entry.character),
                csv_cell(&entry.action),
                csv_cell(&format!("{:?}", entry.source)),
                csv_cell(&format!("{:?}", entry.outcome)),
                csv_cell(entry.detail.as_deref().unwrap_or("")),
            ]
            .join(","),
        );
        csv.push_str("\r\n");
    }
    crate::storage::write_atomic(&output, csv.as_bytes())
        .map_err(|_| "could not write the CSV export".to_string())?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_switch_starts_fail_closed() {
        assert!(killed());
    }

    #[test]
    fn redacts_sensitive_error_text() {
        assert_eq!(
            redact("session token leaked"),
            "[REDACTED SENSITIVE ERROR]"
        );
        assert_eq!(redact("not enough silver"), "not enough silver");
    }
}
