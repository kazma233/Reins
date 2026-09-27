// Grok Build：$GROK_HOME/config.toml（默认 ~/.grok/config.toml）。
// 多 Provider 并存：[model_providers.reins-x] + 每个模型一个
// [model."reins-x--<id>"] 条目；默认值写 [models].default 与
// [models].default_reasoning_effort。api_backend 取值已获官方确认
// （docs.x.ai/build/settings/reference：chat_completions/responses/messages），
// 但 [model_providers] 表本身官方未文档化，见验证文档备注。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use toml::Value as TomlValue;

use super::{
    AppAdapter, AppCapability, ToolEnv, display_path, ensure_protocol_supported, external_entry,
    grok_config_path, read_toml, registration_key, serialize_toml,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningLevel,
    ResolvedProvider,
};

pub(crate) struct GrokbuildAdapter;

const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
const ANTHROPIC_VERSION: &str = "2023-06-01";

// Grok 1.0.40 实测 + 官方 settings-reference 确认：api_backend 合法取值为
// chat_completions / responses / messages；provider 表不接受 name；
// default_reasoning_effort 必须放在 [models] 表内。
fn api_backend(protocol: ProviderProtocol) -> &'static str {
    match protocol {
        ProviderProtocol::OpenaiResponses => "responses",
        ProviderProtocol::OpenaiChatCompletions => "chat_completions",
        ProviderProtocol::AnthropicMessages => "messages",
    }
}

// Grok 的 default_reasoning_effort 配置层接受 max，但交互 UI 只渲染
// low/medium/high/xhigh 四档，且 messages 后端请求层 max 与 xhigh 的
// thinking 参数完全相同（1.0.41 实测）；映射后弹窗所选档位在 Grok
// 界面可见、请求等价。
fn grok_reasoning_effort(level: ReasoningLevel) -> &'static str {
    match level {
        ReasoningLevel::Max => "xhigh",
        _ => level.as_str(),
    }
}

// Grok 的 messages 后端固定请求 {base_url}/messages，不补 /v1 段
// （grok 1.0.41 本地监听实测）；而 anthropic 生态约定 base_url 不带
// /v1、由 SDK 拼版本段。写入时统一补 /v1，反读比对按同一口径折算，
// 避免应用后误判漂移。
fn grok_base_url(protocol: ProviderProtocol, base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if protocol == ProviderProtocol::AnthropicMessages && !trimmed.ends_with("/v1") {
        format!("{trimmed}/v1")
    } else {
        base_url.to_string()
    }
}

// 反读时的 providers.yaml 视图：base_url 折算到 Grok 写入口径，
// 供 classify_reins_entry 比对。
fn grok_providers_view(
    providers: &BTreeMap<String, ResolvedProvider>,
) -> BTreeMap<String, ResolvedProvider> {
    providers
        .iter()
        .map(|(id, provider)| {
            let mut view = provider.clone();
            view.base_url = grok_base_url(provider.protocol, &provider.base_url);
            (id.clone(), view)
        })
        .collect()
}

// 反读时从 model_providers 的 api_backend 字段反推协议；未知取值留空展示。
fn protocol_from_backend(backend: Option<&str>) -> Option<ProviderProtocol> {
    match backend? {
        "responses" => Some(ProviderProtocol::OpenaiResponses),
        "chat_completions" => Some(ProviderProtocol::OpenaiChatCompletions),
        "messages" => Some(ProviderProtocol::AnthropicMessages),
        _ => None,
    }
}

fn model_entry_key(registration: &str, model_id: &str) -> String {
    format!("{registration}--{model_id}")
}

