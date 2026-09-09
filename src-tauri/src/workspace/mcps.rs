// MCP domain: config.yaml MCP server CRUD plus format-agnostic (JSON / TOML /
// OpenCode) reads and writes of per-target MCP config files.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml::Value as TomlValue;

use super::WorkspaceConfigStore;
use super::config;
use super::targets::builtin_target_defaults_map;
use super::types::*;
use super::{display_path, parse_manager_config, resolve_target_from_id};

// ---------------------------------------------------------------------------
// MCP apply / preview / remove (config-path write targets — no workspace dep)
// ---------------------------------------------------------------------------

pub(crate) fn apply_mcp_to_target_inner(
    store: &WorkspaceConfigStore,
    server_name: &str,
    target_id: &str,
) -> Result<McpTargetMutationResult> {
    let config = store.parse()?;
    let server_name = normalize_mcp_name(server_name)?;
    let server = config
        .mcps
        .iter()
        .find(|mcp| mcp.name == server_name)
        .ok_or_else(|| anyhow!("MCP not found: {server_name}"))?;
    let target = resolve_target_from_id(&config, target_id)
        .ok_or_else(|| anyhow!("Missing target {target_id}"))?;

    if !target.enabled {
        bail!("目标 {target_id} 已禁用。");
    }

    let Some(_config_path) = target.config_path.as_ref() else {
        bail!("目标 {target_id} 没有 MCP 配置路径。");
    };

    let updated_path = apply_mcp_to_target_config(target, server)?;

    Ok(McpTargetMutationResult {
        server_name,
        target_id: AgentTargetId(target_id.to_string()),
        updated_path: Some(display_path(&updated_path)),
        action: "apply".to_string(),
        detail: format!("已写入 {}", display_path(&updated_path)),
    })
}

pub(crate) fn preview_mcp_target_inner(
    store: &WorkspaceConfigStore,
    server_name: &str,
    target_id: &str,
) -> Result<McpTargetPreviewResult> {
    let config = store.parse()?;
    let server_name = normalize_mcp_name(server_name)?;
    let server = config
        .mcps
        .iter()
        .find(|mcp| mcp.name == server_name)
        .ok_or_else(|| anyhow!("MCP not found: {server_name}"))?;
    let target = resolve_target_from_id(&config, target_id)
        .ok_or_else(|| anyhow!("Missing target {target_id}"))?;

    if !target.enabled {
        bail!("目标 {target_id} 已禁用。");
    }

    let config_path = target
        .config_path
        .as_ref()
        .ok_or_else(|| anyhow!("目标 {target_id} 没有 MCP 配置路径。"))?;

    let (format, content) = preview_mcp_entry(target, server)?;

    Ok(McpTargetPreviewResult {
        server_name,
        target_id: AgentTargetId(target_id.to_string()),
        config_path: display_path(config_path),
        format,
        content,
    })
}

pub(crate) fn remove_mcp_from_target_inner(
    store: &WorkspaceConfigStore,
    server_name: &str,
    target_id: &str,
) -> Result<McpTargetMutationResult> {
    let config = store.parse()?;
    let server_name = normalize_mcp_name(server_name)?;
    let target = resolve_target_from_id(&config, target_id)
        .ok_or_else(|| anyhow!("Missing target {target_id}"))?;

    if !target.enabled {
        bail!("目标 {target_id} 已禁用。");
    }

    let mutation = remove_mcp_from_target_config(target, &server_name)?;

    Ok(McpTargetMutationResult {
        server_name,
        target_id: AgentTargetId(target_id.to_string()),
        updated_path: mutation.updated_path,
        action: mutation.action,
        detail: mutation.detail,
    })
}

// ---------------------------------------------------------------------------
// MCP config file read/write (format-agnostic: JSON / TOML / OpenCode)
// ---------------------------------------------------------------------------

pub(super) fn read_existing_mcp_entries(
    target: &ResolvedTargetConfig,
    config_path: &Path,
) -> Result<BTreeMap<String, JsonValue>> {
    let target = resolve_read_target_for_config(target, config_path)?;
    read_existing_mcp_entries_for_target(&target, config_path)
}

