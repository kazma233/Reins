// Pi：$PI_CODING_AGENT_DIR/models.json + settings.json（默认 ~/.pi/agent）。
// 多 Provider 并存：新增 reins- 条目；默认值写 settings.json。
// models.json 的 api 取值已确认：KnownApi 含 openai-responses/
// openai-completions/anthropic-messages（pi-ai 包 types.d.ts）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::{
    AppAdapter, AppCapability, ToolEnv, display_path, ensure_protocol_supported, external_entry,
    pi_models_path, pi_settings_path, read_json_object, registration_key, serialize_json,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningLevel,
    ResolvedProvider,
};

pub(crate) struct PiAdapter;

// 各协议对应的 api 取值，按 pi 命名习惯写为 kebab-case；待实现期验证。
fn api_name(protocol: ProviderProtocol) -> &'static str {
    match protocol {
        ProviderProtocol::OpenaiResponses => "openai-responses",
        ProviderProtocol::OpenaiChatCompletions => "openai-completions",
        ProviderProtocol::AnthropicMessages => "anthropic-messages",
    }
}

// 反读时从 models.json 的 api 字段反推协议；未知取值留空展示。
fn protocol_from_api(api: Option<&str>) -> Option<ProviderProtocol> {
    match api? {
        "openai-responses" => Some(ProviderProtocol::OpenaiResponses),
        "openai-completions" => Some(ProviderProtocol::OpenaiChatCompletions),
        "anthropic-messages" => Some(ProviderProtocol::AnthropicMessages),
        _ => None,
    }
}

// Pi 的思考等级枚举首档是 "off"（pi.dev/docs/latest/settings），
// 与归一化集合的 "none" 写法不同，单独映射。
fn thinking_level(level: ReasoningLevel) -> &'static str {
    match level {
        ReasoningLevel::Off => "off",
        _ => level.as_str(),
    }
}

// 反读 settings.json 里的 defaultThinkingLevel：首档同样是 "off"。
fn thinking_level_from_str(value: &str) -> Option<ReasoningLevel> {
    if value.trim().eq_ignore_ascii_case("off") {
        return Some(ReasoningLevel::Off);
    }
    ReasoningLevel::parse(value)
}

// Pi 的思考等级选择器只对 thinkingLevelMap 里显式声明的模型开放
// xhigh/max 档（pi getSupportedThinkingLevels：值 null 禁用该档，
// xhigh/max 缺键即不可选），因此模型的 reasoning_levels 必须逐档下发，
// 否则 Reins 应用的推理模型在 Pi 里最高只能选到 high。键是 Pi 档位名，
// 值是该档实际发给 API 的 effort（off 档为 none，Pi 各协议的缺省值）。
fn thinking_level_map(levels: &[ReasoningLevel]) -> JsonValue {
    let mut map = JsonMap::new();
    for level in ReasoningLevel::all() {
        let value = if levels.contains(level) {
            JsonValue::String(level.as_str().to_string())
        } else {
            JsonValue::Null
        };
        map.insert(thinking_level(*level).to_string(), value);
    }
    JsonValue::Object(map)
}

