//! Kimi Code 会话扫描、消息读取和删除。
//!
//! 当前 Kimi Code 的会话布局为：
//! `sessions/<workDirKey>/<sessionId>/state.json` 和
//! `sessions/<workDirKey>/<sessionId>/agents/main/wire.jsonl`。
//! 这里把会话目录作为 source_path，避免只删除 wire 文件留下不可恢复的残骸。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::kimi_code_config::get_kimi_code_home;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, parse_timestamp_to_ms, path_basename, read_head_tail_lines, truncate_summary,
    TITLE_MAX_CHARS,
};

const PROVIDER_ID: &str = "kimi-code";

pub fn session_roots() -> Vec<PathBuf> {
    vec![get_kimi_code_home().join("sessions")]
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let Some(root) = session_roots().into_iter().next() else {
        return Vec::new();
    };

    let mut dirs = Vec::new();
    collect_session_dirs(&root, &mut dirs);
    dirs.into_iter()
        .filter_map(|dir| parse_session(&dir))
        .collect()
}

pub fn load_messages(session_dir: &Path) -> Result<Vec<SessionMessage>, String> {
    let wire_path = find_wire_path(session_dir).ok_or_else(|| {
        format!(
            "Kimi Code session wire file not found: {}",
            session_dir.display()
        )
    })?;

    let file =
        File::open(&wire_path).map_err(|e| format!("Failed to open Kimi Code wire file: {e}"))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut fallback_turns = Vec::new();
    let mut has_context_messages = false;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let ts = record_timestamp(&value);
        let record_type = value.get("type").and_then(Value::as_str).unwrap_or("");

        match record_type {
            "context.append_message" => {
                let Some(message) = value.get("message") else {
                    continue;
                };
                if let Some(parsed) = parse_context_message(message, ts) {
                    has_context_messages = true;
                    messages.push(parsed);
                }
            }
            // `turn.prompt` records describe the input to a turn but are not
            // part of the context history in current Kimi builds. They are a
            // useful fallback for older/partially written wire files.
            "turn.prompt" | "turn.steer" => {
                if let Some(message) = parse_input_message(value.get("input"), ts) {
                    fallback_turns.push(message);
                }
            }
            _ => {
                if let Some(message) = parse_legacy_message(&value, ts) {
                    messages.push(message);
                }
            }
        }
    }

    if !has_context_messages && messages.is_empty() {
        messages.extend(fallback_turns);
    }

    Ok(messages)
}

pub fn delete_session(root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    if !path.starts_with(root) {
        return Err(format!(
            "Kimi Code session source is outside the session root: {}",
            path.display()
        ));
    }
    if !path.is_dir() {
        return Err(format!(
            "Kimi Code session source is not a directory: {}",
            path.display()
        ));
    }
    if path.file_name().and_then(|name| name.to_str()) != Some(session_id) {
        return Err(format!(
            "Kimi Code session directory does not match session ID: {}",
            path.display()
        ));
    }

    if let Some(state) = read_state(path) {
        if let Some(state_id) = string_field(&state, &["id", "sessionId", "session_id"]) {
            if state_id != session_id {
                return Err(format!(
                    "Kimi Code session ID mismatch: expected {session_id}, found {state_id}"
                ));
            }
        }
    }

    std::fs::remove_dir_all(path).map_err(|e| {
        format!(
            "Failed to delete Kimi Code session directory {}: {e}",
            path.display()
        )
    })?;
    Ok(true)
}

