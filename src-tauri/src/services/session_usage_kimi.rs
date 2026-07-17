//! Kimi Code 会话日志使用追踪。
//!
//! Kimi Code 不经过 cc-switch 的本地代理，而是把每次模型请求的完整使用量
//! 写入 `~/.kimi-code/sessions/**/agents/*/wire.jsonl`。这里读取其中的
//! `usage.record` 事件，写入与其他会话同步器相同的 `proxy_request_logs` 表，
//! 因此它们可以直接出现在使用统计和请求日志页面中。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::kimi_code_config::get_kimi_code_home;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    get_sync_state, metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::sql_helpers::INPUT_TOKEN_SEMANTICS_FRESH;
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const APP_TYPE: &str = "kimi-code";
const SESSION_PROVIDER_ID: &str = "_kimi_code_session";
const DATA_SOURCE: &str = "kimi_session";
const PROVIDER_TYPE: &str = "kimi_code_session";

#[derive(Debug, Clone, Copy, Default)]
struct KimiTiming {
    first_token_ms: Option<u64>,
    stream_duration_ms: Option<u64>,
    request_build_ms: Option<u64>,
}

impl KimiTiming {
    fn latency_ms(self) -> Option<u64> {
        match (self.request_build_ms, self.stream_duration_ms) {
            (Some(build), Some(stream)) => Some(build.saturating_add(stream)),
            (Some(build), None) => Some(build),
            (None, Some(stream)) => Some(stream),
            (None, None) => None,
        }
    }
}

#[derive(Debug)]
struct KimiUsageRecord {
    model: String,
    input_other: u32,
    output: u32,
    cache_read: u32,
    cache_creation: u32,
    created_at: Option<i64>,
    session_id: String,
    timing: KimiTiming,
}

fn number_as_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| {
            value
                .as_i64()
                .filter(|number| *number >= 0)
                .map(|number| number as u64)
        })
        .or_else(|| {
            value
                .as_f64()
                .filter(|number| *number >= 0.0)
                .map(|number| number as u64)
        })
        .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
}

fn timestamp_to_seconds(value: Option<&Value>) -> Option<i64> {
    let value = value?;

    if let Some(number) = number_as_u64(Some(value)) {
        return Some(if number >= 1_000_000_000_000 {
            (number / 1000) as i64
        } else {
            number as i64
        });
    }

    let text = value.as_str()?.trim();
    if let Ok(number) = text.parse::<u64>() {
        return Some(if number >= 1_000_000_000_000 {
            (number / 1000) as i64
        } else {
            number as i64
        });
    }

    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|datetime| datetime.timestamp())
}

fn parse_step_end_timing(value: &Value) -> Option<KimiTiming> {
    if value.get("type").and_then(Value::as_str) != Some("context.append_loop_event") {
        return None;
    }

    let event = value.get("event")?;
    if event.get("type").and_then(Value::as_str) != Some("step.end") {
        return None;
    }

    Some(KimiTiming {
        first_token_ms: number_as_u64(event.get("llmFirstTokenLatencyMs")),
        stream_duration_ms: number_as_u64(event.get("llmStreamDurationMs")),
        request_build_ms: number_as_u64(event.get("llmRequestBuildMs")),
    })
}

fn parse_usage_record(
    value: &Value,
    session_id: &str,
    timing: KimiTiming,
) -> Option<KimiUsageRecord> {
    if value.get("type").and_then(Value::as_str) != Some("usage.record") {
        return None;
    }

    if value
        .get("usageScope")
        .and_then(Value::as_str)
        .is_some_and(|scope| scope != "turn")
    {
        return None;
    }

    let usage = value.get("usage")?;
    let input_other = number_as_u64(usage.get("inputOther")).unwrap_or(0) as u32;
    let output = number_as_u64(usage.get("output")).unwrap_or(0) as u32;
    let cache_read = number_as_u64(usage.get("inputCacheRead")).unwrap_or(0) as u32;
    let cache_creation = number_as_u64(usage.get("inputCacheCreation")).unwrap_or(0) as u32;

    if input_other == 0 && output == 0 && cache_read == 0 && cache_creation == 0 {
        return None;
    }

    Some(KimiUsageRecord {
        model: value
            .get("model")
            .and_then(Value::as_str)
            .filter(|model| !model.trim().is_empty())
            .unwrap_or("unknown")
            .to_string(),
        input_other,
        output,
        cache_read,
        cache_creation,
        created_at: timestamp_to_seconds(value.get("time")),
        session_id: session_id.to_string(),
        timing,
    })
}