// remove 与 remove_external 的共享删除路径：清掉指向该键的默认值、
// 删除 model_providers 条目与 `{key}--` 前缀的模型条目。require_recognizable
// 区分两条路径的防护：reins- 条目内容被手工改过时不自动删，外部条目
// 内容任意、直接删。返回是否删除了内容，无条目的报错文案由调用方决定。
fn remove_provider_tree(
    root_table: &mut toml::map::Map<String, TomlValue>,
    key: &str,
    require_recognizable: bool,
) -> Result<bool> {
    // 默认模型仍指向该条目时随移除清掉；思考等级修饰默认值，一并清。
    let default_refs_key = root_table
        .get("models")
        .and_then(|value| value.get("default"))
        .and_then(TomlValue::as_str)
        .is_some_and(|default| default.starts_with(&format!("{key}--")));
    if default_refs_key {
        if let Some(models_table) = root_table
            .get_mut("models")
            .and_then(TomlValue::as_table_mut)
        {
            models_table.remove("default");
            models_table.remove("default_reasoning_effort");
        }
    }

    let mut removed = false;
    if let Some(providers_table) = root_table
        .get_mut("model_providers")
        .and_then(TomlValue::as_table_mut)
    {
        if let Some(entry) = providers_table.get(key) {
            if require_recognizable {
                let recognizable = entry.get("base_url").and_then(TomlValue::as_str).is_some()
                    && entry
                        .get("api_backend")
                        .and_then(TomlValue::as_str)
                        .is_some();
                if !recognizable {
                    bail!("条目 {key} 已被手工修改，无法自动识别，请手动处理。");
                }
            }
            providers_table.remove(key);
            removed = true;
        }
    }

    if let Some(model_table) = root_table
        .get_mut("model")
        .and_then(TomlValue::as_table_mut)
    {
        let stale_keys: Vec<String> = model_table
            .iter()
            .filter(|(model_key, value)| {
                model_key.starts_with(&format!("{key}--"))
                    && value.get("model_provider").and_then(TomlValue::as_str) == Some(key)
            })
            .map(|(model_key, _)| model_key.clone())
            .collect();
        for model_key in stale_keys {
            model_table.remove(&model_key);
            removed = true;
        }
        if model_table.is_empty() {
            root_table.remove("model");
        }
    }

    Ok(removed)
}