fn parse_session(session_dir: &Path) -> Option<SessionMeta> {
    let session_id = session_dir
        .file_name()
        .and_then(|name| name.to_str())?
        .to_string();
    if session_id.is_empty() {
        return None;
    }

    let state = read_state(session_dir).unwrap_or(Value::Null);
    let wire_path = find_wire_path(session_dir);
    if state.is_null() && wire_path.is_none() {
        return None;
    }

    let project_dir = string_field(
        &state,
        &["workDir", "work_dir", "cwd", "projectDir", "project_dir"],
    )
    .or_else(|| nested_string_field(&state, &["custom", "cwd"]));

    let created_at = timestamp_field(
        &state,
        &[
            "createdAt",
            "created_at",
            "created",
            "startTime",
            "start_time",
        ],
    )
    .or_else(|| wire_path.as_deref().and_then(file_modified_ms));
    let last_active_at = timestamp_field(
        &state,
        &[
            "updatedAt",
            "updated_at",
            "lastActiveAt",
            "last_active_at",
            "lastUpdated",
            "last_updated",
        ],
    )
    .or_else(|| wire_path.as_deref().and_then(file_modified_ms))
    .or(created_at);

    let (first_user, last_message) = wire_path
        .as_deref()
        .map(read_wire_summary)
        .unwrap_or_default();

    let custom_title = string_field(&state, &["customTitle", "custom_title", "title", "name"]);
    let title = custom_title
        .or(first_user.clone())
        .or_else(|| project_dir.as_deref().and_then(path_basename))
        .or_else(|| Some(session_id.clone()))
        .map(|value| truncate_summary(&value, TITLE_MAX_CHARS));
    let summary = last_message.map(|value| truncate_summary(&value, 160));

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title,
        summary,
        project_dir,
        created_at,
        last_active_at,
        source_path: Some(session_dir.to_string_lossy().to_string()),
        resume_command: Some(format!("kimi -r {session_id}")),
    })
}

fn collect_session_dirs(root: &Path, result: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join("state.json").is_file() || find_wire_path(&path).is_some() {
            result.push(path);
        } else {
            collect_session_dirs(&path, result);
        }
    }
}

fn find_wire_path(session_dir: &Path) -> Option<PathBuf> {
    let main = session_dir.join("agents").join("main").join("wire.jsonl");
    if main.is_file() {
        return Some(main);
    }

    let root_wire = session_dir.join("wire.jsonl");
    if root_wire.is_file() {
        return Some(root_wire);
    }

    let agents = session_dir.join("agents");
    let mut wires = Vec::new();
    collect_wire_files(&agents, &mut wires);
    wires.sort();
    wires.into_iter().next()
}

fn collect_wire_files(root: &Path, result: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_wire_files(&path, result);
        } else if path.file_name().and_then(|name| name.to_str()) == Some("wire.jsonl") {
            result.push(path);
        }
    }
}

