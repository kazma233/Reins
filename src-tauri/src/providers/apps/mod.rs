// 各工具全局配置文件的适配器层：每个适配器只负责「生成新文件内容」，
// diff 预览与原子写入统一由 commands 层完成。providers 域与 workspace
// 互不依赖，这里自带一份极小的 JSON/TOML 读写助手。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml::Value as TomlValue;

use super::types::{
    ApplyProviderInput, DiffLine, DiffLineKind, ProviderAppEntry, ProviderAppEntryStatus,
    ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningEffortWrite, ReasoningLevel,
    ResolvedProvider,
};
use crate::support::fs::{display_path, write_atomic, write_atomic_private};
use dialect::{ProtocolDialect, ReasoningDialect};

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod dialect;
pub(crate) mod dsh;
pub(crate) mod grokbuild;
pub(crate) mod opencode;
pub(crate) mod pi;
pub(crate) mod zcode;

// Reins 在各工具配置中的内部注册键前缀；反显按该前缀识别 Reins 条目。
pub(crate) const REINS_PREFIX: &str = "reins-";

// 预览时密钥的脱敏占位符；任何路径（日志、错误、diff）都只允许出现它。
pub(crate) const MASKED_KEY: &str = "******";

pub(crate) fn registration_key(provider_id: &str) -> String {
    format!("{REINS_PREFIX}{provider_id}")
}

// 移除路径的三句报错,全部条目生命周期共用、逐字单一来源:条目内容被
// 手工改过不自动删;注册键查不到;外部键查不到。app_label 是各家文案里
// 的既有主语(Codex/OpenCode/Pi/Grok/dsh),不做大小写统一。
pub(crate) fn modified_entry_error(key: &str) -> anyhow::Error {
    anyhow!("条目 {key} 已被手工修改，无法自动识别，请手动处理。")
}

pub(crate) fn unrecognized_entry_error(app_label: &str, registration: &str) -> anyhow::Error {
    anyhow!("{app_label} 中没有可识别的 {registration} 条目。")
}

pub(crate) fn entry_not_found_error(app_label: &str, entry_key: &str) -> anyhow::Error {
    anyhow!("{app_label} 中没有外部条目 {entry_key}。")
}

// 环境视图与工具目录重定向的唯一来源在 support::environment：这里 re-export
// 供各适配器沿用 `super::Environment` 的既有写法，不再维护第二份字段定义。
pub(crate) use crate::support::environment::Environment;
use crate::support::environment::{self, Redirect};

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelField {
    MaxOutputTokens,
    SupportsImages,
    Reasoning,
}

impl ModelField {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ModelField::MaxOutputTokens => "最大输出",
            ModelField::SupportsImages => "图像输入",
            ModelField::Reasoning => "推理能力",
        }
    }

    fn is_present(self, model: &super::types::ProviderModelRecord) -> bool {
        match self {
            ModelField::MaxOutputTokens => model.max_output_tokens.is_some(),
            ModelField::SupportsImages => model.supports_images.is_some(),
            ModelField::Reasoning => model.reasoning.is_some(),
        }
    }
}

pub(crate) trait AppAdapter: Sync {
    fn id(&self) -> ProviderAppId;
    fn capability(&self) -> AppCapability;

    // 该工具在 providers 域管理的配置文件清单（能力声明）：inspect 失败的
    // 兜底展示与外部调用共用；路径解析失败由调用方兜底为空列表。
    // 注意与 inspect 内部读取的文件集不一定相同（如 dsh 的 inspect 还会
    // 读 workspace 域管理的全局 cordis patch）。
    fn config_paths(&self, env: &Environment) -> Result<Vec<PathBuf>>;

    // 敏感文件清单（凭据等）：写入时强制 owner-only 权限。默认空；
    // 依赖方若对文件权限有守卫（如 dsh 启动时拒绝加载非 0600 的凭据），
    // 必须在此声明，否则原子写会用默认 umask 权限落盘。
    fn restricted_paths(&self, _env: &Environment) -> Vec<PathBuf> {
        Vec::new()
    }

