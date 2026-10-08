// 各工具全局配置文件的适配器层：每个适配器只负责「生成新文件内容」，
// diff 预览与原子写入统一由 commands 层完成。providers 域与 workspace
// 互不依赖，这里自带一份极小的 JSON/TOML 读写助手。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml::Value as TomlValue;

use super::types::{
    ApplyProviderInput, DiffLine, DiffLineKind, ProviderAppEntry, ProviderAppEntryStatus,
    ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningEffortWrite, ReasoningLevel,
    ResolvedProvider,
};
use crate::support::fs::{display_path, grok_home_path, user_home_dir, write_atomic};

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod dsh;
pub(crate) mod grokbuild;
pub(crate) mod opencode;
pub(crate) mod pi;

// Reins 在各工具配置中的内部注册键前缀；反显按该前缀识别 Reins 条目。
pub(crate) const REINS_PREFIX: &str = "reins-";

// 预览时密钥的脱敏占位符；任何路径（日志、错误、diff）都只允许出现它。
pub(crate) const MASKED_KEY: &str = "******";

pub(crate) fn registration_key(provider_id: &str) -> String {
    format!("{REINS_PREFIX}{provider_id}")
}

// 测试与生产共用的环境入口：生产读真实环境变量，home 目录走
// user_home_dir()（支持测试 override）；测试直接构造 ToolEnv。
#[derive(Clone, Debug, Default)]
pub(crate) struct ToolEnv {
    pub(crate) codex_home: Option<PathBuf>,
    pub(crate) pi_agent_dir: Option<PathBuf>,
    pub(crate) grok_home: Option<PathBuf>,
    pub(crate) claude_config_dir: Option<PathBuf>,
    pub(crate) dsh_home: Option<PathBuf>,
}

impl ToolEnv {
    pub(crate) fn from_env() -> Self {
        Self {
            codex_home: std::env::var("CODEX_HOME").ok().map(PathBuf::from),
            pi_agent_dir: std::env::var("PI_CODING_AGENT_DIR").ok().map(PathBuf::from),
            grok_home: std::env::var("GROK_HOME").ok().map(PathBuf::from),
            claude_config_dir: std::env::var("CLAUDE_CONFIG_DIR").ok().map(PathBuf::from),
            dsh_home: std::env::var("DSH_HOME").ok().map(PathBuf::from),
        }
    }
}

// 工具能力表：协议兼容 + 必填字段门控 + 不写入的元数据 + 思考等级支持集 +
// 替换/并存语义。
pub(crate) struct AppCapability {
    pub(crate) supported_protocols: &'static [ProviderProtocol],
    // true = 多 Provider 并存（新增条目）；false = 单活动 Provider（应用即替换）。
    pub(crate) additive: bool,
    // 目标工具必填的模型元数据；缺失时应用按钮禁用并列出缺失项。
    // v1 五个工具暂无硬必填项，机制保留待 unverified 项验证后填充。
    pub(crate) required_model_fields: &'static [ModelField],
    // 该工具配置里没有落点的模型元数据；应用弹窗据此明示用户填写的
    // 这些值不会影响该工具。
    pub(crate) unwritten_model_fields: &'static [ModelField],
    pub(crate) supported_reasoning_levels: &'static [ReasoningLevel],
}

// 模型元数据维度：required_model_fields 用它做应用门控，
// unwritten_model_fields 用它声明该工具不写入哪些元数据。
// ReasoningLevels 只在前者有意义（等级选择本身经弹窗默认档写入），
// 暂无工具引用，保留待 unverified 项验证。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum ModelField {
    ContextWindow,
    MaxOutputTokens,
    SupportsImages,
    Reasoning,
    ReasoningLevels,
}

impl ModelField {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ModelField::ContextWindow => "上下文窗口",
            ModelField::MaxOutputTokens => "最大输出",
            ModelField::SupportsImages => "图像输入",
            ModelField::Reasoning => "推理能力",
            ModelField::ReasoningLevels => "思考等级",
        }
    }

    fn is_present(self, model: &super::types::ProviderModelRecord) -> bool {
        match self {
            ModelField::ContextWindow => model.context_window.is_some(),
            ModelField::MaxOutputTokens => model.max_output_tokens.is_some(),
            ModelField::SupportsImages => model.supports_images.is_some(),
            ModelField::Reasoning => model.reasoning.is_some(),
            ModelField::ReasoningLevels => model.reasoning_levels.is_some(),
        }
    }
}