impl AppAdapter for PiAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Pi
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: ProviderProtocol::all(),
            additive: true,
            required_model_fields: &[],
            // Pi 的 models.json 逐模型写 reasoning/contextWindow/maxTokens/input。
            unwritten_model_fields: &[],
            supported_reasoning_levels: ReasoningLevel::all(),
        }
    }

    fn reasoning_effort_write(&self, level: ReasoningLevel) -> String {
        thinking_level(level).to_string()
    }

    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let models_path = pi_models_path(env)?;
        let settings_path = pi_settings_path(env)?;
        let mut state = super::empty_state(
            ProviderAppId::Pi,
            vec![models_path.clone(), settings_path.clone()],
        );
        if !models_path.exists() && !settings_path.exists() {
            return Ok(state);
        }
        state.config_exists = true;
        let settings = read_json_object(&settings_path)?;
        // 应用弹窗据此预选；反读值只用于回显，不参与漂移判定。
        state.default_reasoning_level = settings
            .get("defaultThinkingLevel")
            .and_then(JsonValue::as_str)
            .and_then(thinking_level_from_str);
        let default_provider = settings
            .get("defaultProvider")
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        let default_model = settings
            .get("defaultModel")
            .and_then(JsonValue::as_str)
            .map(str::to_string);

        let root = read_json_object(&models_path)?;
        if let Some(table) = root.get("providers").and_then(JsonValue::as_object) {
            for (key, value) in table {
                let base_url = value.get("baseUrl").and_then(JsonValue::as_str);
                let protocol = protocol_from_api(value.get("api").and_then(JsonValue::as_str));
                let entry = if key.starts_with(super::REINS_PREFIX) {
                    let mut entry = super::classify_reins_entry(key, base_url, protocol, providers);
                    if let Some(models) = value.get("models").and_then(JsonValue::as_array) {
                        entry.model_ids = models
                            .iter()
                            .filter_map(|model| model.get("id").and_then(JsonValue::as_str))
                            .map(str::to_string)
                            .collect();
                    }
                    if default_provider.as_deref() == Some(key.as_str()) {
                        entry.default_model_id = default_model.clone();
                    }
                    entry
                } else {
                    external_entry(
                        key.clone(),
                        base_url.map(str::to_string),
                        vec!["外部 Provider。".to_string()],
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
        let models_path = pi_models_path(env)?;
        let settings_path = pi_settings_path(env)?;
        let registration = registration_key(&provider.id);

        let mut root = read_json_object(&models_path)?;
        // Pi 的 models.json schema 要求 models 是数组，元素带 id 字段
        //（真实文件验证：id/name/reasoning/contextWindow/maxTokens/input）。
        let mut models = Vec::new();
        for model_id in &plan.model_ids {
            let model = provider
                .models
                .iter()
                .find(|m| &m.id == model_id)
                .expect("validated by validate_plan");
            let mut model_entry = serde_json::Map::new();
            model_entry.insert("id".to_string(), JsonValue::String(model.id.clone()));
            model_entry.insert("name".to_string(), JsonValue::String(model.label.clone()));
            if let Some(reasoning) = model.reasoning {
                model_entry.insert("reasoning".to_string(), JsonValue::Bool(reasoning));
            }
            // 未填等级时不写 map：Pi 对无 map 的模型默认开放 off～high，
            // 与空档位集的语义一致；写全 null 反而连 off 档都会消失。
            if let Some(levels) = model
                .reasoning_levels
                .as_deref()
                .filter(|levels| !levels.is_empty())
            {
                model_entry.insert("thinkingLevelMap".to_string(), thinking_level_map(levels));
            }
            if let Some(context_window) = model.context_window {
                model_entry.insert("contextWindow".to_string(), JsonValue::from(context_window));
            }
            if let Some(max_tokens) = model.max_output_tokens {
                model_entry.insert("maxTokens".to_string(), JsonValue::from(max_tokens));
            }
            if let Some(supports_images) = model.supports_images {
                let input = if supports_images {
                    vec![
                        JsonValue::String("text".to_string()),
                        JsonValue::String("image".to_string()),
                    ]
                } else {
                    vec![JsonValue::String("text".to_string())]
                };
                model_entry.insert("input".to_string(), JsonValue::Array(input));
            }
            models.push(JsonValue::Object(model_entry));
        }

        let mut provider_entry = JsonMap::new();
        provider_entry.insert(
            "baseUrl".to_string(),
            JsonValue::String(provider.base_url.clone()),
        );
        provider_entry.insert(
            "api".to_string(),
            JsonValue::String(api_name(provider.protocol).to_string()),
        );
        provider_entry.insert("apiKey".to_string(), JsonValue::String(api_key.to_string()));
        provider_entry.insert("models".to_string(), JsonValue::Array(models));

        if !root
            .entry("providers")
            .or_insert_with(|| JsonValue::Object(JsonMap::new()))
            .is_object()
        {
            bail!("providers 段不是对象：{}", models_path.display());
        }
        root.get_mut("providers")
            .and_then(JsonValue::as_object_mut)
            .expect("checked above")
            .insert(registration.clone(), JsonValue::Object(provider_entry));

        let mut settings = read_json_object(&settings_path)?;
        settings.insert(
            "defaultProvider".to_string(),
            JsonValue::String(registration),
        );
        settings.insert(
            "defaultModel".to_string(),
            JsonValue::String(plan.default_model_id.clone()),
        );
        if let Some(level) = plan.default_reasoning_level {
            settings.insert(
                "defaultThinkingLevel".to_string(),
                JsonValue::String(thinking_level(level).to_string()),
            );
        }

        Ok(vec![
            (models_path, serialize_json(&root)?),
            (settings_path, serialize_json(&settings)?),
        ])
    }

    fn remove(
        &self,
        env: &ToolEnv,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>> {
        let _ = provider;
        let models_path = pi_models_path(env)?;
        let settings_path = pi_settings_path(env)?;
        if !models_path.exists() {
            bail!("Pi models.json 不存在：{}", display_path(&models_path));
        }
        let mut root = read_json_object(&models_path)?;
        let registration = registration_key(provider_id);

        let mut settings = read_json_object(&settings_path)?;
        // 默认 Provider 仍指向该平台时随移除清掉 apply 写入的默认三元组；
        // 只有 settings 真正被修改时才写回，避免凭空创建 settings.json。
        let mut files: Vec<(PathBuf, String)> = Vec::new();
        if settings.get("defaultProvider").and_then(JsonValue::as_str)
            == Some(registration.as_str())
        {
            settings.remove("defaultProvider");
            settings.remove("defaultModel");
            settings.remove("defaultThinkingLevel");
            files.push((settings_path, serialize_json(&settings)?));
        }

        let Some(providers_table) = root.get_mut("providers").and_then(JsonValue::as_object_mut)
        else {
            bail!("Pi 中没有可识别的 {registration} 条目。");
        };
        let entry = providers_table.get(&registration);
        let Some(entry) = entry else {
            bail!("Pi 中没有可识别的 {registration} 条目。");
        };
        if entry.get("baseUrl").and_then(JsonValue::as_str).is_none() {
            bail!("条目 {registration} 已被手工修改，无法自动识别，请手动处理。");
        }
        providers_table.remove(&registration);
        if providers_table.is_empty() {
            root.remove("providers");
        }

        files.push((models_path, serialize_json(&root)?));
        Ok(files)
    }

    fn remove_external(&self, env: &ToolEnv, entry_key: &str) -> Result<Vec<(PathBuf, String)>> {
        let models_path = pi_models_path(env)?;
        let settings_path = pi_settings_path(env)?;
        if !models_path.exists() {
            bail!("Pi models.json 不存在：{}", display_path(&models_path));
        }
        let mut root = read_json_object(&models_path)?;

        let mut settings = read_json_object(&settings_path)?;
        let mut files: Vec<(PathBuf, String)> = Vec::new();
        // 与 reins- 条目移除一致：默认 Provider 指向被删条目时随删除清空；
        // 只有 settings 真正被修改时才写回。
        if settings.get("defaultProvider").and_then(JsonValue::as_str) == Some(entry_key) {
            settings.remove("defaultProvider");
            settings.remove("defaultModel");
            settings.remove("defaultThinkingLevel");
            files.push((settings_path, serialize_json(&settings)?));
        }

        let Some(providers_table) = root.get_mut("providers").and_then(JsonValue::as_object_mut)
        else {
            bail!("Pi 中没有外部条目 {entry_key}。");
        };
        if providers_table.remove(entry_key).is_none() {
            bail!("Pi 中没有外部条目 {entry_key}。");
        }
        if providers_table.is_empty() {
            root.remove("providers");
        }

        files.push((models_path, serialize_json(&root)?));
        Ok(files)
    }
}
