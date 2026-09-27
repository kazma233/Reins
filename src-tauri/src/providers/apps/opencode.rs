// OpenCode v2：~/.config/opencode/opencode.json。多 Provider 并存：新增
// reins- 条目，不动用户已有 Provider。
//
// 写入一律用 v2 规范 schema（providers 节点 + package + settings，见
// opencode.ai/v2/docs/providers）；v2 运行时同时兼容 v1 遗留 schema
// （provider 节点 + npm + options），用户手工配置或旧版 Reins 写入的
// 条目可能留在 v1 节点，因此读取与删除两个节点都支持，但不向 v1 写入。
// openai_responses：2026-09-25 在 v2.0.16 实测 responses 包静默加载失败而
// 关闭；2026-09-27 在 v2.0.18 复测，openai/responses 本地监听端到端可用，
// 恢复放行；openai-compatible/responses 同版初始化即报
// Cannot find package '@opencode/ai'，不采用。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::{
    AppAdapter, AppCapability, ToolEnv, display_path, ensure_protocol_supported, external_entry,
    opencode_config_path, read_json_object, registration_key, serialize_json,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppEntry, ProviderAppId, ProviderAppState, ProviderProtocol,
    ReasoningLevel, ResolvedProvider,
};

pub(crate) struct OpencodeAdapter;

// v2 原生运行时包：OpenAI 请求格式的端点走 openai-compatible；Responses
// 协议走 openai/responses（compatible/responses 有包解析缺陷，见文件头）；
// Anthropic 协议端点走 anthropic（官方文档的端点代理示例即该包 +
// settings.baseURL）；anthropic-compatible 面向第三方兼容端点，仅用于反读。
const PKG_OPENAI_COMPATIBLE: &str = "@opencode/ai/providers/openai-compatible";
const PKG_OPENAI_RESPONSES: &str = "@opencode/ai/providers/openai/responses";
const PKG_OPENAI_COMPATIBLE_RESPONSES: &str = "@opencode/ai/providers/openai-compatible/responses";
const PKG_ANTHROPIC: &str = "@opencode/ai/providers/anthropic";
const PKG_ANTHROPIC_COMPATIBLE: &str = "@opencode/ai/providers/anthropic-compatible";

// v1 遗留 npm 包名，仅用于反读识别。
const NPM_OPENAI_COMPATIBLE: &str = "@ai-sdk/openai-compatible";
const NPM_ANTHROPIC: &str = "@ai-sdk/anthropic";

// apply 时按协议选择 v2 原生运行时包，与反读的 protocol_from_package 互为映射。
fn runtime_package(protocol: ProviderProtocol) -> &'static str {
    match protocol {
        ProviderProtocol::OpenaiChatCompletions => PKG_OPENAI_COMPATIBLE,
        ProviderProtocol::OpenaiResponses => PKG_OPENAI_RESPONSES,
        ProviderProtocol::AnthropicMessages => PKG_ANTHROPIC,
    }
}

// 反读时从运行时包反推协议；覆盖 v2 原生包名与 v1 遗留 npm 包名，
// 其余包协议不确定，留空展示。
fn protocol_from_package(package: Option<&str>) -> Option<ProviderProtocol> {
    match package? {
        PKG_OPENAI_COMPATIBLE | NPM_OPENAI_COMPATIBLE => {
            Some(ProviderProtocol::OpenaiChatCompletions)
        }
        PKG_OPENAI_RESPONSES | PKG_OPENAI_COMPATIBLE_RESPONSES => {
            Some(ProviderProtocol::OpenaiResponses)
        }
        PKG_ANTHROPIC | PKG_ANTHROPIC_COMPATIBLE | NPM_ANTHROPIC => {
            Some(ProviderProtocol::AnthropicMessages)
        }
        _ => None,
    }
}

// v2 文档未确认思考等级的写入方式，apply 也不消费该值；
// 能力表置空让应用弹窗不出现思考等级选项，避免可选不可写。
const SUPPORTED_LEVELS: &[ReasoningLevel] = &[];

fn entry_from_provider_value(
    key: &str,
    base_url: Option<&str>,
    protocol: Option<ProviderProtocol>,
    models: Option<&JsonMap<String, JsonValue>>,
    default_model: Option<&str>,
    providers: &BTreeMap<String, ResolvedProvider>,
) -> ProviderAppEntry {
    if key.starts_with(super::REINS_PREFIX) {
        let mut entry = super::classify_reins_entry(key, base_url, protocol, providers);
        if let Some(models) = models {
            entry.model_ids = models.keys().cloned().collect();
        }
        if let Some(default_model) = default_model {
            let prefix = format!("{key}/");
            if let Some(model_id) = default_model.strip_prefix(&prefix) {
                entry.default_model_id = Some(model_id.to_string());
            }
        }
        entry
    } else {
        external_entry(
            key.to_string(),
            base_url.map(str::to_string),
            vec!["外部 Provider。".to_string()],
            protocol,
        )
    }
}