fn read_state(session_dir: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(session_dir.join("state.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_wire_summary(path: &Path) -> (Option<String>, Option<String>) {
    let Ok((head, tail)) = read_head_tail_lines(path, 24, 80) else {
        return (None, None);
    };
    let mut first_user = None;
    let mut last_message = None;

    for line in head.iter().chain(tail.iter()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(message) = parse_record_for_summary(&value) {
            if message.role == "user" && first_user.is_none() {
                first_user = Some(message.content.clone());
            }
            if !message.content.trim().is_empty() {
                last_message = Some(message.content);
            }
        }
    }

    (first_user, last_message)
}

fn parse_record_for_summary(value: &Value) -> Option<SessionMessage> {
    let ts = record_timestamp(value);
    match value.get("type").and_then(Value::as_str).unwrap_or("") {
        "context.append_message" => value
            .get("message")
            .and_then(|message| parse_context_message(message, ts)),
        "turn.prompt" | "turn.steer" => parse_input_message(value.get("input"), ts),
        _ => parse_legacy_message(value, ts),
    }
}

fn parse_context_message(value: &Value, ts: Option<i64>) -> Option<SessionMessage> {
    let role = value.get("role").and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant" | "tool") {
        return None;
    }

    let mut content = value.get("content").map(extract_text).unwrap_or_default();
    append_tool_calls(
        &mut content,
        value.get("toolCalls").or_else(|| value.get("tool_calls")),
    );
    if content.trim().is_empty() {
        return None;
    }

    Some(SessionMessage {
        role: role.to_string(),
        content,
        ts,
    })
}

fn parse_input_message(input: Option<&Value>, ts: Option<i64>) -> Option<SessionMessage> {
    let input = input?;
    let content = extract_text(input);
    if content.trim().is_empty() {
        return None;
    }
    Some(SessionMessage {
        role: "user".to_string(),
        content,
        ts,
    })
}

fn parse_legacy_message(value: &Value, ts: Option<i64>) -> Option<SessionMessage> {
    let message = value.get("message")?;
    if message.get("role").is_some() {
        return parse_context_message(message, ts);
    }

    let kind = message.get("type").and_then(Value::as_str)?;
    let payload = message.get("payload").unwrap_or(&Value::Null);

    match kind {
        "TurnBegin" => {
            let content = payload
                .get("user_input")
                .or_else(|| payload.get("userInput"))
                .map(extract_text)
                .unwrap_or_default();
            non_empty_message("user", content, ts)
        }
        "ContentPart" => {
            let content = payload
                .get("text")
                .or_else(|| payload.get("content"))
                .map(extract_text)
                .unwrap_or_else(|| extract_text(payload));
            non_empty_message("assistant", content, ts)
        }
        "ToolCall" => {
            let name = payload
                .get("name")
                .or_else(|| payload.get("tool_name"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            non_empty_message("assistant", format!("[Tool: {name}]"), ts)
        }
        "ToolResult" => {
            let content = payload
                .get("content")
                .or_else(|| payload.get("output"))
                .or_else(|| payload.get("result"))
                .map(extract_text)
                .unwrap_or_else(|| extract_text(payload));
            non_empty_message("tool", content, ts)
        }
        _ => None,
    }
}

fn non_empty_message(role: &str, content: String, ts: Option<i64>) -> Option<SessionMessage> {
    if content.trim().is_empty() {
        return None;
    }
    Some(SessionMessage {
        role: role.to_string(),
        content,
        ts,
    })
}

fn append_tool_calls(content: &mut String, tool_calls: Option<&Value>) {
    let Some(calls) = tool_calls.and_then(Value::as_array) else {
        return;
    };
    for call in calls {
        let name = call
            .get("name")
            .or_else(|| {
                call.get("function")
                    .and_then(|function| function.get("name"))
            })
            .and_then(Value::as_str);
        let Some(name) = name else {
            continue;
        };
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&format!("[Tool: {name}]"));
    }
}

fn record_timestamp(value: &Value) -> Option<i64> {
    ["timestamp", "time", "ts", "createdAt", "created_at"]
        .iter()
        .find_map(|key| value.get(*key).and_then(parse_timestamp_to_ms))
}

fn timestamp_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(parse_timestamp_to_ms))
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn nested_string_field(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn file_modified_ms(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_current_wire_format_and_legacy_records() {
        let temp = tempdir().expect("tempdir");
        let session_dir = temp.path().join("bucket").join("session-1");
        let wire = session_dir.join("agents").join("main").join("wire.jsonl");
        std::fs::create_dir_all(wire.parent().expect("wire parent")).expect("mkdir");
        std::fs::write(
            &wire,
            concat!(
                "{\"type\":\"metadata\"}\n",
                "{\"type\":\"turn.prompt\",\"input\":[{\"type\":\"text\",\"text\":\"fallback\"}]}\n",
                "{\"type\":\"context.append_message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"hello\"}]}}\n",
                "{\"type\":\"context.append_message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n"
            ),
        )
        .expect("write wire");

        let messages = load_messages(&session_dir).expect("load messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "hello");
        assert_eq!(messages[1].role, "assistant");

        std::fs::write(
            &wire,
            r#"{"timestamp":1,"message":{"type":"TurnBegin","payload":{"user_input":"old hello"}}}
{"timestamp":2,"message":{"type":"ContentPart","payload":{"type":"text","text":"old hi"}}}
{"timestamp":3,"message":{"type":"ToolCall","payload":{"name":"read"}}}
{"timestamp":4,"message":{"type":"ToolResult","payload":{"content":"done"}}}
"#,
        )
        .expect("write legacy wire");
        let messages = load_messages(&session_dir).expect("load legacy messages");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2].content, "[Tool: read]");
        assert_eq!(messages[3].role, "tool");
    }

    #[test]
    fn delete_session_requires_matching_directory_name() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("sessions");
        let session = root.join("bucket").join("session-1");
        std::fs::create_dir_all(&session).expect("mkdir");
        let error = delete_session(&root, &session, "other").expect_err("should reject");
        assert!(error.contains("does not match"));
    }
}
