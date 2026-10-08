// MCP 配置「格式 writer」：每种 McpConfigType 一个 writer，收敛该类型在
// 目标工具配置里的条目形态、支持的文件格式与前缀约束；跨格式的文件解析、
// 前缀定位与写入编排在 mcps.rs 的通用底座上完成。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use dirs::home_dir;
use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use serde_yaml::Value as YamlValue;
use toml::Value as TomlValue;

use super::display_path;
use super::types::{
    McpConfigFileFormat, McpConfigType, McpTargetMutationResult, McpTransport, ResolvedMcpConfig,
    ResolvedTargetConfig,
};

// 每个 writer 只声明「这个 config type 的条目长什么样、接受什么文件、
// 需要哪些前置条件」；JSON/TOML 的读/预览/应用/移除由 mcps.rs 的共享引擎
// 完成并把钩子回调到 writer，dsh patch 则整体覆写。
pub(super) trait McpFormatWriter: Sync {
    /// mcp.config_prefix 是否必填（dsh 按 name+serverName 定位，无 prefix 概念）。
    fn prefix_required(&self) -> bool;

    /// 新建空 JSON 配置的默认根（OpenCode 注入 $schema）。
    fn default_json_root(&self) -> JsonValue;

    /// 文件格式强校验（GrokBuild 只接受 TOML）；默认放行。
    fn ensure_format_supported(&self, _format: McpConfigFileFormat) -> Result<()> {
        Ok(())
    }

    /// 构造 desired 条目的 TOML 形态；不支持的类型按既定文案报错。
    fn desired_toml_entry(&self, server: &ResolvedMcpConfig) -> Result<TomlValue>;

    /// 构造 desired 条目的 JSON 形态；不支持的类型按既定文案报错。
    fn desired_json_entry(&self, server: &ResolvedMcpConfig) -> Result<JsonValue>;

    /// 读取侧的条目 key：默认与 server 名一致；dsh 按清洗后的 serverName。
    fn entry_key_for(&self, name: &str) -> String {
        name.to_string()
    }

    /// 读既有条目（key 为 server 名；dsh 按清洗后的 serverName）。
    fn read_existing_entries(
        &self,
        target: &ResolvedTargetConfig,
        config_path: &Path,
    ) -> Result<BTreeMap<String, JsonValue>> {
        super::mcps::read_file_entries(self, target, config_path)
    }

    /// 预览条目内容（格式标签 + 序列化文本），不落盘。
    fn preview_entry(
        &self,
        config_path: &Path,
        server: &ResolvedMcpConfig,
    ) -> Result<(String, String)> {
        super::mcps::preview_file_entry(self, config_path, server)
    }

    /// 应用条目到目标配置；幂等（重复应用内容不变）。
    fn apply_entry(
        &self,
        target: &ResolvedTargetConfig,
        config_path: &Path,
        server: &ResolvedMcpConfig,
    ) -> Result<()> {
        super::mcps::apply_file_entry(self, target, config_path, server)
    }

    /// 移除条目；条目不存在时返回 action 为 noop 的结果而非报错。
    fn remove_entry(
        &self,
        target: &ResolvedTargetConfig,
        config_path: &Path,
        server_name: &str,
    ) -> Result<McpTargetMutationResult> {
        super::mcps::remove_file_entry(self, target, config_path, server_name)
    }
}

struct CommonWriter;
struct GrokBuildWriter;
struct OpenCodeWriter;
struct DshPatchWriter;

impl McpFormatWriter for CommonWriter {
    fn prefix_required(&self) -> bool {
        true
    }

    fn default_json_root(&self) -> JsonValue {
        JsonValue::Object(JsonMap::new())
    }

    fn desired_toml_entry(&self, server: &ResolvedMcpConfig) -> Result<TomlValue> {
        desired_openai_toml_mcp(server)
    }

    fn desired_json_entry(&self, server: &ResolvedMcpConfig) -> Result<JsonValue> {
        desired_openai_json_mcp(server)
    }
}