impl AppAdapter for GrokbuildAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Grokbuild
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: ProviderProtocol::all(),
            additive: true,
            required_model_fields: &[],
            supported_reasoning_levels: ReasoningLevel::all(),
        }
    }

    fn reasoning_effort_write(&self, level: ReasoningLevel) -> String {
        grok_reasoning_effort(level).to_string()
    }

    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let path = grok_config_path(env)?;
        let mut state = super::empty_state(ProviderAppId::Grokbuild, vec![path.clone()]);
        if !path.exists() {
            return Ok(state);
        }
        state.config_exists = true;
        let root = read_toml(&path)?;
        let default_model = root
            .get("models")
            .and_then(|value| value.get("default"))
            .and_then(TomlValue::as_str)
            .map(str::to_string);

        if let Some(table) = root.get("model_providers").and_then(TomlValue::as_table) {
            let grok_providers = grok_providers_view(providers);
            for (key, value) in table {
                let base_url = value.get("base_url").and_then(TomlValue::as_str);
                let protocol =
                    protocol_from_backend(value.get("api_backend").and_then(TomlValue::as_str));
                let entry = if key.starts_with(super::REINS_PREFIX) {
                    let mut entry =
                        super::classify_reins_entry(key, base_url, protocol, &grok_providers);
                    // 属于该 provider 的模型条目：model 表中 model_provider
                    // 指向注册键的 reins- 键。
                    if let Some(models) = root.get("model").and_then(TomlValue::as_table) {
                        for (model_key, model_value) in models {
                            if model_key.starts_with(&format!("{key}--")) {
                                if let Some(model_id) =
                                    model_value.get("model").and_then(TomlValue::as_str)
                                {
                                    entry.model_ids.push(model_id.to_string());
                                }
                            }
                        }
                        entry.model_ids.sort();
                    }
                    if let Some(default_model) = &default_model {
                        let prefix = format!("{key}--");
                        if let Some(model_id) = default_model.strip_prefix(&prefix) {
                            entry.default_model_id = Some(model_id.to_string());
                        }
                    }
                    entry
                } else {
                    let label = value
                        .get("name")
                        .and_then(TomlValue::as_str)
                        .map(str::to_string);
                    external_entry(
                        key.clone(),
                        base_url.map(str::to_string),
                        label
                            .map(|label| vec![format!("外部 Provider：{label}")])
                            .unwrap_or_default(),
                        protocol,
                    )
                };
                state.entries.push(entry);
            }
        }

        Ok(state)
    }

    fn apply(
        &self,
        env: &ToolEnv,
        provider: &ResolvedProvider,
        plan: &ApplyProviderInput,
        api_key: &str,
    ) -> Result<Vec<(PathBuf, String)>> {
        ensure_protocol_supported(self, provider)?;
        let path = grok_config_path(env)?;
        let mut root = read_toml(&path)?;
        if !root.is_table() {
            bail!("Grok 配置顶层必须是表：{}", path.display());
        }
        let root_table = root.as_table_mut().expect("checked above");
        let registration = registration_key(&provider.id);

        // 1. model_providers 段
        let providers_table = root_table
            .entry("model_providers")
            .or_insert_with(|| TomlValue::Table(toml::map::Map::new()));
        if !providers_table.is_table() {
            bail!("model_providers 段不是表：{}", path.display());
        }
        let mut provider_table = toml::map::Map::new();
        // 实测：provider 表不接受 name 字段（unknown-field 警告）。
        provider_table.insert(
            "base_url".to_string(),
            TomlValue::String(grok_base_url(provider.protocol, &provider.base_url)),
        );
        provider_table.insert(
            "api_backend".to_string(),
            TomlValue::String(api_backend(provider.protocol).to_string()),
        );
        providers_table
            .as_table_mut()
            .expect("checked above")
            .insert(registration.clone(), TomlValue::Table(provider_table));

        // 2. 每个选中模型一个 model 表条目；重新应用时同步移除不在本次
        // 选择里的旧 reins 模型条目（替换该平台的模型集合）。
        let model_table = root_table
            .entry("model")
            .or_insert_with(|| TomlValue::Table(toml::map::Map::new()));
        if !model_table.is_table() {
            bail!("model 段不是表：{}", path.display());
        }
        let model_table = model_table.as_table_mut().expect("checked above");
        let stale_keys: Vec<String> = model_table
            .keys()
            .filter(|key| key.starts_with(&format!("{registration}--")))
            .cloned()
            .collect();
        for key in stale_keys {
            model_table.remove(&key);
        }
        for model_id in &plan.model_ids {
            let model = provider
                .models
                .iter()
                .find(|m| &m.id == model_id)
                .expect("validated by validate_plan");
            let mut entry = toml::map::Map::new();
            entry.insert("name".to_string(), TomlValue::String(model.label.clone()));
            entry.insert("model".to_string(), TomlValue::String(model.id.clone()));
            entry.insert(
                "model_provider".to_string(),
                TomlValue::String(registration.clone()),
            );
            if provider.protocol == ProviderProtocol::AnthropicMessages {
                // anthropic 协议的模型条目用 extra_headers 携带 x-api-key
                // 与 anthropic-version，不写 api_key 字段。
                let mut headers = toml::map::Map::new();
                headers.insert(
                    "x-api-key".to_string(),
                    TomlValue::String(api_key.to_string()),
                );
                headers.insert(
                    ANTHROPIC_VERSION_HEADER.to_string(),
                    TomlValue::String(ANTHROPIC_VERSION.to_string()),
                );
                entry.insert("extra_headers".to_string(), TomlValue::Table(headers));
            } else {
                entry.insert(
                    "api_key".to_string(),
                    TomlValue::String(api_key.to_string()),
                );
            }
            model_table.insert(
                model_entry_key(&registration, model_id),
                TomlValue::Table(entry),
            );
        }

        // 3. 默认值：default 与 default_reasoning_effort 都在 [models]
        // 表内（顶层 default_reasoning_effort 会被 grok 判为未知键）。
        let models_section = root_table
            .entry("models")
            .or_insert_with(|| TomlValue::Table(toml::map::Map::new()));
        if !models_section.is_table() {
            bail!("models 段不是表：{}", path.display());
        }
        let models_table = models_section.as_table_mut().expect("checked above");
        models_table.insert(
            "default".to_string(),
            TomlValue::String(model_entry_key(&registration, &plan.default_model_id)),
        );
        if let Some(level) = plan.default_reasoning_level {
            models_table.insert(
                "default_reasoning_effort".to_string(),
                TomlValue::String(grok_reasoning_effort(level).to_string()),
            );
        }

        Ok(vec![(path, serialize_toml(&root)?)])
    }

    fn remove(
        &self,
        env: &ToolEnv,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>> {
        let _ = provider;
        let path = grok_config_path(env)?;
        if !path.exists() {
            bail!("Grok 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_toml(&path)?;
        if !root.is_table() {
            bail!("Grok 配置顶层必须是表：{}", path.display());
        }
        let root_table = root.as_table_mut().expect("checked above");
        let registration = registration_key(provider_id);
        if !remove_provider_tree(root_table, &registration, true)? {
            bail!("Grok 中没有可识别的 {registration} 条目。");
        }
        Ok(vec![(path, serialize_toml(&root)?)])
    }

    fn remove_external(&self, env: &ToolEnv, entry_key: &str) -> Result<Vec<(PathBuf, String)>> {
        let path = grok_config_path(env)?;
        if !path.exists() {
            bail!("Grok 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_toml(&path)?;
        if !root.is_table() {
            bail!("Grok 配置顶层必须是表：{}", path.display());
        }
        let root_table = root.as_table_mut().expect("checked above");
        if !remove_provider_tree(root_table, entry_key, false)? {
            bail!("Grok 中没有外部条目 {entry_key}。");
        }
        Ok(vec![(path, serialize_toml(&root)?)])
    }
}