/// Kimi Code 会话目录的固定结构为：
/// `sessions/<workDirKey>/<sessionId>/agents/<agentId>/wire.jsonl`。
fn collect_wire_files(home: &Path) -> Vec<PathBuf> {
    let sessions_dir = home.join("sessions");
    let mut files = Vec::new();

    let Ok(work_dirs) = fs::read_dir(sessions_dir) else {
        return files;
    };

    for work_entry in work_dirs.flatten() {
        let work_dir = work_entry.path();
        if !work_dir.is_dir() {
            continue;
        }

        let Ok(session_dirs) = fs::read_dir(work_dir) else {
            continue;
        };
        for session_entry in session_dirs.flatten() {
            let session_dir = session_entry.path();
            if !session_dir.is_dir() {
                continue;
            }

            let agents_dir = session_dir.join("agents");
            let Ok(agent_dirs) = fs::read_dir(agents_dir) else {
                continue;
            };
            for agent_entry in agent_dirs.flatten() {
                let wire_path = agent_entry.path().join("wire.jsonl");
                if wire_path.is_file() {
                    files.push(wire_path);
                }
            }
        }
    }

    files.sort();
    files
}

fn session_id_from_wire_path(path: &Path) -> Option<String> {
    path.parent()?
        .parent()?
        .parent()?
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

/// 同步 Kimi Code 的所有会话 wire 日志。
pub fn sync_kimi_code_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let home = get_kimi_code_home();
    let files = collect_wire_files(&home);
    let mut result = SessionSyncResult {
        imported: 0,
        skipped: 0,
        files_scanned: 0,
        errors: vec![],
    };

    for file_path in files {
        result.files_scanned += 1;
        match sync_wire_file(db, &file_path) {
            Ok((imported, skipped)) => {
                result.imported += imported;
                result.skipped += skipped;
            }
            Err(error) => {
                let message = format!("{}: {error}", file_path.display());
                log::warn!("[KIMI-SYNC] 文件解析失败: {message}");
                result.errors.push(message);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[KIMI-SYNC] 同步完成: 导入 {} 条, 跳过 {} 条, 扫描 {} 个文件",
            result.imported,
            result.skipped,
            result.files_scanned
        );
    }

    Ok(result)
}

fn sync_wire_file(db: &Database, file_path: &Path) -> Result<(u32, u32), AppError> {
    let session_id = session_id_from_wire_path(file_path).ok_or_else(|| {
        AppError::Config(format!(
            "无法从 Kimi Code 日志路径识别会话: {}",
            file_path.display()
        ))
    })?;
    let agent_id = file_path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("main")
        .to_string();

    let metadata = fs::metadata(file_path)
        .map_err(|error| AppError::Config(format!("无法读取 Kimi Code 日志元数据: {error}")))?;
    let file_modified = metadata_modified_nanos(&metadata);
    let file_key = file_path.to_string_lossy().to_string();
    let (last_modified, last_offset) = get_sync_state(db, &file_key)?;
    if file_modified <= last_modified {
        return Ok((0, 0));
    }

    let file = File::open(file_path)
        .map_err(|error| AppError::Config(format!("无法打开 Kimi Code 日志: {error}")))?;
    let reader = BufReader::new(file);
    let mut line_offset = 0i64;
    let mut pending_timing = KimiTiming::default();
    let mut imported = 0u32;
    let mut skipped = 0u32;

    for line_result in reader.lines() {
        line_offset += 1;
        if line_offset <= last_offset {
            continue;
        }

        let line = match line_result {
            Ok(line) => line,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        match value.get("type").and_then(Value::as_str) {
            Some("llm.request") => {
                pending_timing = KimiTiming::default();
            }
            Some("context.append_loop_event") => {
                if let Some(timing) = parse_step_end_timing(&value) {
                    pending_timing = timing;
                }
            }
            Some("usage.record") => {
                let Some(record) = parse_usage_record(&value, &session_id, pending_timing) else {
                    continue;
                };
                pending_timing = KimiTiming::default();

                let request_id = format!("kimi_code_session:{session_id}:{agent_id}:{line_offset}");
                match insert_kimi_usage(db, &request_id, &record) {
                    Ok(true) => imported += 1,
                    Ok(false) => skipped += 1,
                    Err(error) => {
                        log::warn!(
                            "[KIMI-SYNC] 插入失败 (session={session_id}, line={line_offset}): {error}"
                        );
                        skipped += 1;
                    }
                }
            }
            _ => {}
        }
    }

    update_sync_state(db, &file_key, file_modified, line_offset)?;
    Ok((imported, skipped))
}

fn insert_kimi_usage(
    db: &Database,
    request_id: &str,
    record: &KimiUsageRecord,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);
    let created_at = record.created_at.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    });

    let dedup_key = DedupKey {
        app_type: APP_TYPE,
        model: &record.model,
        input_tokens: record.input_other,
        output_tokens: record.output,
        cache_read_tokens: record.cache_read,
        cache_creation_tokens: record.cache_creation,
        created_at,
    };
    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    let usage = TokenUsage {
        input_tokens: record.input_other,
        output_tokens: record.output,
        cache_read_tokens: record.cache_read,
        cache_creation_tokens: record.cache_creation,
        model: Some(record.model.clone()),
        message_id: None,
    };

    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) =
        match find_model_pricing(&conn, &record.model) {
            Some(pricing) => {
                let cost = CostCalculator::calculate(&usage, &pricing, Decimal::ONE);
                (
                    cost.input_cost.to_string(),
                    cost.output_cost.to_string(),
                    cost.cache_read_cost.to_string(),
                    cost.cache_creation_cost.to_string(),
                    cost.total_cost.to_string(),
                )
            }
            None => (
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
            ),
        };

    let latency_ms = record.timing.latency_ms().unwrap_or(0) as i64;
    let first_token_ms = record.timing.first_token_ms.map(|value| value as i64);
    let duration_ms = record.timing.stream_duration_ms.map(|value| value as i64);

    let inserted_rows = conn
        .execute(
            "INSERT OR IGNORE INTO proxy_request_logs (
                request_id, provider_id, app_type, model, request_model, pricing_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_token_semantics,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
                total_cost_usd, latency_ms, first_token_ms, duration_ms, status_code,
                error_message, session_id, provider_type, is_streaming, cost_multiplier,
                created_at, data_source
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
            )",
            rusqlite::params![
                request_id,
                SESSION_PROVIDER_ID,
                APP_TYPE,
                record.model,
                record.model,
                record.model,
                record.input_other,
                record.output,
                record.cache_read,
                record.cache_creation,
                INPUT_TOKEN_SEMANTICS_FRESH,
                input_cost,
                output_cost,
                cache_read_cost,
                cache_creation_cost,
                total_cost,
                latency_ms,
                first_token_ms,
                duration_ms,
                200i64,
                Option::<String>::None,
                record.session_id,
                PROVIDER_TYPE,
                1i64,
                "1.0",
                created_at,
                DATA_SOURCE,
            ],
        )
        .map_err(|error| AppError::Database(format!("插入 Kimi Code 会话日志失败: {error}")))?;

    if inserted_rows > 0 {
        crate::usage_events::notify_log_recorded();
    }

    Ok(inserted_rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kimi_usage_record() {
        let value = serde_json::json!({
            "type": "usage.record",
            "model": "kimi-for-coding",
            "usageScope": "turn",
            "time": 1784255603141i64,
            "usage": {
                "inputOther": 10168,
                "output": 327,
                "inputCacheRead": 42,
                "inputCacheCreation": 9
            }
        });

        let record = parse_usage_record(&value, "session-1", KimiTiming::default()).unwrap();
        assert_eq!(record.model, "kimi-for-coding");
        assert_eq!(record.input_other, 10168);
        assert_eq!(record.output, 327);
        assert_eq!(record.cache_read, 42);
        assert_eq!(record.cache_creation, 9);
        assert_eq!(record.created_at, Some(1784255603));
    }

    #[test]
    fn parses_kimi_step_timing() {
        let value = serde_json::json!({
            "type": "context.append_loop_event",
            "event": {
                "type": "step.end",
                "llmFirstTokenLatencyMs": 420,
                "llmStreamDurationMs": 2400,
                "llmRequestBuildMs": 80
            },
            "time": 1784255603141i64
        });

        let timing = parse_step_end_timing(&value).unwrap();
        assert_eq!(timing.first_token_ms, Some(420));
        assert_eq!(timing.stream_duration_ms, Some(2400));
        assert_eq!(timing.request_build_ms, Some(80));
        assert_eq!(timing.latency_ms(), Some(2480));
    }

    #[test]
    fn ignores_non_turn_or_zero_usage_records() {
        let non_turn = serde_json::json!({
            "type": "usage.record",
            "usageScope": "session",
            "usage": { "output": 10 }
        });
        assert!(parse_usage_record(&non_turn, "session-1", KimiTiming::default()).is_none());

        let zero = serde_json::json!({
            "type": "usage.record",
            "usageScope": "turn",
            "usage": { "output": 0 }
        });
        assert!(parse_usage_record(&zero, "session-1", KimiTiming::default()).is_none());
    }
}