pub(crate) trait AppAdapter: Sync {
    fn id(&self) -> ProviderAppId;
    fn capability(&self) -> AppCapability;

    // 思考等级在该平台配置文件里的实际写入值；必须与 apply 的写入
    // 逻辑同源（特殊映射的平台覆写，如 Grok max→xhigh、Pi off→off），
    // 供应用弹窗展示映射，避免前端复刻规则。
    fn reasoning_effort_write(&self, level: ReasoningLevel) -> String {
        level.as_str().to_string()
    }

    // 反读工具配置；providers.yaml 解析结果用于把 reins- 条目归类为
    // 已应用 / 漂移，无法归类的都按外部配置展示。
    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState>;

    // 生成应用后的完整新文件内容列表；不直接落盘。
    fn apply(
        &self,
        env: &ToolEnv,
        provider: &ResolvedProvider,
        plan: &ApplyProviderInput,
        api_key: &str,
    ) -> Result<Vec<(PathBuf, String)>>;

    // 生成移除后的完整新文件内容列表。provider 为 None 表示平台元数据
    // 已删除，只能按注册键结构化识别；不支持这种方式（Claude）时报错。
    fn remove(
        &self,
        env: &ToolEnv,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>>;

    // 删除外部（非 Reins 纳管）provider 条目：按工具配置里的原样键结构化删除。
    // 外部条目内容任意，跳过内容识别校验；默认模型/活动 Provider 仍指向
    // 被删条目时随删除清空。Claude 的外部配置内嵌在 env 中，无可删除条目，
    // 实现固定报错。
    fn remove_external(&self, env: &ToolEnv, entry_key: &str) -> Result<Vec<(PathBuf, String)>>;
}

pub(crate) fn adapter_for(app: ProviderAppId) -> &'static dyn AppAdapter {
    match app {
        ProviderAppId::Codex => &codex::CodexAdapter,
        ProviderAppId::Claude => &claude::ClaudeAdapter,
        ProviderAppId::Opencode => &opencode::OpencodeAdapter,
        ProviderAppId::Pi => &pi::PiAdapter,
        ProviderAppId::Grokbuild => &grokbuild::GrokbuildAdapter,
        ProviderAppId::Dsh => &dsh::DshAdapter,
    }
}

// ---------------------------------------------------------------------------
// 能力检查
// ---------------------------------------------------------------------------

pub(crate) fn ensure_protocol_supported(
    adapter: &dyn AppAdapter,
    provider: &ResolvedProvider,
) -> Result<()> {
    let capability = adapter.capability();
    if !capability.supported_protocols.contains(&provider.protocol) {
        bail!(
            "{} 不支持协议 {}，支持：{}",
            adapter.id().label(),
            protocol_label(provider.protocol),
            capability
                .supported_protocols
                .iter()
                .map(|p| protocol_label(*p))
                .collect::<Vec<_>>()
                .join("、")
        );
    }
    Ok(())
}

pub(crate) fn protocol_label(protocol: ProviderProtocol) -> &'static str {
    match protocol {
        ProviderProtocol::OpenaiResponses => "openai_responses",
        ProviderProtocol::OpenaiChatCompletions => "openai_chat_completions",
        ProviderProtocol::AnthropicMessages => "anthropic_messages",
    }
}

