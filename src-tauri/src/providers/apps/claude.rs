// Claude Code：~/.claude/settings.json。单活动 Provider：应用即替换；
// 仅接受 anthropic_messages。写入覆盖 env.ANTHROPIC_BASE_URL /
// env.ANTHROPIC_API_KEY / model / effortLevel，保留文件中其他键。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::{
    AppAdapter, AppCapability, ModelField, ToolEnv, claude_settings_path, display_path,
    ensure_protocol_supported, external_entry, read_json_object, serialize_json,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningLevel,
    ResolvedProvider,
};

pub(crate) struct ClaudeAdapter;

const BASE_URL_KEY: &str = "ANTHROPIC_BASE_URL";
const API_KEY_ENV_KEY: &str = "ANTHROPIC_API_KEY";
// 聚合模型对 Claude Code 是「不认识的模型 ID」：窗口按内置同名 ID 推断、
// 输出默认 32000（code.claude.com/docs/en/env-vars）。这两个 env 键正是
// 官方为此场景提供的纠正入口，随活动 provider 由 Reins 管理。
const MAX_CONTEXT_TOKENS_KEY: &str = "CLAUDE_CODE_MAX_CONTEXT_TOKENS";
const MAX_OUTPUT_TOKENS_KEY: &str = "CLAUDE_CODE_MAX_OUTPUT_TOKENS";

// 官方 effortLevel 值域为 low/medium/high/xhigh/max（code.claude.com/docs/en/settings-reference），
// off/minimal 不在枚举内，不放行避免写出无效值。
const SUPPORTED_LEVELS: &[ReasoningLevel] = &[
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::Xhigh,
    ReasoningLevel::Max,
];

