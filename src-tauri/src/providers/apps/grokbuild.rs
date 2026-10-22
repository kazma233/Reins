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

use super::dialect::{GROK_PROTOCOL, GROK_REASONING};
use super::{
    AppAdapter, AppCapability, Environment, ModelField, display_path, ensure_protocol_supported,
    entry_not_found_error, external_entry, grok_config_path, modified_entry_error, read_toml,
    registration_key, serialize_toml, unrecognized_entry_error,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningLevel,
    ResolvedProvider,
};

pub(crate) struct GrokbuildAdapter;

const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
const ANTHROPIC_VERSION: &str = "2023-06-01";

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

// 反读比对口径:Anthropic 平台在 Grok 配置里写的是补了 /v1 的 base_url
// (见 grok_base_url),classify 前对平台侧做同样折算再比较。
fn grok_base_url_matches(entry_base_url: Option<&str>, provider: &ResolvedProvider) -> bool {
    entry_base_url == Some(grok_base_url(provider.protocol, &provider.base_url).as_str())
}

fn model_entry_key(registration: &str, model_id: &str) -> String {
    format!("{registration}--{model_id}")
}

// models.default 与 default_reasoning_effort 是一对:默认指向被删条目时
// 随删除清掉,单独留思考等级没有意义。Provider 树与独立模型两条删除路径
// 共用。
fn clear_models_default(root_table: &mut toml::map::Map<String, TomlValue>) {
    if let Some(models_table) = root_table
        .get_mut("models")
        .and_then(TomlValue::as_table_mut)
    {
        models_table.remove("default");
        models_table.remove("default_reasoning_effort");
    }
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
        clear_models_default(root_table);
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
                    return Err(modified_entry_error(key));
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

// 独立模型条目（[model.<键>] 自带 base_url，不挂 model_provider）的删除路径：
// 删掉该模型键，并在它被 [models].default 引用时一并清默认值，避免留下指向
// 缺失模型的引用。返回是否删除了内容。
fn remove_standalone_model(root_table: &mut toml::map::Map<String, TomlValue>, key: &str) -> bool {
    let mut removed = false;
    if let Some(model_table) = root_table
        .get_mut("model")
        .and_then(TomlValue::as_table_mut)
    {
        removed = model_table.remove(key).is_some();
        if model_table.is_empty() {
            root_table.remove("model");
        }
    }
    if !removed {
        return false;
    }

    let is_default = root_table
        .get("models")
        .and_then(|value| value.get("default"))
        .and_then(TomlValue::as_str)
        .is_some_and(|default| default == key);
    if is_default {
        clear_models_default(root_table);
    }
    true
}

impl AppAdapter for GrokbuildAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Grokbuild
    }

    fn config_paths(&self, env: &Environment) -> Result<Vec<PathBuf>> {
        Ok(vec![grok_config_path(env)?])
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: ProviderProtocol::all(),
            additive: true,
            required_model_fields: &[],
            // Grok 有 context_window 与 supports_reasoning_effort 落点；
            // 最大输出（max_completion_tokens 属采样上限，Reins 不写）与
            // 图像输入（配置无对应字段）无落点。
            unwritten_model_fields: &[ModelField::MaxOutputTokens, ModelField::SupportsImages],
            supported_reasoning_levels: ReasoningLevel::all(),
            behavior_notes: &[
                "选择最高（max）档时实际以 xhigh 落盘：Grok 交互界面只渲染到 xhigh，两档的 thinking 参数完全相同（1.0.41 实测）。",
                "Anthropic Messages 协议下 Grok 固定请求 {Base URL}/messages，不自动补 /v1 段。",
            ],
        }
    }

    fn reasoning_dialect(&self) -> &'static super::dialect::ReasoningDialect {
        &GROK_REASONING
    }

    fn protocol_dialect(&self) -> Option<&'static super::dialect::ProtocolDialect> {
        Some(&GROK_PROTOCOL)
    }

    fn inspect(
        &self,
        env: &Environment,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let path = grok_config_path(env)?;
        let mut state = super::empty_state(ProviderAppId::Grokbuild, vec![path.clone()]);
        if !path.exists() {
            return Ok(state);
        }
        state.config_exists = true;
        let root = read_toml(&path)?;
        // 应用弹窗据此预选；反读值只用于回显，不参与漂移判定。写入时 max
        // 已折算成 xhigh，反读同样是 xhigh，两边写入值等价。
        state.default_reasoning_level = root
            .get("models")
            .and_then(|value| value.get("default_reasoning_effort"))
            .and_then(TomlValue::as_str)
            .and_then(|value| self.reasoning_dialect().parse(value));
        let default_model = root
            .get("models")
            .and_then(|value| value.get("default"))
            .and_then(TomlValue::as_str)
            .map(str::to_string);

        let provider_table = root.get("model_providers").and_then(TomlValue::as_table);
        if let Some(table) = provider_table {
            for (key, value) in table {
                let base_url = value.get("base_url").and_then(TomlValue::as_str);
                let protocol = value
                    .get("api_backend")
                    .and_then(TomlValue::as_str)
                    .and_then(|backend| GROK_PROTOCOL.parse(backend));
                let entry = if key.starts_with(super::REINS_PREFIX) {
                    let mut entry = super::classify_reins_entry_with(
                        key,
                        base_url,
                        protocol,
                        providers,
                        grok_base_url_matches,
                    );
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

        // 独立 [model.<键>] 条目（官方简化式：模型自带 base_url/api_backend，
        // 不挂 model_provider）不落在 model_providers 里，按模型键展示为外部
        // 配置；挂到某个 Provider 条目下的模型已由该条目代表，不重复列出。
        if let Some(models) = root.get("model").and_then(TomlValue::as_table) {
            for (model_key, value) in models {
                let mounted = value
                    .get("model_provider")
                    .and_then(TomlValue::as_str)
                    .is_some_and(|provider_key| {
                        provider_table.is_some_and(|table| table.contains_key(provider_key))
                    });
                // 键与某个 Provider 条目同名时无法在界面区分，留给该条目。
                let shadowed =
                    provider_table.is_some_and(|table| table.contains_key(model_key.as_str()));
                if model_key.starts_with(super::REINS_PREFIX) || mounted || shadowed {
                    continue;
                }
                let label = value.get("name").and_then(TomlValue::as_str);
                state.entries.push(external_entry(
                    model_key.clone(),
                    value
                        .get("base_url")
                        .and_then(TomlValue::as_str)
                        .map(str::to_string),
                    label
                        .map(|label| vec![format!("外部模型：{label}")])
                        .unwrap_or_default(),
                    value
                        .get("api_backend")
                        .and_then(TomlValue::as_str)
                        .and_then(|backend| GROK_PROTOCOL.parse(backend)),
                ));
            }
        }

        Ok(state)
    }

    fn apply(
        &self,
        env: &Environment,
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
            TomlValue::String(GROK_PROTOCOL.spell(provider.protocol).to_string()),
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
            // 缺 context_window 时 grok 按 200000 兜底并据此计算自动压缩
            // 时机（1.0.41 debug 日志：missing context_window, defaulting to
            // 200000），有元数据就显式写入。
            if let Some(context_window) = model.context_window {
                entry.insert(
                    "context_window".to_string(),
                    TomlValue::Integer(context_window),
                );
            }
            // 未声明支持的模型会被 grok 判为不支持思考等级并丢弃配置档位
            // （1.0.41 实测 WARN：model does not support effort; ignoring it），
            // 官方字段 supports_reasoning_effort 是 [models].default_reasoning_effort
            // 生效的前提。
            if model.reasoning == Some(true) {
                entry.insert(
                    "supports_reasoning_effort".to_string(),
                    TomlValue::Boolean(true),
                );
            }
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
                TomlValue::String(GROK_REASONING.spell(level).to_string()),
            );
        }

        Ok(vec![(path, serialize_toml(&root)?)])
    }

    fn remove(
        &self,
        env: &Environment,
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
            return Err(unrecognized_entry_error("Grok", &registration));
        }
        Ok(vec![(path, serialize_toml(&root)?)])
    }

    fn remove_external(
        &self,
        env: &Environment,
        entry_key: &str,
    ) -> Result<Vec<(PathBuf, String)>> {
        let path = grok_config_path(env)?;
        if !path.exists() {
            bail!("Grok 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_toml(&path)?;
        if !root.is_table() {
            bail!("Grok 配置顶层必须是表：{}", path.display());
        }
        let root_table = root.as_table_mut().expect("checked above");
        // 外部条目键来自两处：model_providers 的 Provider 条目，或 model 表的
        // 独立模型键；前者按 Provider 树删除，后者只删模型键。
        let is_provider_entry = root_table
            .get("model_providers")
            .and_then(TomlValue::as_table)
            .is_some_and(|table| table.contains_key(entry_key));
        let removed = if is_provider_entry {
            remove_provider_tree(root_table, entry_key, false)?
        } else {
            remove_standalone_model(root_table, entry_key)
        };
        if !removed {
            return Err(entry_not_found_error("Grok", entry_key));
        }
        Ok(vec![(path, serialize_toml(&root)?)])
    }
}