// 应用前置校验：模型选择合法 + 必填元数据齐全；返回能力警告（不阻断）。
pub(crate) fn validate_plan(
    adapter: &dyn AppAdapter,
    provider: &ResolvedProvider,
    plan: &ApplyProviderInput,
) -> Result<Vec<String>> {
    if plan.model_ids.is_empty() {
        bail!("至少选择一个模型。");
    }
    let known: std::collections::BTreeSet<&str> =
        provider.models.iter().map(|m| m.id.as_str()).collect();
    for model_id in &plan.model_ids {
        if !known.contains(model_id.as_str()) {
            bail!("模型 {} 不在平台 {} 的目录中。", model_id, provider.id);
        }
    }
    if !plan.model_ids.contains(&plan.default_model_id) {
        bail!("默认模型必须在已选模型中。");
    }
    if let Some(level) = plan.default_reasoning_level {
        if !adapter
            .capability()
            .supported_reasoning_levels
            .contains(&level)
        {
            bail!(
                "{} 不支持思考等级 {}。",
                adapter.id().label(),
                level.as_str()
            );
        }
    }

    let mut warnings = Vec::new();
    let capability = adapter.capability();
    for model_id in &plan.model_ids {
        let model = provider
            .models
            .iter()
            .find(|m| &m.id == model_id)
            .expect("validated above");
        let missing: Vec<&str> = capability
            .required_model_fields
            .iter()
            .filter(|field| !field.is_present(model))
            .map(|field| field.label())
            .collect();
        if !missing.is_empty() {
            bail!(
                "模型 {model_id} 缺少 {} 必填项：{}",
                adapter.id().label(),
                missing.join("、")
            );
        }
        if model.context_window.is_none() {
            warnings.push(format!("模型 {model_id} 未填写上下文窗口。"));
        }
        if model.max_output_tokens.is_none() {
            warnings.push(format!("模型 {model_id} 未填写最大输出。"));
        }
    }
    warnings.dedup();
    Ok(warnings)
}

// ---------------------------------------------------------------------------
// 配置文件读写原语（providers 域自持，避免依赖 workspace）
// ---------------------------------------------------------------------------

pub(crate) fn read_json_object(path: &Path) -> Result<JsonMap<String, JsonValue>> {
    if !path.exists() {
        return Ok(JsonMap::new());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(JsonMap::new());
    }
    let value: JsonValue = serde_json::from_str(&content)
        .with_context(|| format!("JSON 解析失败：{}", path.display()))?;
    match value {
        JsonValue::Object(map) => Ok(map),
        _ => bail!("配置文件顶层必须是 JSON 对象：{}", path.display()),
    }
}

pub(crate) fn serialize_json(map: &JsonMap<String, JsonValue>) -> Result<String> {
    let mut content = serde_json::to_string_pretty(&JsonValue::Object(map.clone()))?;
    content.push('\n');
    Ok(content)
}

pub(crate) fn read_toml(path: &Path) -> Result<TomlValue> {
    if !path.exists() {
        return Ok(TomlValue::Table(toml::map::Map::new()));
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(TomlValue::Table(toml::map::Map::new()));
    }
    toml::from_str(&content).with_context(|| format!("TOML 解析失败：{}", path.display()))
}

pub(crate) fn serialize_toml(value: &TomlValue) -> Result<String> {
    // 错误上下文不能嵌入文档内容：TOML 里可能带明文密钥。
    toml::to_string_pretty(value).context("TOML 序列化失败")
}

// ---------------------------------------------------------------------------
// 行级 diff（LCS）
// ---------------------------------------------------------------------------