fn read_existing_mcp_entries_for_target(
    target: &ResolvedTargetConfig,
    config_path: &Path,
) -> Result<BTreeMap<String, JsonValue>> {
    match detect_mcp_file_format(config_path)? {
        McpConfigFileFormat::Toml => {
            let mut root = read_toml_config(config_path)?;
            let Some(table) = get_toml_table_path_mut(&mut root, &target.mcp_config_prefix) else {
                return Ok(BTreeMap::new());
            };

            Ok(table
                .iter()
                .map(|(name, value)| {
                    let json_value = serde_json::to_value(value).unwrap_or(JsonValue::Null);
                    (name.clone(), json_value)
                })
                .collect())
        }
        McpConfigFileFormat::Json => {
            let mut root = read_json_config(config_path)?;
            let Some(table) = get_json_object_path_mut(&mut root, &target.mcp_config_prefix) else {
                return Ok(BTreeMap::new());
            };

            Ok(table
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect())
        }
    }
}

fn resolve_read_target_for_config(
    target: &ResolvedTargetConfig,
    config_path: &Path,
) -> Result<ResolvedTargetConfig> {
    if target.mcp_config_prefix.trim().is_empty() {
        if let Some(fallback) = inferred_target_layout(&target.id, config_path) {
            return Ok(fallback);
        }
    }

    Ok(target.clone())
}

fn inferred_target_layout(
    target_id: &AgentTargetId,
    config_path: &Path,
) -> Option<ResolvedTargetConfig> {
    let defaults = builtin_target_defaults_map();
    let default = defaults.get(target_id)?;

    Some(ResolvedTargetConfig {
        id: target_id.clone(),
        enabled: true,
        is_project: false,
        skill_dir: default.skill_dir.clone(),
        config_path: Some(config_path.to_path_buf()),
        mcp_config_prefix: default.config_prefix.to_string(),
        mcp_config_type: default.config_type,
    })
}

fn detect_mcp_file_format(config_path: &Path) -> Result<McpConfigFileFormat> {
    if config_path.exists() {
        let raw = fs::read_to_string(config_path)?;
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            return infer_mcp_file_format_from_path(config_path);
        }

        if json5::from_str::<JsonValue>(trimmed).is_ok() {
            return Ok(McpConfigFileFormat::Json);
        }

        if toml::from_str::<TomlValue>(trimmed).is_ok() {
            return Ok(McpConfigFileFormat::Toml);
        }

        bail!("无法识别 MCP 配置文件格式：{}", config_path.display());
    }

    infer_mcp_file_format_from_path(config_path)
}

fn infer_mcp_file_format_from_path(config_path: &Path) -> Result<McpConfigFileFormat> {
    match config_path
        .extension()
        .and_then(OsStr::to_str)
        .map(|item| item.to_ascii_lowercase())
    {
        Some(extension) if extension == "json" => Ok(McpConfigFileFormat::Json),
        Some(extension) if extension == "toml" => Ok(McpConfigFileFormat::Toml),
        _ => bail!(
            "无法从文件名推断 MCP 配置格式：{}，仅支持 json 和 toml。",
            config_path.display()
        ),
    }
}

fn desired_toml_mcp_entry(
    target: &ResolvedTargetConfig,
    server: &ResolvedMcpConfig,
) -> Result<TomlValue> {
    match target.mcp_config_type {
        McpConfigType::Common => desired_openai_toml_mcp(server),
        McpConfigType::OpenCode => bail!("OpenCode config type 不支持 TOML。"),
    }
}

fn desired_json_mcp_entry(
    target: &ResolvedTargetConfig,
    server: &ResolvedMcpConfig,
) -> Result<JsonValue> {
    match target.mcp_config_type {
        McpConfigType::Common => desired_openai_json_mcp(server),
        McpConfigType::OpenCode => desired_opencode_mcp(server),
    }
}

fn preview_mcp_entry(
    target: &ResolvedTargetConfig,
    server: &ResolvedMcpConfig,
) -> Result<(String, String)> {
    match detect_mcp_file_format(
        target
            .config_path
            .as_ref()
            .ok_or_else(|| anyhow!("目标 {} 没有 MCP 配置路径。", target.id.as_str()))?,
    )? {
        McpConfigFileFormat::Toml => Ok((
            "toml".to_string(),
            toml::to_string_pretty(&desired_toml_mcp_entry(target, server)?)
                .context("MCP 预览序列化失败。")?,
        )),
        McpConfigFileFormat::Json => Ok((
            "json".to_string(),
            serde_json::to_string_pretty(&desired_json_mcp_entry(target, server)?)
                .context("MCP 预览序列化失败。")?,
        )),
    }
}