    // 本工具的思考档位方言：apply 写入、inspect 反读、弹窗映射文案全部
    // 从这张表派生（特殊映射在表内声明，如 Grok max→xhigh、pi-ai
    // off→off），正反互逆由 roundtrip 测试守卫。
    fn reasoning_dialect(&self) -> &'static ReasoningDialect;

    // 本工具的协议方言；配置文件里没有协议字段、语义固定的工具（Claude）
    // 返回 None。roundtrip 测试经适配器遍历取用；生产路径直接引用各方言
    // 常量。
    #[cfg_attr(not(test), allow(dead_code))]
    fn protocol_dialect(&self) -> Option<&'static ProtocolDialect>;

    // 反读工具配置；providers.yaml 解析结果用于把 reins- 条目归类为
    // 已应用 / 漂移，无法归类的都按外部配置展示。
    fn inspect(
        &self,
        env: &Environment,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState>;

    // 生成应用后的完整新文件内容列表；不直接落盘。
    fn apply(
        &self,
        env: &Environment,
        provider: &ResolvedProvider,
        plan: &ApplyProviderInput,
        api_key: &str,
    ) -> Result<Vec<(PathBuf, String)>>;

    // 生成移除后的完整新文件内容列表。provider 为 None 表示平台元数据
    // 已删除，只能按注册键结构化识别；不支持这种方式（Claude）时报错。
    fn remove(
        &self,
        env: &Environment,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>>;

    // 删除外部（非 Reins 纳管）provider 条目：按工具配置里的原样键结构化删除。
    // 外部条目内容任意，跳过内容识别校验；默认模型/活动 Provider 仍指向
    // 被删条目时随删除清空。Claude 的外部配置内嵌在 env 中，无可删除条目，
    // 实现固定报错。
    fn remove_external(&self, env: &Environment, entry_key: &str)
    -> Result<Vec<(PathBuf, String)>>;
}

pub(crate) fn adapter_for(app: ProviderAppId) -> &'static dyn AppAdapter {
    match app {
        ProviderAppId::Codex => &codex::CodexAdapter,
        ProviderAppId::Claude => &claude::ClaudeAdapter,
        ProviderAppId::Opencode => &opencode::OpencodeAdapter,
        ProviderAppId::Pi => &pi::PiAdapter,
        ProviderAppId::Grokbuild => &grokbuild::GrokbuildAdapter,
        ProviderAppId::Dsh => &dsh::DshAdapter,
        ProviderAppId::Zcode => &zcode::ZcodeAdapter,
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
    classify_reins_entry_with(key, base_url, protocol, providers, |entry, provider| {
        entry == Some(provider.base_url.as_str())
    })
}

// 比较口径钩子:entry 的 base_url 与平台 base_url 怎么算一致。Grok 的
// 写入口径会把 Anthropic 平台 base_url 折算成 /v1 结尾,恒等比较会把它
// 错判成漂移;口径不是恒等的适配器经此声明,不必克隆视图凑相等。
pub(crate) fn classify_reins_entry_with(
    key: &str,
    base_url: Option<&str>,
    protocol: Option<ProviderProtocol>,
    providers: &BTreeMap<String, ResolvedProvider>,
    base_url_matches: fn(Option<&str>, &ResolvedProvider) -> bool,
) -> ProviderAppEntry {
    let provider_id = key.strip_prefix(REINS_PREFIX).unwrap_or(key).to_string();
    match providers.get(&provider_id) {
        Some(provider) if base_url_matches(base_url, provider) => applied_entry(
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
    let reasoning_dialect = adapter.reasoning_dialect();
    let reasoning_level_writes = capability
        .supported_reasoning_levels
        .iter()
        .map(|level| ReasoningEffortWrite {
            level: *level,
            writes: reasoning_dialect.spell(*level).to_string(),
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
//
// 目录的相对布局不在本文件重述：根经 Environment 按重定向解析，配置文件名
// 从 agents 清单派生（mcp_config_path）；只有"同一目录下的另一个文件"
// （codex 的 reins-models.json、pi 的 models.json/settings.json、claude 的
// settings.json）在这里显式拼接。
// ---------------------------------------------------------------------------

pub(crate) fn codex_home_dir(env: &Environment) -> Result<PathBuf> {
    env.resolve_required(Redirect::CodexHome)
}

pub(crate) fn codex_config_path(env: &Environment) -> Result<PathBuf> {
    environment::mcp_config_path(env, ProviderAppId::Codex)
        .ok_or_else(|| anyhow::anyhow!("codex 缺少 MCP 配置入口。"))
}

pub(crate) fn claude_config_dir(env: &Environment) -> Result<PathBuf> {
    env.resolve_required(Redirect::ClaudeConfigDir)
}

pub(crate) fn claude_settings_path(env: &Environment) -> Result<PathBuf> {
    Ok(claude_config_dir(env)?.join("settings.json"))
}

// opencode.json 与 opencode.jsonc 都会被 OpenCode 加载并按顶层键合并，
// 后者覆盖前者（v2.0.18 config/discovery.ts + config.ts findLast）；
// 顺序即优先级：低 → 高。基名从 agents 清单派生（与 codex/grok 同一条
// 路径来源），jsonc 是清单不表达的工具加载语义，按同目录兄弟候选枚举。
// OpenCode 在 Windows 上不认 XDG_CONFIG_HOME（v2.0.16 实测，官方文档未写
// 差异）；清单登记的就是 HOME 相对路径，派生结果与该行为一致。
pub(crate) fn opencode_candidate_paths(env: &Environment) -> Result<Vec<PathBuf>> {
    let base = environment::mcp_config_path(env, ProviderAppId::Opencode)
        .ok_or_else(|| anyhow::anyhow!("OpenCode 配置路径未在 agents 清单登记"))?;
    let jsonc = base.with_extension("jsonc");
    Ok(vec![base, jsonc])
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
    let value: JsonValue =
        json5::from_str(&content).with_context(|| format!("JSONC 解析失败：{}", path.display()))?;
    match value {
        JsonValue::Object(map) => Ok(map),
        _ => bail!("配置文件顶层必须是 JSON 对象：{}", path.display()),
    }
}

pub(crate) fn pi_models_path(env: &Environment) -> Result<PathBuf> {
    Ok(pi_agent_dir(env)?.join("models.json"))
}

pub(crate) fn pi_settings_path(env: &Environment) -> Result<PathBuf> {
    Ok(pi_agent_dir(env)?.join("settings.json"))
}

fn pi_agent_dir(env: &Environment) -> Result<PathBuf> {
    env.resolve_required(Redirect::PiAgentDir)
}

pub(crate) fn grok_config_path(env: &Environment) -> Result<PathBuf> {
    environment::mcp_config_path(env, ProviderAppId::Grokbuild)
        .ok_or_else(|| anyhow::anyhow!("grok 缺少 MCP 配置入口。"))
}

// dsh 的 settings.yaml 与 GROK_HOME / PI_CODING_AGENT_DIR 同一重定向模式：
// 优先 $DSH_HOME，默认 ~/.dsh。路径函数定义在 dsh.rs，由其
// config_paths() 能力声明对外。

// ---------------------------------------------------------------------------
// 共享写入助手
// ---------------------------------------------------------------------------

// restricted 内的路径按敏感文件写（强制 owner-only 权限），其余沿用
// 原子写；受限清单由写入方从 adapter 的 restricted_paths() 取。
pub(crate) fn write_files(files: &[(PathBuf, String)], restricted: &[PathBuf]) -> Result<()> {
    for (path, content) in files {
        let write = if restricted.contains(path) {
            write_atomic_private
        } else {
            write_atomic
        };
        write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}
