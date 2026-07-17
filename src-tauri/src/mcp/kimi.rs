//! Kimi Code MCP 同步和导入模块。
//!
//! Kimi Code 使用 `~/.kimi-code/mcp.json`（或 `KIMI_CODE_HOME` 指定目录），
//! 格式与 Claude 的 `mcpServers` 外壳相同，但连接类型通过字段推断：
//! `command` 表示 stdio，`url` 默认表示 HTTP，旧式 SSE 需要额外的
//! `transport: "sse"`。

use serde_json::{json, Map, Value};
use std::collections::HashMap;

use crate::app_config::{McpApps, McpServer, MultiAppConfig};
use crate::error::AppError;
use crate::kimi_code_config::get_kimi_code_home;

use super::validation::validate_server_spec;

fn kimi_mcp_path() -> std::path::PathBuf {
    get_kimi_code_home().join("mcp.json")
}

fn read_mcp_servers() -> Result<HashMap<String, Value>, AppError> {
    let path = kimi_mcp_path();
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let text = std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    if text.trim().is_empty() {
        return Ok(HashMap::new());
    }

    let root: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::McpValidation(format!("解析 Kimi Code mcp.json 失败: {e}")))?;
    let Some(map) = root.get("mcpServers").and_then(Value::as_object) else {
        return Err(AppError::McpValidation(
            "Kimi Code mcp.json 缺少 mcpServers 对象".into(),
        ));
    };

    Ok(map
        .iter()
        .map(|(id, spec)| (id.clone(), spec.clone()))
        .collect())
}

fn write_mcp_servers(servers: &HashMap<String, Value>) -> Result<(), AppError> {
    let path = kimi_mcp_path();
    crate::config::write_json_file(&path, &json!({ "mcpServers": servers }))
}

/// 将 Kimi Code 格式转换为 CC Switch 统一格式。
fn convert_from_kimi_format(id: &str, spec: &Value) -> Result<Value, AppError> {
    let obj = spec.as_object().ok_or_else(|| {
        AppError::McpValidation(format!("Kimi Code MCP 服务器 '{id}' 必须是对象"))
    })?;

    let mut result = obj.clone();
    let transport = obj.get("transport").and_then(Value::as_str);

    let typ = if obj
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| !command.trim().is_empty())
    {
        "stdio"
    } else if obj
        .get("url")
        .and_then(Value::as_str)
        .is_some_and(|url| !url.trim().is_empty())
    {
        match transport {
            Some("sse") => "sse",
            Some("http") | None => "http",
            Some(other) => {
                return Err(AppError::McpValidation(format!(
                    "Kimi Code MCP 服务器 '{id}' 的 transport 不支持: {other}"
                )))
            }
        }
    } else {
        return Err(AppError::McpValidation(format!(
            "Kimi Code MCP 服务器 '{id}' 缺少 command 或 url 字段"
        )));
    };

    result.insert("type".into(), json!(typ));
    Ok(Value::Object(result))
}

/// 将 CC Switch 统一格式转换为 Kimi Code 格式。
fn convert_to_kimi_format(id: &str, spec: &Value) -> Result<Value, AppError> {
    validate_server_spec(spec)?;

    let obj = spec
        .as_object()
        .ok_or_else(|| AppError::McpValidation(format!("MCP 服务器 '{id}' 必须是 JSON 对象")))?;
    let typ = obj.get("type").and_then(Value::as_str).unwrap_or("stdio");

    let mut result: Map<String, Value> = obj.clone();
    result.remove("type");

    match typ {
        "stdio" => {
            result.remove("transport");
        }
        "http" => {
            result.remove("transport");
        }
        "sse" => {
            result.insert("transport".into(), json!("sse"));
        }
        other => {
            return Err(AppError::McpValidation(format!(
                "MCP 服务器 '{id}' 的 type 不支持: {other}"
            )))
        }
    }

    Ok(Value::Object(result))
}

/// 从 Kimi Code 的 mcp.json 导入 MCP 到统一结构。
pub fn import_from_kimi(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let map = read_mcp_servers()?;
    if map.is_empty() {
        return Ok(0);
    }

    let servers = config.mcp.servers.get_or_insert_with(HashMap::new);
    let mut changed = 0;
    let mut errors = Vec::new();

    for (id, spec) in map {
        if spec.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }

        let unified = match convert_from_kimi_format(&id, &spec) {
            Ok(value) => value,
            Err(error) => {
                log::warn!("跳过无效的 Kimi Code MCP 服务器 '{id}': {error}");
                errors.push(format!("{id}: {error}"));
                continue;
            }
        };

        if let Err(error) = validate_server_spec(&unified) {
            log::warn!("跳过无效的 Kimi Code MCP 服务器 '{id}': {error}");
            errors.push(format!("{id}: {error}"));
            continue;
        }

        if let Some(existing) = servers.get_mut(&id) {
            if !existing.apps.kimi_code {
                existing.apps.kimi_code = true;
                changed += 1;
            }
        } else {
            servers.insert(
                id.clone(),
                McpServer {
                    id: id.clone(),
                    name: id.clone(),
                    server: unified,
                    apps: McpApps {
                        kimi_code: true,
                        ..Default::default()
                    },
                    description: None,
                    homepage: None,
                    docs: None,
                    tags: Vec::new(),
                },
            );
            changed += 1;
        }
    }

    if !errors.is_empty() {
        log::warn!(
            "Kimi Code MCP 导入完成，但有 {} 项失败: {:?}",
            errors.len(),
            errors
        );
    }

    Ok(changed)
}

/// 将单个 MCP 服务器同步到 Kimi Code 的 mcp.json。
pub fn sync_single_server_to_kimi(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    let kimi_spec = convert_to_kimi_format(id, server_spec)?;
    let mut current = read_mcp_servers()?;
    current.insert(id.to_string(), kimi_spec);
    write_mcp_servers(&current)
}

/// 从 Kimi Code 的 mcp.json 中移除单个 MCP 服务器。
pub fn remove_server_from_kimi(id: &str) -> Result<(), AppError> {
    let path = kimi_mcp_path();
    if !path.exists() {
        return Ok(());
    }

    let mut current = read_mcp_servers()?;
    if current.remove(id).is_some() {
        write_mcp_servers(&current)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_kimi_mcp_shapes() {
        let stdio =
            convert_from_kimi_format("local", &json!({ "command": "npx", "args": ["server"] }))
                .expect("stdio conversion");
        assert_eq!(stdio["type"], "stdio");

        let http = convert_from_kimi_format("remote", &json!({ "url": "https://example.com/mcp" }))
            .expect("http conversion");
        assert_eq!(http["type"], "http");

        let sse = convert_from_kimi_format(
            "legacy",
            &json!({ "transport": "sse", "url": "https://example.com/sse" }),
        )
        .expect("sse conversion");
        assert_eq!(sse["type"], "sse");
    }

    #[test]
    fn removes_unified_type_when_writing_kimi_config() {
        let result = convert_to_kimi_format(
            "legacy",
            &json!({ "type": "sse", "url": "https://example.com/sse" }),
        )
        .expect("Kimi conversion");
        assert!(result.get("type").is_none());
        assert_eq!(result["transport"], "sse");
    }
}