fn default_json_root_for_target(target: &ResolvedTargetConfig) -> JsonValue {
    match target.mcp_config_type {
        McpConfigType::Common => JsonValue::Object(JsonMap::new()),
        McpConfigType::OpenCode => JsonValue::Object(JsonMap::from_iter([(
            "$schema".to_string(),
            JsonValue::String("https://opencode.ai/config.json".to_string()),
        )])),
    }
}

fn desired_openai_toml_mcp(server: &ResolvedMcpConfig) -> Result<TomlValue> {
    let mut table = toml::map::Map::new();
    table.insert("enabled".to_string(), TomlValue::Boolean(server.enabled));

    if let Some(timeout) = server.timeout {
        table.insert(
            "tool_timeout_sec".to_string(),
            TomlValue::Float((timeout as f64) / 1000.0),
        );
    }

    match server.transport {
        McpTransport::Stdio => {
            table.insert(
                "command".to_string(),
                TomlValue::String(
                    server
                        .command
                        .clone()
                        .ok_or_else(|| anyhow!("stdio MCP 缺少 command"))?,
                ),
            );

            if !server.args.is_empty() {
                table.insert(
                    "args".to_string(),
                    TomlValue::Array(server.args.iter().cloned().map(TomlValue::String).collect()),
                );
            }

            if !server.env.is_empty() {
                let env_table = server
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), TomlValue::String(value.clone())))
                    .collect();
                table.insert("env".to_string(), TomlValue::Table(env_table));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            table.insert(
                "url".to_string(),
                TomlValue::String(
                    server
                        .url
                        .clone()
                        .ok_or_else(|| anyhow!("远程 MCP 缺少 mcp 链接"))?,
                ),
            );

            if !server.headers.is_empty() {
                let headers = server
                    .headers
                    .iter()
                    .map(|(key, value)| (key.clone(), TomlValue::String(value.clone())))
                    .collect();
                table.insert("http_headers".to_string(), TomlValue::Table(headers));
            }
        }
    }

    Ok(TomlValue::Table(table))
}

fn desired_openai_json_mcp(server: &ResolvedMcpConfig) -> Result<JsonValue> {
    let mut object = JsonMap::new();

    match server.transport {
        McpTransport::Stdio => {
            object.insert("type".to_string(), JsonValue::String("stdio".to_string()));
            object.insert(
                "command".to_string(),
                JsonValue::String(
                    server
                        .command
                        .clone()
                        .ok_or_else(|| anyhow!("stdio MCP 缺少 command"))?,
                ),
            );
            object.insert(
                "args".to_string(),
                JsonValue::Array(server.args.iter().cloned().map(JsonValue::String).collect()),
            );
            object.insert(
                "env".to_string(),
                JsonValue::Object(
                    server
                        .env
                        .iter()
                        .map(|(key, value)| (key.clone(), JsonValue::String(value.clone())))
                        .collect(),
                ),
            );
        }
        McpTransport::Http | McpTransport::Sse => {
            object.insert(
                "type".to_string(),
                JsonValue::String(
                    match server.transport {
                        McpTransport::Http => "http",
                        McpTransport::Sse => "sse",
                        McpTransport::Stdio => unreachable!(),
                    }
                    .to_string(),
                ),
            );
            object.insert(
                "url".to_string(),
                JsonValue::String(
                    server
                        .url
                        .clone()
                        .ok_or_else(|| anyhow!("远程 MCP 缺少 mcp 链接"))?,
                ),
            );

            if !server.headers.is_empty() {
                object.insert(
                    "headers".to_string(),
                    JsonValue::Object(
                        server
                            .headers
                            .iter()
                            .map(|(key, value)| (key.clone(), JsonValue::String(value.clone())))
                            .collect(),
                    ),
                );
            }
        }
    }

    Ok(JsonValue::Object(object))
}