impl McpFormatWriter for GrokBuildWriter {
    fn prefix_required(&self) -> bool {
        true
    }

    fn default_json_root(&self) -> JsonValue {
        JsonValue::Object(JsonMap::new())
    }

    fn ensure_format_supported(&self, format: McpConfigFileFormat) -> Result<()> {
        if format != McpConfigFileFormat::Toml {
            bail!("Grok Build config type 仅支持 TOML。");
        }
        Ok(())
    }

    fn desired_toml_entry(&self, server: &ResolvedMcpConfig) -> Result<TomlValue> {
        desired_grok_toml_mcp(server)
    }

    fn desired_json_entry(&self, _server: &ResolvedMcpConfig) -> Result<JsonValue> {
        bail!("Grok Build config type 仅支持 TOML。");
    }
}

impl McpFormatWriter for OpenCodeWriter {
    fn prefix_required(&self) -> bool {
        true
    }

    fn default_json_root(&self) -> JsonValue {
        JsonValue::Object(JsonMap::from_iter([(
            "$schema".to_string(),
            JsonValue::String("https://opencode.ai/config.json".to_string()),
        )]))
    }

    fn desired_toml_entry(&self, _server: &ResolvedMcpConfig) -> Result<TomlValue> {
        bail!("OpenCode config type 不支持 TOML。");
    }

    fn desired_json_entry(&self, server: &ResolvedMcpConfig) -> Result<JsonValue> {
        desired_opencode_mcp(server)
    }
}

impl McpFormatWriter for DshPatchWriter {
    fn prefix_required(&self) -> bool {
        false
    }

    // dsh 的配置根是操作列表；dsh 写入路径不走 JSON 根，该方法仅为穷尽。
    fn default_json_root(&self) -> JsonValue {
        JsonValue::Array(Vec::new())
    }

    fn desired_toml_entry(&self, _server: &ResolvedMcpConfig) -> Result<TomlValue> {
        bail!("Dsh config type 仅支持 Cordis patch YAML。");
    }

    fn desired_json_entry(&self, _server: &ResolvedMcpConfig) -> Result<JsonValue> {
        bail!("Dsh config type 仅支持 Cordis patch YAML。");
    }

    fn entry_key_for(&self, name: &str) -> String {
        dsh_server_name(name)
    }

    fn read_existing_entries(
        &self,
        _target: &ResolvedTargetConfig,
        config_path: &Path,
    ) -> Result<BTreeMap<String, JsonValue>> {
        read_existing_dsh_patch_entries(config_path)
    }

    fn preview_entry(
        &self,
        config_path: &Path,
        server: &ResolvedMcpConfig,
    ) -> Result<(String, String)> {
        // 与 JSON/TOML target 一致：preview 先确认文件可解析，再展示条目。
        read_dsh_patch_ops(config_path)?;
        let op = desired_dsh_insert_op(&server.name, server)?;
        let content = serde_yaml::to_string(&vec![op]).context("MCP 预览序列化失败")?;
        Ok(("yaml".to_string(), content))
    }

    fn apply_entry(
        &self,
        _target: &ResolvedTargetConfig,
        config_path: &Path,
        server: &ResolvedMcpConfig,
    ) -> Result<()> {
        apply_dsh_patch_entry(config_path, server)
    }

    fn remove_entry(
        &self,
        target: &ResolvedTargetConfig,
        config_path: &Path,
        server_name: &str,
    ) -> Result<McpTargetMutationResult> {
        remove_dsh_patch_entry(target, config_path, server_name)
    }
}

// 全仓库唯一允许的 config_type 分发点。trait 与分发函数保持 workspace 可见：
// types.rs 的 McpConfigFileFormat 是 workspace 私有，pub(crate) 签名会泄漏它。
pub(super) fn mcp_format_writer(config_type: McpConfigType) -> &'static dyn McpFormatWriter {
    match config_type {
        McpConfigType::Common => &CommonWriter,
        McpConfigType::GrokBuild => &GrokBuildWriter,
        McpConfigType::OpenCode => &OpenCodeWriter,
        McpConfigType::Dsh => &DshPatchWriter,
    }
}