impl AppAdapter for OpencodeAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Opencode
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: &[
                ProviderProtocol::OpenaiChatCompletions,
                ProviderProtocol::OpenaiResponses,
                ProviderProtocol::AnthropicMessages,
            ],
            additive: true,
            required_model_fields: &[],
            supported_reasoning_levels: SUPPORTED_LEVELS,
        }
    }

    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let path = opencode_config_path(env)?;
        let mut state = super::empty_state(ProviderAppId::Opencode, vec![path.clone()]);
        if !path.exists() {
            return Ok(state);
        }
        state.config_exists = true;
        let root = read_json_object(&path)?;
        let default_model = root.get("model").and_then(JsonValue::as_str);

        // v2 规范节点 providers；v1 遗留节点 provider。两个节点都读。
        for (node, is_v2) in [("providers", true), ("provider", false)] {
            let Some(table) = root.get(node).and_then(JsonValue::as_object) else {
                continue;
            };
            for (key, value) in table {
                let (base_url, package, models) = if is_v2 {
                    (
                        value
                            .get("settings")
                            .and_then(|settings| settings.get("baseURL"))
                            .and_then(JsonValue::as_str),
                        value.get("package").and_then(JsonValue::as_str),
                        value.get("models").and_then(JsonValue::as_object),
                    )
                } else {
                    (
                        value
                            .get("options")
                            .and_then(|options| options.get("baseURL"))
                            .and_then(JsonValue::as_str),
                        value.get("npm").and_then(JsonValue::as_str),
                        value.get("models").and_then(JsonValue::as_object),
                    )
                };
                let protocol = protocol_from_package(package);
                let entry = entry_from_provider_value(
                    key,
                    base_url,
                    protocol,
                    models,
                    default_model,
                    providers,
                );
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
        let path = opencode_config_path(env)?;
        let mut root = read_json_object(&path)?;
        let registration = registration_key(&provider.id);

        let mut models = JsonMap::new();
        for model_id in &plan.model_ids {
            let label = provider
                .models
                .iter()
                .find(|m| &m.id == model_id)
                .map(|m| m.label.clone())
                .unwrap_or_else(|| model_id.clone());
            let mut model_entry = JsonMap::new();
            model_entry.insert("name".to_string(), JsonValue::String(label));
            models.insert(model_id.clone(), JsonValue::Object(model_entry));
        }

        let mut settings = JsonMap::new();
        settings.insert(
            "baseURL".to_string(),
            JsonValue::String(provider.base_url.clone()),
        );
        settings.insert("apiKey".to_string(), JsonValue::String(api_key.to_string()));

        let mut provider_entry = JsonMap::new();
        provider_entry.insert(
            "name".to_string(),
            JsonValue::String(provider.label.clone()),
        );
        provider_entry.insert(
            "package".to_string(),
            JsonValue::String(runtime_package(provider.protocol).to_string()),
        );
        provider_entry.insert("settings".to_string(), JsonValue::Object(settings));
        provider_entry.insert("models".to_string(), JsonValue::Object(models));

        if !root
            .entry("providers")
            .or_insert_with(|| JsonValue::Object(JsonMap::new()))
            .is_object()
        {
            bail!("providers 段不是对象：{}", path.display());
        }
        root.get_mut("providers")
            .and_then(JsonValue::as_object_mut)
            .expect("checked above")
            .insert(registration.clone(), JsonValue::Object(provider_entry));
        root.insert(
            "model".to_string(),
            JsonValue::String(format!("{registration}/{}", plan.default_model_id)),
        );

        Ok(vec![(path, serialize_json(&root)?)])
    }

    fn remove(
        &self,
        env: &ToolEnv,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>> {
        let _ = provider;
        let path = opencode_config_path(env)?;
        if !path.exists() {
            bail!("OpenCode 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_json_object(&path)?;
        let registration = registration_key(provider_id);

        // 默认模型仍指向该平台时随移除清掉；model 是顶层单值键，
        // 留空后由用户在 OpenCode 内重选，不再要求先切换。
        let default_refs_registration = root
            .get("model")
            .and_then(JsonValue::as_str)
            .is_some_and(|model| model.starts_with(&format!("{registration}/")));
        if default_refs_registration {
            root.remove("model");
        }

        // 本版 Reins 写 v2 providers 节点；旧版 Reins 写 v1 provider 节点。
        for node in ["providers", "provider"] {
            let Some(table) = root.get_mut(node).and_then(JsonValue::as_object_mut) else {
                continue;
            };
            let Some(entry) = table.get(&registration) else {
                continue;
            };
            // 只删内容仍可识别的条目：baseURL 必须是字符串。
            let recognized = ["settings", "options"].iter().any(|section| {
                entry
                    .get(section)
                    .and_then(|settings| settings.get("baseURL"))
                    .and_then(JsonValue::as_str)
                    .is_some()
            });
            if !recognized {
                bail!("条目 {registration} 已被手工修改，无法自动识别，请手动处理。");
            }
            table.remove(&registration);
            if table.is_empty() {
                root.remove(node);
            }
            return Ok(vec![(path, serialize_json(&root)?)]);
        }
        bail!("OpenCode 中没有可识别的 {registration} 条目。");
    }

    fn remove_external(&self, env: &ToolEnv, entry_key: &str) -> Result<Vec<(PathBuf, String)>> {
        let path = opencode_config_path(env)?;
        if !path.exists() {
            bail!("OpenCode 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_json_object(&path)?;

        // 与 reins- 条目移除一致：默认模型指向被删条目时随删除清空。
        let default_refs_entry = root
            .get("model")
            .and_then(JsonValue::as_str)
            .is_some_and(|model| model.starts_with(&format!("{entry_key}/")));
        if default_refs_entry {
            root.remove("model");
        }

        for node in ["providers", "provider"] {
            let Some(table) = root.get_mut(node).and_then(JsonValue::as_object_mut) else {
                continue;
            };
            if table.remove(entry_key).is_some() {
                if table.is_empty() {
                    root.remove(node);
                }
                return Ok(vec![(path, serialize_json(&root)?)]);
            }
        }
        bail!("OpenCode 中没有外部条目 {entry_key}。");
    }
}