impl AppAdapter for ClaudeAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Claude
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: &[ProviderProtocol::AnthropicMessages],
            additive: false,
            required_model_fields: &[],
            // 窗口与最大输出已写 env；图像输入与 per-model 推理能力在
            // Claude Code 配置里没有对应键。
            unwritten_model_fields: &[ModelField::SupportsImages, ModelField::Reasoning],
            supported_reasoning_levels: SUPPORTED_LEVELS,
        }
    }

    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let path = claude_settings_path(env)?;
        let mut state = super::empty_state(ProviderAppId::Claude, vec![path.clone()]);
        if !path.exists() {
            return Ok(state);
        }
        state.config_exists = true;
        let root = read_json_object(&path)?;
        // 应用弹窗据此预选；反读值只用于回显，不参与漂移判定。
        state.default_reasoning_level = root
            .get("effortLevel")
            .and_then(JsonValue::as_str)
            .and_then(ReasoningLevel::parse);
        let Some(env_table) = root.get("env").and_then(JsonValue::as_object) else {
            return Ok(state);
        };
        let Some(base_url) = env_table.get(BASE_URL_KEY).and_then(JsonValue::as_str) else {
            return Ok(state);
        };

        // Claude 没有 reins- 注册键可认，只能靠 base_url 归属：
        // 与某平台一致 → 已应用；不一致 → 外部配置（只展示不纳管）。
        let matched = providers
            .values()
            .find(|provider| provider.base_url == base_url);
        let model = root
            .get("model")
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        let entry = if let Some(provider) = matched {
            let mut entry = super::applied_entry(
                "env".to_string(),
                provider,
                Some(base_url.to_string()),
                model.clone().map(|m| vec![m]).unwrap_or_default(),
                model,
                // Claude 的 env 写法语义固定为 Anthropic Messages，无字段可反推。
                Some(ProviderProtocol::AnthropicMessages),
            );
            if env_table.get(API_KEY_ENV_KEY).is_none() {
                entry
                    .notes
                    .push(format!("缺少 {API_KEY_ENV_KEY}，应用后才能生效。"));
            }
            entry
        } else {
            external_entry(
                "env".to_string(),
                Some(base_url.to_string()),
                vec!["外部 ANTHROPIC_BASE_URL，非 Reins 纳管。".to_string()],
                Some(ProviderProtocol::AnthropicMessages),
            )
        };
        state.entries.push(entry);
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
        let path = claude_settings_path(env)?;
        let mut root = read_json_object(&path)?;

        let env_table = root
            .entry("env")
            .or_insert_with(|| JsonValue::Object(JsonMap::new()));
        let Some(env_table) = env_table.as_object_mut() else {
            bail!("env 段不是对象：{}", path.display());
        };
        env_table.insert(
            BASE_URL_KEY.to_string(),
            JsonValue::String(provider.base_url.clone()),
        );
        env_table.insert(
            API_KEY_ENV_KEY.to_string(),
            JsonValue::String(api_key.to_string()),
        );
        // 窗口与输出上限取自默认模型元数据；缺失时清空，避免上一个 provider
        // 的值残留在唯一的活动配置上。
        let default_model = provider
            .models
            .iter()
            .find(|model| model.id == plan.default_model_id);
        for (key, value) in [
            (
                MAX_CONTEXT_TOKENS_KEY,
                default_model.and_then(|model| model.context_window),
            ),
            (
                MAX_OUTPUT_TOKENS_KEY,
                default_model.and_then(|model| model.max_output_tokens),
            ),
        ] {
            match value {
                Some(value) => {
                    env_table.insert(key.to_string(), JsonValue::String(value.to_string()));
                }
                None => {
                    env_table.remove(key);
                }
            }
        }
        root.insert(
            "model".to_string(),
            JsonValue::String(plan.default_model_id.clone()),
        );
        if let Some(level) = plan.default_reasoning_level {
            root.insert(
                "effortLevel".to_string(),
                JsonValue::String(level.as_str().to_string()),
            );
        }

        Ok(vec![(path, serialize_json(&root)?)])
    }

    fn remove(
        &self,
        env: &ToolEnv,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>> {
        // 没有 base_url 比对就无法证明这条配置属于该平台，宁可拒绝也
        // 不能误删用户自己的官方配置。
        let Some(provider) = provider else {
            bail!(
                "平台 {provider_id} 的元数据已删除，无法安全移除 Claude Code 配置；请手动清理 settings.json 中的 {BASE_URL_KEY} 与 {API_KEY_ENV_KEY}。"
            );
        };
        let path = claude_settings_path(env)?;
        if !path.exists() {
            bail!("Claude Code 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_json_object(&path)?;

        let current_base_url = root
            .get("env")
            .and_then(|value| value.get(BASE_URL_KEY))
            .and_then(JsonValue::as_str);
        if current_base_url != Some(provider.base_url.as_str()) {
            bail!("settings.json 的 {BASE_URL_KEY} 与平台 {provider_id} 不一致，未做任何修改。");
        }

        if let Some(env_table) = root.get_mut("env").and_then(JsonValue::as_object_mut) {
            env_table.remove(BASE_URL_KEY);
            env_table.remove(API_KEY_ENV_KEY);
            env_table.remove(MAX_CONTEXT_TOKENS_KEY);
            env_table.remove(MAX_OUTPUT_TOKENS_KEY);
            if env_table.is_empty() {
                root.remove("env");
            }
        }
        // model 与 effortLevel 只有与平台写入值一致才清，避免误删
        // 用户自己设置的值。
        let model_is_ours = root
            .get("model")
            .and_then(JsonValue::as_str)
            .map(|model| provider.models.iter().any(|m| m.id == model))
            .unwrap_or(false);
        if model_is_ours {
            root.remove("model");
            if let Some(level) = root.get("effortLevel").and_then(JsonValue::as_str) {
                if ReasoningLevel::parse(level).is_some() {
                    root.remove("effortLevel");
                }
            }
        }

        Ok(vec![(path, serialize_json(&root)?)])
    }

    fn remove_external(&self, _env: &ToolEnv, _entry_key: &str) -> Result<Vec<(PathBuf, String)>> {
        // Claude 的外部配置是 env 内嵌值，没有可结构化删除的独立条目。
        bail!("Claude 的外部配置不支持按条目删除。");
    }
}