// ---------------------------------------------------------------------------
// 条目构造：Common / GrokBuild 的 TOML 形态，Common / OpenCode 的 JSON 形态
// ---------------------------------------------------------------------------

fn desired_openai_toml_mcp(server: &ResolvedMcpConfig) -> Result<TomlValue> {
    desired_toml_mcp(
        server,
        "http_headers",
        server
            .timeout
            .map(|timeout| TomlValue::Float((timeout as f64) / 1000.0)),
    )
}

fn desired_grok_toml_mcp(server: &ResolvedMcpConfig) -> Result<TomlValue> {
    let timeout = server
        .timeout
        .map(|milliseconds| {
            // Grok 1.0.30 要求 u64 秒，不能静默丢弃毫秒精度。
            if milliseconds % 1000 != 0 {
                bail!("Grok Build MCP timeout 必须是整秒（毫秒值须为 1000 的倍数）。");
            }
            Ok(TomlValue::Integer(i64::try_from(milliseconds / 1000)?))
        })
        .transpose()?;
    desired_toml_mcp(server, "headers", timeout)
}

fn desired_toml_mcp(
    server: &ResolvedMcpConfig,
    headers_key: &str,
    timeout: Option<TomlValue>,
) -> Result<TomlValue> {
    let mut table = toml::map::Map::new();
    table.insert("enabled".to_string(), TomlValue::Boolean(server.enabled));

    if let Some(timeout) = timeout {
        table.insert("tool_timeout_sec".to_string(), timeout);
    }

    match server.transport {
        McpTransport::Stdio => {
            table.insert(
                "command".to_string(),
                TomlValue::String(
                    server
                        .command
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("stdio MCP 缺少 command"))?,
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
                        .ok_or_else(|| anyhow::anyhow!("远程 MCP 缺少 mcp 链接"))?,
                ),
            );

            if !server.headers.is_empty() {
                let headers = server
                    .headers
                    .iter()
                    .map(|(key, value)| (key.clone(), TomlValue::String(value.clone())))
                    .collect();
                table.insert(headers_key.to_string(), TomlValue::Table(headers));
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
                        .ok_or_else(|| anyhow::anyhow!("stdio MCP 缺少 command"))?,
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
                        .ok_or_else(|| anyhow::anyhow!("远程 MCP 缺少 mcp 链接"))?,
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
                .ok_or_else(|| anyhow::anyhow!("stdio MCP 缺少 command"))?;
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
                        .ok_or_else(|| anyhow::anyhow!("远程 MCP 缺少 mcp 链接"))?,
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

// ---------------------------------------------------------------------------
// dsh (DeepSeek Harness): Cordis patch YAML (~/.dsh/cordis.patch.yml)
// ---------------------------------------------------------------------------

// dsh 的 MCP server 是 Cordis 插件树里的 dsh-mcp-client 条目。Reins 只认
// name 为该插件且 config.serverName 匹配的 insert 条目；定位与其它 target
// 按 server key 定位的语义一致，条目 id 不参与定位。patch 文件其余内容
// （无关插件、未知操作）原样保留，绝不整文件覆盖。
const DSH_MCP_CLIENT_PLUGIN: &str = "@deepseek-ai/dsh-mcp-client";
const DSH_DEFAULT_TOOL_CALL_TIMEOUT_MS: u64 = 60_000;

// dsh 要求 serverName 匹配 [A-Za-z0-9_-]{1,32}：非法字符折叠为 -，超长截断。
// 读取侧（inspect 状态归类）与写入侧（DshPatchWriter）共用这一清洗规则。
fn dsh_server_name(server_name: &str) -> String {
    let mut sanitized: String = server_name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    // 映射后只有 ASCII，truncate 不会落在多字节边界上。
    sanitized.truncate(32);
    sanitized
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DshMcpClientConfig {
    server_name: String,
    transport: String,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    cwd: String,
    tool_call_timeout_ms: u64,
    fail_on_startup_error: bool,
}

#[derive(Serialize)]
struct DshInsertEntry {
    id: String,
    name: String,
    config: DshMcpClientConfig,
}

fn desired_dsh_insert_entry(server_name: &str, server: &ResolvedMcpConfig) -> Result<YamlValue> {
    // streamable-http 形态的必填字段契约未定稿，v1 只分发 stdio。
    // 与其写出 dsh 无法启动的条目不如直接失败。
    match server.transport {
        McpTransport::Stdio => {}
        McpTransport::Http | McpTransport::Sse => {
            bail!("dsh 的 MCP 分发仅支持 stdio transport。")
        }
    }

    let entry = DshInsertEntry {
        id: format!("reins-mcp-{server_name}"),
        name: DSH_MCP_CLIENT_PLUGIN.to_string(),
        config: DshMcpClientConfig {
            server_name: server_name.to_string(),
            transport: "stdio".to_string(),
            command: server
                .command
                .clone()
                .ok_or_else(|| anyhow::anyhow!("stdio MCP 缺少 command"))?,
            args: server.args.clone(),
            env: server.env.clone(),
            cwd: home_dir()
                .ok_or_else(|| anyhow::anyhow!("无法解析 HOME 目录。"))?
                .display()
                .to_string(),
            tool_call_timeout_ms: server.timeout.unwrap_or(DSH_DEFAULT_TOOL_CALL_TIMEOUT_MS),
            fail_on_startup_error: false,
        },
    };

    Ok(serde_yaml::to_value(entry).context("dsh MCP 条目序列化失败")?)
}

fn desired_dsh_insert_op(server_name: &str, server: &ResolvedMcpConfig) -> Result<YamlValue> {
    let mut op = serde_yaml::Mapping::new();
    op.insert(
        YamlValue::String("insert".to_string()),
        YamlValue::Sequence(vec![desired_dsh_insert_entry(server_name, server)?]),
    );
    Ok(YamlValue::Mapping(op))
}

fn read_dsh_patch_ops(config_path: &Path) -> Result<Vec<YamlValue>> {
    crate::support::dsh_patch::read_ops(config_path)
}

fn write_dsh_patch_ops(config_path: &Path, ops: &[YamlValue]) -> Result<()> {
    crate::support::dsh_patch::write_ops(config_path, ops)
}

fn is_dsh_claimed_entry(entry: &YamlValue, server_name: &str) -> bool {
    entry.get("name").and_then(YamlValue::as_str) == Some(DSH_MCP_CLIENT_PLUGIN)
        && entry
            .get("config")
            .and_then(|config| config.get("serverName"))
            .and_then(YamlValue::as_str)
            == Some(server_name)
}

fn locate_dsh_claimed_entry(ops: &[YamlValue], server_name: &str) -> Option<(usize, usize)> {
    for (op_index, op) in ops.iter().enumerate() {
        let Some(entries) = op.get("insert").and_then(YamlValue::as_sequence) else {
            continue;
        };
        for (entry_index, entry) in entries.iter().enumerate() {
            if is_dsh_claimed_entry(entry, server_name) {
                return Some((op_index, entry_index));
            }
        }
    }
    None
}

fn dsh_insert_list_mut(ops: &mut [YamlValue], op_index: usize) -> Result<&mut Vec<YamlValue>> {
    ops.get_mut(op_index)
        .and_then(|op| op.get_mut("insert"))
        .and_then(YamlValue::as_sequence_mut)
        .ok_or_else(|| anyhow::anyhow!("Cordis patch 的 insert 操作必须是条目列表。"))
}

fn read_existing_dsh_patch_entries(config_path: &Path) -> Result<BTreeMap<String, JsonValue>> {
    let ops = read_dsh_patch_ops(config_path)?;
    let mut entries = BTreeMap::new();

    for op in &ops {
        let Some(list) = op.get("insert").and_then(YamlValue::as_sequence) else {
            continue;
        };
        for entry in list {
            if entry.get("name").and_then(YamlValue::as_str) != Some(DSH_MCP_CLIENT_PLUGIN) {
                continue;
            }
            let Some(server_name) = entry
                .get("config")
                .and_then(|config| config.get("serverName"))
                .and_then(YamlValue::as_str)
            else {
                continue;
            };
            let config = entry.get("config").cloned().unwrap_or(YamlValue::Null);
            let json = serde_json::to_value(crate::support::dsh_patch::untag(&config))
                .with_context(|| format!("Cordis patch 条目转换失败：{server_name}"))?;
            entries.insert(server_name.to_string(), json);
        }
    }

    Ok(entries)
}

fn apply_dsh_patch_entry(config_path: &Path, server: &ResolvedMcpConfig) -> Result<()> {
    let server_name = dsh_server_name(&server.name);
    let mut ops = read_dsh_patch_ops(config_path)?;

    match locate_dsh_claimed_entry(&ops, &server_name) {
        Some((op_index, entry_index)) => {
            let entry = desired_dsh_insert_entry(&server_name, server)?;
            dsh_insert_list_mut(&mut ops, op_index)?[entry_index] = entry;
        }
        None => ops.push(desired_dsh_insert_op(&server_name, server)?),
    }

    write_dsh_patch_ops(config_path, &ops)
}

fn remove_dsh_patch_entry(
    target: &ResolvedTargetConfig,
    config_path: &Path,
    server_name: &str,
) -> Result<McpTargetMutationResult> {
    let claimed_name = dsh_server_name(server_name);
    let mut ops = read_dsh_patch_ops(config_path)?;

    let Some((op_index, entry_index)) = locate_dsh_claimed_entry(&ops, &claimed_name) else {
        return Ok(McpTargetMutationResult {
            server_name: server_name.to_string(),
            target_id: target.id.clone(),
            updated_path: None,
            action: "noop".to_string(),
            detail: "目标配置里不存在该 MCP。".to_string(),
        });
    };

    let insert_list = dsh_insert_list_mut(&mut ops, op_index)?;
    insert_list.remove(entry_index);
    if insert_list.is_empty() {
        ops.remove(op_index);
    }
    // 删空后保留空列表文件（`[]`），与“不整文件覆盖”的合并语义一致。
    write_dsh_patch_ops(config_path, &ops)?;

    Ok(McpTargetMutationResult {
        server_name: server_name.to_string(),
        target_id: target.id.clone(),
        updated_path: Some(display_path(config_path)),
        action: "remove".to_string(),
        detail: format!("已从 {} 移除 {}", target.id.as_str(), server_name),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    use anyhow::Result;

    use super::mcp_format_writer;
    use crate::test_support::TestDir;
    use crate::workspace::types::{
        AgentTargetId, McpConfigType, McpTransport, ResolvedMcpConfig, ResolvedTargetConfig,
    };

    // 三句 bail 文案是既有契约，改造前后必须逐字一致。
    const GROK_TOML_ONLY: &str = "Grok Build config type 仅支持 TOML。";
    const OPENCODE_JSON_ONLY: &str = "OpenCode config type 不支持 TOML。";
    const DSH_YAML_ONLY: &str = "Dsh config type 仅支持 Cordis patch YAML。";

    fn stdio_server(name: &str) -> ResolvedMcpConfig {
        ResolvedMcpConfig {
            name: name.to_string(),
            enabled: true,
            transport: McpTransport::Stdio,
            created_at: None,
            homepage: None,
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            env: BTreeMap::from([("VALUE".to_string(), "1".to_string())]),
            url: None,
            headers: BTreeMap::new(),
            timeout: Some(2000),
        }
    }

    fn target(
        id: &str,
        config_path: PathBuf,
        prefix: &str,
        config_type: McpConfigType,
    ) -> ResolvedTargetConfig {
        ResolvedTargetConfig {
            id: AgentTargetId(id.to_string()),
            enabled: true,
            is_project: false,
            skill_dir: PathBuf::from("/tmp/reins-mcp-formats-skills"),
            config_path: Some(config_path),
            mcp_config_prefix: prefix.to_string(),
            mcp_config_type: config_type,
        }
    }

    #[test]
    fn common_json_roundtrip_preserves_unrelated_config() -> Result<()> {
        let root = TestDir::new("mcp-fmt-common-json")?;
        let config_path = root.path().join("mcp.json");
        let original = r#"{"topLevel": {"keep": true}, "mcpServers": {"other": {"type": "stdio", "command": "keep"}}}"#;
        fs::write(&config_path, original)?;
        let target = target(
            "claude",
            config_path.clone(),
            "mcpServers",
            McpConfigType::Common,
        );
        let writer = mcp_format_writer(McpConfigType::Common);

        let (format_tag, content) = writer.preview_entry(&config_path, &stdio_server("probe"))?;
        assert_eq!(format_tag, "json");
        let previewed: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(previewed["command"], "node");

        writer.apply_entry(&target, &config_path, &stdio_server("probe"))?;
        let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(written["mcpServers"]["probe"]["command"], "node");
        assert_eq!(written["mcpServers"]["probe"]["args"][0], "server.js");
        assert_eq!(written["mcpServers"]["other"]["command"], "keep");
        assert_eq!(written["topLevel"]["keep"], true);

        assert!(
            writer
                .read_existing_entries(&target, &config_path)?
                .contains_key("probe")
        );

        assert_eq!(
            writer.remove_entry(&target, &config_path, "probe")?.action,
            "remove"
        );
        let after: serde_json::Value = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(after, serde_json::from_str::<serde_json::Value>(original)?);
        assert_eq!(
            writer.remove_entry(&target, &config_path, "probe")?.action,
            "noop"
        );
        Ok(())
    }

    #[test]
    fn common_toml_roundtrip_uses_second_floats() -> Result<()> {
        let root = TestDir::new("mcp-fmt-common-toml")?;
        let config_path = root.path().join("config.toml");
        let original = "model = 'unchanged'\n\n[mcp_servers.other]\ncommand = 'keep'\n";
        fs::write(&config_path, original)?;
        let target = target(
            "codex",
            config_path.clone(),
            "mcp_servers",
            McpConfigType::Common,
        );
        let writer = mcp_format_writer(McpConfigType::Common);

        let (format_tag, content) = writer.preview_entry(&config_path, &stdio_server("probe"))?;
        assert_eq!(format_tag, "toml");
        let previewed: toml::Value = toml::from_str(&content)?;
        assert_eq!(previewed["command"].as_str(), Some("node"));

        writer.apply_entry(&target, &config_path, &stdio_server("probe"))?;
        let written: toml::Value = toml::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(written["model"].as_str(), Some("unchanged"));
        assert_eq!(
            written["mcp_servers"]["probe"]["command"].as_str(),
            Some("node")
        );
        // Common 的 TOML 形态把毫秒 timeout 写成秒（浮点）。
        assert_eq!(
            written["mcp_servers"]["probe"]["tool_timeout_sec"].as_float(),
            Some(2.0)
        );

        assert!(
            writer
                .read_existing_entries(&target, &config_path)?
                .contains_key("probe")
        );

        writer.remove_entry(&target, &config_path, "probe")?;
        let after: toml::Value = toml::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(after, toml::from_str::<toml::Value>(original)?);
        assert_eq!(
            writer.remove_entry(&target, &config_path, "probe")?.action,
            "noop"
        );
        Ok(())
    }

    #[test]
    fn grokbuild_toml_roundtrip_and_json_rejected_verbatim() -> Result<()> {
        let root = TestDir::new("mcp-fmt-grok")?;
        let config_path = root.path().join("config.toml");
        let original = "[mcp_servers.other]\ncommand = 'keep'\n";
        fs::write(&config_path, original)?;
        let target = target(
            "grokbuild",
            config_path.clone(),
            "mcp_servers",
            McpConfigType::GrokBuild,
        );
        let writer = mcp_format_writer(McpConfigType::GrokBuild);

        writer.apply_entry(&target, &config_path, &stdio_server("probe"))?;
        let written: toml::Value = toml::from_str(&fs::read_to_string(&config_path)?)?;
        // GrokBuild 的 TOML 形态要求整秒，写入整数秒。
        assert_eq!(
            written["mcp_servers"]["probe"]["tool_timeout_sec"].as_integer(),
            Some(2)
        );
        assert!(
            writer
                .read_existing_entries(&target, &config_path)?
                .contains_key("probe")
        );

        writer.remove_entry(&target, &config_path, "probe")?;
        let after: toml::Value = toml::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(after, toml::from_str::<toml::Value>(original)?);

        // GrokBuild 只接受 TOML：JSON 路径在任何操作下都按既定文案拒绝。
        let json_path = root.path().join("config.json");
        fs::write(&json_path, r#"{"mcp_servers": {}}"#)?;
        assert_eq!(
            writer
                .preview_entry(&json_path, &stdio_server("probe"))
                .unwrap_err()
                .to_string(),
            GROK_TOML_ONLY
        );
        assert_eq!(
            writer
                .apply_entry(&target, &json_path, &stdio_server("probe"))
                .unwrap_err()
                .to_string(),
            GROK_TOML_ONLY
        );
        assert_eq!(
            writer
                .read_existing_entries(&target, &json_path)
                .unwrap_err()
                .to_string(),
            GROK_TOML_ONLY
        );
        assert_eq!(
            writer
                .remove_entry(&target, &json_path, "probe")
                .unwrap_err()
                .to_string(),
            GROK_TOML_ONLY
        );
        Ok(())
    }

    #[test]
    fn opencode_json_roundtrip_and_toml_rejected_verbatim() -> Result<()> {
        let root = TestDir::new("mcp-fmt-opencode")?;
        let config_path = root.path().join("opencode.json");
        let original = r#"{"theme": "dark"}"#;
        fs::write(&config_path, original)?;
        let target = target(
            "opencode",
            config_path.clone(),
            "mcp.servers",
            McpConfigType::OpenCode,
        );
        let writer = mcp_format_writer(McpConfigType::OpenCode);

        writer.apply_entry(&target, &config_path, &stdio_server("probe"))?;
        let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(written["theme"], "dark");
        assert_eq!(written["mcp"]["servers"]["probe"]["type"], "local");
        assert_eq!(
            written["mcp"]["servers"]["probe"]["command"],
            serde_json::json!(["node", "server.js"])
        );

        assert!(
            writer
                .read_existing_entries(&target, &config_path)?
                .contains_key("probe")
        );

        writer.remove_entry(&target, &config_path, "probe")?;
        let after: serde_json::Value = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
        assert!(after["mcp"]["servers"].get("probe").is_none());
        assert_eq!(after["theme"], "dark");

        // OpenCode 的拒绝点在条目构造（读取 TOML 不拦），文案逐字。
        let toml_path = root.path().join("opencode.toml");
        fs::write(&toml_path, "key = 'value'\n")?;
        assert_eq!(
            writer
                .preview_entry(&toml_path, &stdio_server("probe"))
                .unwrap_err()
                .to_string(),
            OPENCODE_JSON_ONLY
        );
        assert_eq!(
            writer
                .desired_toml_entry(&stdio_server("probe"))
                .unwrap_err()
                .to_string(),
            OPENCODE_JSON_ONLY
        );
        Ok(())
    }

    #[test]
    fn dsh_patch_roundtrip_claims_and_restores_ops() -> Result<()> {
        let root = TestDir::new("mcp-fmt-dsh")?;
        let config_path = root.path().join("cordis.patch.yml");
        fs::write(&config_path, "- remove: [user-owned-node]\n")?;
        let target = target("dsh", config_path.clone(), "", McpConfigType::Dsh);
        let writer = mcp_format_writer(McpConfigType::Dsh);

        let (format_tag, content) = writer.preview_entry(&config_path, &stdio_server("probe"))?;
        assert_eq!(format_tag, "yaml");
        let preview_ops: serde_yaml::Value = serde_yaml::from_str(&content)?;
        assert_eq!(preview_ops.as_sequence().map(Vec::len), Some(1));

        writer.apply_entry(&target, &config_path, &stdio_server("probe"))?;
        let ops: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&config_path)?)?;
        let list = ops.as_sequence().unwrap();
        assert_eq!(list.len(), 2, "用户操作原样保留，Reins 只追加 insert");
        assert!(list[0].get("remove").is_some(), "用户操作在前");
        let entries = list[1]
            .get("insert")
            .and_then(|value| value.as_sequence())
            .unwrap();
        let entry = &entries[0];
        assert_eq!(
            entry.get("name").and_then(|value| value.as_str()),
            Some("@deepseek-ai/dsh-mcp-client")
        );
        let config = entry.get("config").unwrap();
        assert_eq!(
            config.get("serverName").and_then(|value| value.as_str()),
            Some("probe")
        );
        assert_eq!(
            config.get("command").and_then(|value| value.as_str()),
            Some("node")
        );

        // 读取侧按清洗后的 serverName 建键。
        assert!(
            writer
                .read_existing_entries(&target, &config_path)?
                .contains_key("probe")
        );

        assert_eq!(
            writer.remove_entry(&target, &config_path, "probe")?.action,
            "remove"
        );
        let after: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(after.as_sequence().map(Vec::len), Some(1));
        assert!(after.as_sequence().unwrap()[0].get("remove").is_some());
        assert_eq!(
            writer.remove_entry(&target, &config_path, "probe")?.action,
            "noop"
        );

        // dsh 条目不走 TOML/JSON 形态，两个构造入口都按既定文案拒绝。
        assert_eq!(
            writer
                .desired_toml_entry(&stdio_server("probe"))
                .unwrap_err()
                .to_string(),
            DSH_YAML_ONLY
        );
        assert_eq!(
            writer
                .desired_json_entry(&stdio_server("probe"))
                .unwrap_err()
                .to_string(),
            DSH_YAML_ONLY
        );
        Ok(())
    }

    #[test]
    fn entry_key_for_is_identity_except_dsh_sanitized() {
        // 非 dsh writer 的条目 key 与 Reins 的 mcp name 恒等。
        let plain = "my probe.v2/中文";
        for config_type in [
            McpConfigType::Common,
            McpConfigType::GrokBuild,
            McpConfigType::OpenCode,
        ] {
            assert_eq!(mcp_format_writer(config_type).entry_key_for(plain), plain);
        }

        // dsh 的条目 key 是清洗后的 serverName；用例形态与
        // dsh_server_name_sanitization 既有测试保持一致。
        let dsh = mcp_format_writer(McpConfigType::Dsh);
        assert_eq!(dsh.entry_key_for("probe"), "probe");
        assert_eq!(dsh.entry_key_for("my probe.v2/中文"), "my-probe-v2---");
        assert_eq!(dsh.entry_key_for(&"a".repeat(40)), "a".repeat(32));
    }

    #[test]
    fn writer_constraint_declarations() {
        // 前缀必填：dsh 按 name+serverName 定位条目，没有 prefix 概念。
        assert!(mcp_format_writer(McpConfigType::Common).prefix_required());
        assert!(mcp_format_writer(McpConfigType::GrokBuild).prefix_required());
        assert!(mcp_format_writer(McpConfigType::OpenCode).prefix_required());
        assert!(!mcp_format_writer(McpConfigType::Dsh).prefix_required());

        // JSON 默认根：OpenCode 注入 $schema，其余 JSON 形态从空对象开始。
        assert_eq!(
            mcp_format_writer(McpConfigType::Common).default_json_root(),
            serde_json::json!({})
        );
        assert_eq!(
            mcp_format_writer(McpConfigType::GrokBuild).default_json_root(),
            serde_json::json!({})
        );
        assert_eq!(
            mcp_format_writer(McpConfigType::OpenCode).default_json_root(),
            serde_json::json!({"$schema": "https://opencode.ai/config.json"})
        );
    }
}