pub(crate) fn diff_lines(old_content: &str, new_content: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    // LCS 动态规划表；工具配置文件都很小，O(n*m) 可接受。
    let mut table = vec![vec![0usize; new_lines.len() + 1]; old_lines.len() + 1];
    for i in (0..old_lines.len()).rev() {
        for j in (0..new_lines.len()).rev() {
            table[i][j] = if old_lines[i] == new_lines[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut diff = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < old_lines.len() && j < new_lines.len() {
        if old_lines[i] == new_lines[j] {
            diff.push(DiffLine {
                kind: DiffLineKind::Context,
                text: old_lines[i].to_string(),
            });
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            diff.push(DiffLine {
                kind: DiffLineKind::Remove,
                text: old_lines[i].to_string(),
            });
            i += 1;
        } else {
            diff.push(DiffLine {
                kind: DiffLineKind::Add,
                text: new_lines[j].to_string(),
            });
            j += 1;
        }
    }
    while i < old_lines.len() {
        diff.push(DiffLine {
            kind: DiffLineKind::Remove,
            text: old_lines[i].to_string(),
        });
        i += 1;
    }
    while j < new_lines.len() {
        diff.push(DiffLine {
            kind: DiffLineKind::Add,
            text: new_lines[j].to_string(),
        });
        j += 1;
    }
    diff
}

// ---------------------------------------------------------------------------
// 共享条目构造（反显）
// ---------------------------------------------------------------------------

pub(crate) fn applied_entry(
    key: String,
    provider: &ResolvedProvider,
    base_url: Option<String>,
    model_ids: Vec<String>,
    default_model_id: Option<String>,
    protocol: Option<ProviderProtocol>,
) -> ProviderAppEntry {
    ProviderAppEntry {
        key,
        status: ProviderAppEntryStatus::Applied,
        provider_id: Some(provider.id.clone()),
        label: Some(provider.label.clone()),
        base_url,
        model_ids,
        default_model_id,
        notes: Vec::new(),
        protocol,
    }
}

pub(crate) fn drifted_entry(
    key: String,
    provider_id: String,
    base_url: Option<String>,
    note: String,
    protocol: Option<ProviderProtocol>,
) -> ProviderAppEntry {
    ProviderAppEntry {
        key,
        status: ProviderAppEntryStatus::Drifted,
        provider_id: Some(provider_id),
        label: None,
        base_url,
        model_ids: Vec::new(),
        default_model_id: None,
        notes: vec![note],
        protocol,
    }
}

pub(crate) fn external_entry(
    key: String,
    base_url: Option<String>,
    notes: Vec<String>,
    protocol: Option<ProviderProtocol>,
) -> ProviderAppEntry {
    ProviderAppEntry {
        key,
        status: ProviderAppEntryStatus::External,
        provider_id: None,
        label: None,
        base_url,
        model_ids: Vec::new(),
        default_model_id: None,
        notes,
        protocol,
    }
}

// reins- 前缀条目的统一归类：providers.yaml 匹配 → 已应用；不匹配或平台
// 已删除 → 漂移；无前缀 → 外部配置（只展示不纳管）。status 字段承载
// 已应用/漂移二态，调用方直接入列。
pub(crate) fn classify_reins_entry(
    key: &str,
    base_url: Option<&str>,
    protocol: Option<ProviderProtocol>,
    providers: &BTreeMap<String, ResolvedProvider>,
) -> ProviderAppEntry {
    let provider_id = key.strip_prefix(REINS_PREFIX).unwrap_or(key).to_string();
    match providers.get(&provider_id) {
        Some(provider) if base_url == Some(provider.base_url.as_str()) => applied_entry(
            key.to_string(),
            provider,
            base_url.map(str::to_string),
            Vec::new(),
            None,
            protocol,
        ),
        Some(provider) => drifted_entry(
            key.to_string(),
            provider_id.clone(),
            base_url.map(str::to_string),
            format!(
                "base_url 与平台 {} 当前配置（{}）不一致。",
                provider.id, provider.base_url
            ),
            protocol,
        ),
        None => drifted_entry(
            key.to_string(),
            provider_id.clone(),
            base_url.map(str::to_string),
            "平台已删除。".to_string(),
            protocol,
        ),
    }
}

pub(crate) fn empty_state(app: ProviderAppId, config_paths: Vec<PathBuf>) -> ProviderAppState {
    let adapter = adapter_for(app);
    let capability = adapter.capability();
    let reasoning_level_writes = capability
        .supported_reasoning_levels
        .iter()
        .map(|level| ReasoningEffortWrite {
            level: *level,
            writes: adapter.reasoning_effort_write(*level),
        })
        .collect();
    ProviderAppState {
        app,
        config_paths: config_paths.iter().map(|p| display_path(p)).collect(),
        config_exists: false,
        supported_protocols: capability.supported_protocols.to_vec(),
        supported_reasoning_levels: capability.supported_reasoning_levels.to_vec(),
        reasoning_level_writes,
        additive: capability.additive,
        unwritten_model_fields: capability
            .unwritten_model_fields
            .iter()
            .map(|field| field.label().to_string())
            .collect(),
        default_reasoning_level: None,
        entries: Vec::new(),
        load_error: None,
    }
}

// ---------------------------------------------------------------------------
// 工具配置路径
// ---------------------------------------------------------------------------

fn home_dir() -> Result<PathBuf> {
    user_home_dir().ok_or_else(|| anyhow::anyhow!("无法解析 HOME 目录。"))
}

pub(crate) fn codex_home_dir(env: &ToolEnv) -> Result<PathBuf> {
    Ok(match &env.codex_home {
        Some(dir) => dir.clone(),
        None => home_dir()?.join(".codex"),
    })
}

pub(crate) fn codex_config_path(env: &ToolEnv) -> Result<PathBuf> {
    Ok(codex_home_dir(env)?.join("config.toml"))
}

pub(crate) fn claude_settings_path(env: &ToolEnv) -> Result<PathBuf> {
    // Claude Code 官方支持 CLAUDE_CONFIG_DIR 重定位配置目录，与
    // CODEX_HOME / PI_CODING_AGENT_DIR / GROK_HOME 同一模式。
    Ok(match &env.claude_config_dir {
        Some(dir) => dir.clone(),
        None => home_dir()?.join(".claude"),
    }
    .join("settings.json"))
}

// OpenCode 在 Windows 上不认 XDG_CONFIG_HOME（v2.0.16 实测，官方文档未写
// 差异），统一按 HOME/.config 解析，避免与工具实际读取位置分叉。
pub(crate) fn opencode_config_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config").join("opencode"))
}

// opencode.json 与 opencode.jsonc 都会被 OpenCode 加载并按顶层键合并，
// 后者覆盖前者（v2.0.18 config/discovery.ts + config.ts findLast）；
// 顺序即优先级：低 → 高。
pub(crate) fn opencode_candidate_paths() -> Result<Vec<PathBuf>> {
    let dir = opencode_config_dir()?;
    Ok(vec![
        dir.join("opencode.json"),
        dir.join("opencode.jsonc"),
    ])
}

// jsonc 容忍注释与尾逗号，用 json5 解析；行为与 read_json_object 对齐。
pub(crate) fn read_jsonc_object(path: &Path) -> Result<JsonMap<String, JsonValue>> {
    if !path.exists() {
        return Ok(JsonMap::new());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(JsonMap::new());
    }
    let value: JsonValue = json5::from_str(&content)
        .with_context(|| format!("JSONC 解析失败：{}", path.display()))?;
    match value {
        JsonValue::Object(map) => Ok(map),
        _ => bail!("配置文件顶层必须是 JSON 对象：{}", path.display()),
    }
}

pub(crate) fn pi_models_path(env: &ToolEnv) -> Result<PathBuf> {
    Ok(pi_agent_dir(env)?.join("models.json"))
}

pub(crate) fn pi_settings_path(env: &ToolEnv) -> Result<PathBuf> {
    Ok(pi_agent_dir(env)?.join("settings.json"))
}

fn pi_agent_dir(env: &ToolEnv) -> Result<PathBuf> {
    let base = match &env.pi_agent_dir {
        Some(dir) => dir.clone(),
        None => home_dir()?.join(".pi").join("agent"),
    };
    Ok(base)
}

pub(crate) fn grok_config_path(env: &ToolEnv) -> Result<PathBuf> {
    let home = Some(home_dir()?);
    let base = grok_home_path(
        env.grok_home
            .as_ref()
            .map(|path| path.as_os_str().to_os_string()),
        home,
    )
    .ok_or_else(|| anyhow::anyhow!("无法解析 GROK_HOME 目录。"))?;
    Ok(base.join("config.toml"))
}

// dsh 的 settings.yaml 与 GROK_HOME / PI_CODING_AGENT_DIR 同一重定向模式：
// 优先 $DSH_HOME，默认 ~/.dsh。
pub(crate) use dsh::{dsh_credentials_path, dsh_profile_patch_path};

// ---------------------------------------------------------------------------
// 共享写入助手
// ---------------------------------------------------------------------------

pub(crate) fn write_files(files: &[(PathBuf, String)]) -> Result<()> {
    for (path, content) in files {
        write_atomic(path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}