pub(super) fn desired_opencode_mcp(server: &ResolvedMcpConfig) -> Result<JsonValue> {
    let mut object = JsonMap::new();
    object.insert("enabled".to_string(), JsonValue::Bool(server.enabled));
    if let Some(timeout) = server.timeout {
        object.insert("timeout".to_string(), JsonValue::Number(timeout.into()));
    }

    match server.transport {
        McpTransport::Stdio => {
            object.insert("type".to_string(), JsonValue::String("local".to_string()));
            let command = server
                .command
                .clone()
                .ok_or_else(|| anyhow!("stdio MCP 缺少 command"))?;
            let mut command_parts = vec![JsonValue::String(command)];
            command_parts.extend(server.args.iter().cloned().map(JsonValue::String));
            object.insert("command".to_string(), JsonValue::Array(command_parts));

            if !server.env.is_empty() {
                object.insert(
                    "environment".to_string(),
                    JsonValue::Object(
                        server
                            .env
                            .iter()
                            .map(|(key, value)| (key.clone(), JsonValue::String(value.clone())))
                            .collect(),
                    ),
                );
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            object.insert("type".to_string(), JsonValue::String("remote".to_string()));
            object.insert(
                "url".to_string(),
                JsonValue::String(
                    server
                        .url
                        .clone()
                        .ok_or_else(|| anyhow!("远程 MCP 缺少 mcp 链接"))?,
                ),
            );

            if !server.headers.is_empty() {
                object.insert(
                    "headers".to_string(),
                    JsonValue::Object(
                        server
                            .headers
                            .iter()
                            .map(|(key, value)| (key.clone(), JsonValue::String(value.clone())))
                            .collect(),
                    ),
                );
            }
        }
    }

    Ok(JsonValue::Object(object))
}

fn apply_mcp_to_target_config(
    target: &ResolvedTargetConfig,
    server: &ResolvedMcpConfig,
) -> Result<PathBuf> {
    let config_path = target
        .config_path
        .as_ref()
        .ok_or_else(|| anyhow!("目标 {} 没有 MCP 配置路径。", target.id.as_str()))?;

    match detect_mcp_file_format(config_path)? {
        McpConfigFileFormat::Toml => {
            let mut root = read_toml_config(config_path)?;
            let server_map = ensure_toml_table_path(&mut root, &target.mcp_config_prefix)?;
            server_map.insert(server.name.clone(), desired_toml_mcp_entry(target, server)?);
            write_toml_config(config_path, &root)?;
        }
        McpConfigFileFormat::Json => {
            let mut root = read_json_config(config_path)?;

            if root.is_null() {
                root = default_json_root_for_target(target);
            }

            let server_map = ensure_json_object_path(&mut root, &target.mcp_config_prefix)?;
            server_map.insert(server.name.clone(), desired_json_mcp_entry(target, server)?);
            write_json_config(config_path, &root)?;
        }
    }

    Ok(config_path.clone())
}

fn remove_mcp_from_target_config(
    target: &ResolvedTargetConfig,
    server_name: &str,
) -> Result<McpTargetMutationResult> {
    let config_path = target
        .config_path
        .as_ref()
        .ok_or_else(|| anyhow!("目标 {} 没有 MCP 配置路径。", target.id.as_str()))?;

    let removed = match detect_mcp_file_format(config_path)? {
        McpConfigFileFormat::Toml => {
            let mut root = read_toml_config(config_path)?;
            match get_toml_table_path_mut(&mut root, &target.mcp_config_prefix) {
                Some(table) => {
                    let removed = table.remove(server_name).is_some();
                    if removed {
                        write_toml_config(config_path, &root)?;
                    }
                    removed
                }
                None => false,
            }
        }
        McpConfigFileFormat::Json => {
            let mut root = read_json_config(config_path)?;
            match get_json_object_path_mut(&mut root, &target.mcp_config_prefix) {
                Some(table) => {
                    let removed = table.remove(server_name).is_some();
                    if removed {
                        write_json_config(config_path, &root)?;
                    }
                    removed
                }
                None => false,
            }
        }
    };

    if !removed {
        return Ok(McpTargetMutationResult {
            server_name: server_name.to_string(),
            target_id: target.id.clone(),
            updated_path: None,
            action: "noop".to_string(),
            detail: "目标配置里不存在该 MCP。".to_string(),
        });
    }

    Ok(McpTargetMutationResult {
        server_name: server_name.to_string(),
        target_id: target.id.clone(),
        updated_path: Some(display_path(config_path)),
        action: "remove".to_string(),
        detail: format!("已从 {} 移除 {}", target.id.as_str(), server_name),
    })
}

fn read_toml_config(config_path: &Path) -> Result<toml::map::Map<String, TomlValue>> {
    if !config_path.exists() {
        return Ok(Default::default());
    }

    let raw = fs::read_to_string(config_path)?;
    if raw.trim().is_empty() {
        return Ok(Default::default());
    }

    if let Ok(parsed) = raw.parse::<TomlValue>() {
        return parsed
            .as_table()
            .cloned()
            .ok_or_else(|| anyhow!("TOML 配置根节点必须是 table。"));
    }

    toml::Table::from_str(&raw)
        .with_context(|| format!("TOML 配置解析失败：{}", config_path.display()))
}

fn write_toml_config(config_path: &Path, root: &toml::map::Map<String, TomlValue>) -> Result<()> {
    let serialized = toml::to_string_pretty(root)
        .with_context(|| format!("TOML 配置序列化失败：{}", config_path.display()))?;

    config::write_atomic(config_path, &serialized)
        .with_context(|| format!("写入 TOML 配置失败：{}", config_path.display()))
}

fn read_json_config(config_path: &Path) -> Result<JsonValue> {
    if !config_path.exists() {
        return Ok(JsonValue::Object(JsonMap::new()));
    }

    let raw = fs::read_to_string(config_path)?;
    if raw.trim().is_empty() {
        return Ok(JsonValue::Object(JsonMap::new()));
    }

    let parsed = json5::from_str::<JsonValue>(&raw)
        .with_context(|| format!("JSON 配置解析失败：{}", config_path.display()))?;

    if parsed.is_object() {
        return Ok(parsed);
    }

    bail!("JSON 配置根节点必须是 object：{}", config_path.display())
}

fn write_json_config(config_path: &Path, root: &JsonValue) -> Result<()> {
    let serialized = serde_json::to_string_pretty(root)
        .with_context(|| format!("JSON 配置序列化失败：{}", config_path.display()))?;

    config::write_atomic(config_path, &format!("{serialized}\n"))
        .with_context(|| format!("写入 JSON 配置失败：{}", config_path.display()))
}

fn ensure_json_object_path<'a>(
    root: &'a mut JsonValue,
    path: &str,
) -> Result<&'a mut JsonMap<String, JsonValue>> {
    let keys = split_config_path(path)?;
    let mut current = root
        .as_object_mut()
        .ok_or_else(|| anyhow!("JSON 根节点必须是 object。"))?;

    for key in keys {
        let entry = current
            .entry(key.to_string())
            .or_insert_with(|| JsonValue::Object(JsonMap::new()));
        current = entry
            .as_object_mut()
            .ok_or_else(|| anyhow!("{path} 必须是 object。"))?;
    }

    Ok(current)
}

fn get_json_object_path_mut<'a>(
    root: &'a mut JsonValue,
    path: &str,
) -> Option<&'a mut JsonMap<String, JsonValue>> {
    let keys = split_config_path(path).ok()?;
    let mut current = root.as_object_mut()?;

    for key in keys {
        current = current.get_mut(key)?.as_object_mut()?;
    }

    Some(current)
}

fn ensure_toml_table_path<'a>(
    root: &'a mut toml::map::Map<String, TomlValue>,
    path: &str,
) -> Result<&'a mut toml::map::Map<String, TomlValue>> {
    let keys = split_config_path(path)?;
    let mut current = root;

    for key in keys {
        let entry = current
            .entry(key.to_string())
            .or_insert_with(|| TomlValue::Table(Default::default()));
        current = entry
            .as_table_mut()
            .ok_or_else(|| anyhow!("{path} 对应的 TOML 节点必须是 table。"))?;
    }

    Ok(current)
}

fn get_toml_table_path_mut<'a>(
    root: &'a mut toml::map::Map<String, TomlValue>,
    path: &str,
) -> Option<&'a mut toml::map::Map<String, TomlValue>> {
    let keys = split_config_path(path).ok()?;
    let mut current = root;

    for key in keys {
        current = current.get_mut(key)?.as_table_mut()?;
    }

    Some(current)
}

fn split_config_path(path: &str) -> Result<Vec<&str>> {
    let keys = path
        .split('.')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();

    if keys.is_empty() {
        bail!("configPrefix 不能为空。");
    }

    Ok(keys)
}

pub(crate) fn create_workspace_mcp_inner(
    store: &WorkspaceConfigStore,
    raw_mcp: RawMcpConfig,
) -> Result<WorkspaceMcpMutationResult> {
    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;
        let server_name = normalize_mcp_name(&raw_mcp.name)?;
        let next_mcp = normalize_raw_mcp(raw_mcp, server_name.clone())?;

        if raw_config.mcps.iter().any(|mcp| mcp.name == server_name) {
            bail!("mcp name 已存在：{}", server_name);
        }

        raw_config.mcps.push(next_mcp);

        config.write_raw(&raw_config)?;

        Ok(WorkspaceMcpMutationResult {
            server_name,
            updated_paths: vec![display_path(config.config_path())],
        })
    })
}

pub(crate) fn update_workspace_mcp_inner(
    store: &WorkspaceConfigStore,
    server_name: &str,
    raw_mcp: RawMcpConfig,
) -> Result<WorkspaceMcpMutationResult> {
    let server_name = normalize_mcp_name(server_name)?;

    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;
        let existing_index = raw_config
            .mcps
            .iter()
            .position(|mcp| mcp.name == server_name)
            .ok_or_else(|| anyhow!("MCP not found: {server_name}"))?;
        let created_at = raw_config.mcps[existing_index].created_at;
        let next_server_name = normalize_mcp_name(&raw_mcp.name)?;

        if next_server_name != server_name
            && raw_config
                .mcps
                .iter()
                .any(|mcp| mcp.name == next_server_name)
        {
            bail!("mcp name 已存在：{}", next_server_name);
        }

        let next_mcp = normalize_raw_mcp(
            RawMcpConfig {
                created_at,
                ..raw_mcp
            },
            next_server_name.clone(),
        )?;

        raw_config.mcps[existing_index] = next_mcp;

        config.write_raw(&raw_config)?;

        Ok(WorkspaceMcpMutationResult {
            server_name: next_server_name,
            updated_paths: vec![display_path(config.config_path())],
        })
    })
}

pub(crate) fn delete_workspace_mcp_inner(
    store: &WorkspaceConfigStore,
    server_name: &str,
) -> Result<WorkspaceMcpMutationResult> {
    // The whole sequence stays under the lock: the manager config mutation and
    // the per-target config cleanups are derived from one config snapshot.
    store.locked(|config| {
        let raw_content = config.read_raw()?;
        let mut raw_config: RawManagerConfig = serde_yaml::from_str(&raw_content)?;
        let resolved_config = parse_manager_config(&raw_content, config.config_path())?;
        let server_name = normalize_mcp_name(server_name)?;
        let mut updated_paths = vec![display_path(config.config_path())];

        raw_config
            .mcps
            .iter()
            .find(|mcp| mcp.name == server_name)
            .ok_or_else(|| anyhow!("MCP not found: {server_name}"))?;
        let resolved = resolved_config
            .mcps
            .iter()
            .find(|mcp| mcp.name == server_name)
            .ok_or_else(|| anyhow!("MCP not found: {server_name}"))?;

        raw_config.mcps.retain(|mcp| mcp.name != server_name);
        config.write_raw(&raw_config)?;

        for target in resolved_config.targets.values() {
            let mutation = remove_mcp_from_target_config(target, &resolved.name)?;

            if let Some(path) = mutation.updated_path {
                updated_paths.push(path);
            }
        }

        for project in resolved_config.projects.values() {
            for target in project.agents.values() {
                let mutation = remove_mcp_from_target_config(target, &resolved.name)?;

                if let Some(path) = mutation.updated_path {
                    updated_paths.push(path);
                }
            }
        }

        updated_paths.sort();
        updated_paths.dedup();

        Ok(WorkspaceMcpMutationResult {
            server_name,
            updated_paths,
        })
    })
}

fn normalize_mcp_name(value: &str) -> Result<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        bail!("mcp name 不能为空。");
    }

    Ok(trimmed.to_string())
}

fn normalize_raw_mcp(raw_mcp: RawMcpConfig, name: String) -> Result<RawMcpConfig> {
    let next = RawMcpConfig {
        name,
        enabled: raw_mcp.enabled,
        transport: raw_mcp.transport,
        created_at: raw_mcp.created_at,
        homepage: raw_mcp
            .homepage
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        command: raw_mcp
            .command
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        args: raw_mcp
            .args
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect(),
        env: raw_mcp
            .env
            .into_iter()
            .filter(|(key, _)| !key.trim().is_empty())
            .map(|(key, value)| (key.trim().to_string(), value))
            .collect(),
        url: raw_mcp
            .url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        headers: raw_mcp
            .headers
            .into_iter()
            .filter(|(key, _)| !key.trim().is_empty())
            .map(|(key, value)| (key.trim().to_string(), value))
            .collect(),
        timeout: raw_mcp.timeout,
    };

    match next.transport {
        McpTransport::Stdio => {
            if next.command.as_deref().unwrap_or("").trim().is_empty() {
                bail!("stdio MCP 必须提供 command。");
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            if next.url.as_deref().unwrap_or("").trim().is_empty() {
                bail!("远程 MCP 必须提供 mcp 链接。");
            }
        }
    }

    Ok(next)
}
