// Codex：$CODEX_HOME/config.toml（默认 ~/.codex/config.toml）。
// 单活动 Provider：应用即替换；仅接受 openai_responses。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use toml::Value as TomlValue;

use super::{
    AppAdapter, AppCapability, ModelField, ToolEnv, classify_reins_entry, codex_config_path,
    display_path, ensure_protocol_supported, external_entry, read_toml, registration_key,
    serialize_toml,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningLevel,
    ResolvedProvider,
};

pub(crate) struct CodexAdapter;

const WIRE_API: &str = "responses";

// 聚合模型多不在 codex 内置目录里，缺该键时 codex 会走兜底元数据
// （context_window = 272000）并告警 Unknown model；有元数据就写默认模型的窗口。
fn default_model_context_window(
    provider: &ResolvedProvider,
    default_model_id: &str,
) -> Option<i64> {
    provider
        .models
        .iter()
        .find(|model| model.id == default_model_id)
        .and_then(|model| model.context_window)
}

// 反读时从 wire_api 字段反推协议；其他取值（手工改成 chat 等）无法确认，留空展示。
fn protocol_from_wire_api(wire_api: Option<&str>) -> Option<ProviderProtocol> {
    match wire_api? {
        WIRE_API => Some(ProviderProtocol::OpenaiResponses),
        _ => None,
    }
}
// 思考等级：官方值域 low/medium/high/xhigh/max/ultra（config-reference，
// 非穷举）。minimal 为 Codex 历史合法档位，保留；off（写入 "none"）
// 无任何文档依据，不放行（用户决策，2026-09-25）。
const SUPPORTED_LEVELS: &[ReasoningLevel] = &[
    ReasoningLevel::Minimal,
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::Xhigh,
    ReasoningLevel::Max,
];

impl AppAdapter for CodexAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Codex
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: &[ProviderProtocol::OpenaiResponses],
            additive: false,
            required_model_fields: &[],
            // 窗口已写 model_context_window；最大输出与图像输入在 codex 配置里
            // 没有模型级字段，推理能力也只有全局默认档、没有 per-model 开关。
            unwritten_model_fields: &[
                ModelField::MaxOutputTokens,
                ModelField::SupportsImages,
                ModelField::Reasoning,
            ],
            supported_reasoning_levels: SUPPORTED_LEVELS,
        }
    }

    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let path = codex_config_path(env)?;
        let mut state = super::empty_state(ProviderAppId::Codex, vec![path.clone()]);
        if !path.exists() {
            return Ok(state);
        }
        state.config_exists = true;
        let root = read_toml(&path)?;
        let model_provider = root
            .get("model_provider")
            .and_then(TomlValue::as_str)
            .map(str::to_string);
        let model = root
            .get("model")
            .and_then(TomlValue::as_str)
            .map(str::to_string);

        if let Some(table) = root.get("model_providers").and_then(TomlValue::as_table) {
            for (key, value) in table {
                let base_url = value.get("base_url").and_then(TomlValue::as_str);
                let protocol =
                    protocol_from_wire_api(value.get("wire_api").and_then(TomlValue::as_str));
                let entry = if key.starts_with(super::REINS_PREFIX) {
                    let mut entry = classify_reins_entry(key, base_url, protocol, providers);
                    let is_default = model_provider.as_deref() == Some(key.as_str());
                    if is_default {
                        entry.default_model_id = model.clone();
                        if let Some(model_id) = &model {
                            entry.model_ids = vec![model_id.clone()];
                        }
                    }
                    if value.get("wire_api").and_then(TomlValue::as_str) != Some(WIRE_API) {
                        entry
                            .notes
                            .push(format!("wire_api 不是 {WIRE_API}，可能已被手工修改。"));
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
        let path = codex_config_path(env)?;
        let mut root = read_toml(&path)?;

        // 替换语义：写入前记录旧的活动 Provider 是否为 Reins 条目，
        // 换平台时把属于旧 Reins 应用的思考等级一并清掉。
        let replacing_reins = root
            .get("model_provider")
            .and_then(TomlValue::as_str)
            .map(|value| value.starts_with(super::REINS_PREFIX))
            .unwrap_or(false);

        let registration = registration_key(&provider.id);

        // Codex 是单活动 Provider：应用新平台时移除旧的、仍可识别的
        // reins- 条目；用户手工改过的漂移配置不自动删。
        {
            let providers_table = root
                .get_mut("model_providers")
                .and_then(|value| value.as_table_mut());
            if let Some(providers_table) = providers_table {
                let stale_keys: Vec<String> = providers_table
                    .iter()
                    .filter(|(key, value)| {
                        key.starts_with(super::REINS_PREFIX)
                            && key.as_str() != registration.as_str()
                            && value.get("base_url").and_then(TomlValue::as_str).is_some()
                            && value.get("wire_api").and_then(TomlValue::as_str) == Some(WIRE_API)
                    })
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in stale_keys {
                    providers_table.remove(&key);
                }
            }
        }
        let provider_table = {
            let mut table = toml::map::Map::new();
            table.insert(
                "name".to_string(),
                TomlValue::String(provider.label.clone()),
            );
            table.insert(
                "base_url".to_string(),
                TomlValue::String(provider.base_url.clone()),
            );
            table.insert(
                "wire_api".to_string(),
                TomlValue::String(WIRE_API.to_string()),
            );
            table.insert(
                "experimental_bearer_token".to_string(),
                TomlValue::String(api_key.to_string()),
            );
            TomlValue::Table(table)
        };

        if !root.is_table() {
            bail!("Codex 配置顶层必须是表：{}", path.display());
        }
        let root_table = root.as_table_mut().expect("checked above");
        let providers_table = root_table
            .entry("model_providers")
            .or_insert_with(|| TomlValue::Table(toml::map::Map::new()));
        if !providers_table.is_table() {
            bail!("model_providers 段不是表：{}", path.display());
        }
        providers_table
            .as_table_mut()
            .expect("checked above")
            .insert(registration.clone(), provider_table);

        root_table.insert(
            "model_provider".to_string(),
            TomlValue::String(registration.clone()),
        );
        root_table.insert(
            "model".to_string(),
            TomlValue::String(plan.default_model_id.clone()),
        );
        match default_model_context_window(provider, &plan.default_model_id) {
            Some(context_window) => {
                root_table.insert(
                    "model_context_window".to_string(),
                    TomlValue::Integer(context_window),
                );
            }
            None => {
                // 元数据被清空后不再写；只有本次是替换 Reins 自己的配置时才
                // 清掉旧值，避免动用户自设的窗口。
                if replacing_reins {
                    root_table.remove("model_context_window");
                }
            }
        }
        match plan.default_reasoning_level {
            Some(level) => {
                root_table.insert(
                    "model_reasoning_effort".to_string(),
                    TomlValue::String(level.as_str().to_string()),
                );
            }
            None => {
                if replacing_reins {
                    root_table.remove("model_reasoning_effort");
                }
            }
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
        let path = codex_config_path(env)?;
        if !path.exists() {
            bail!("Codex 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_toml(&path)?;
        let registration = registration_key(provider_id);

        if !root.is_table() {
            bail!("Codex 配置顶层必须是表：{}", path.display());
        }
        let root_table = root.as_table_mut().expect("checked above");

        let mut recognizable = false;
        if let Some(providers_table) = root_table
            .get_mut("model_providers")
            .and_then(|value| value.as_table_mut())
        {
            if let Some(entry) = providers_table.get(&registration) {
                // 只删内容仍可识别的条目：必须带 base_url 且 wire_api 是
                // responses；用户改过的漂移配置不自动删。
                let recognizable_now = entry.get("base_url").and_then(TomlValue::as_str).is_some()
                    && entry.get("wire_api").and_then(TomlValue::as_str) == Some(WIRE_API);
                if !recognizable_now {
                    bail!("条目 {registration} 已被手工修改，无法自动识别，请手动处理。");
                }
                recognizable = true;
                providers_table.remove(&registration);
            }
        }
        if !recognizable {
            bail!("Codex 中没有可识别的 {} 条目。", registration);
        }

        if root_table.get("model_provider").and_then(TomlValue::as_str)
            == Some(registration.as_str())
        {
            root_table.remove("model_provider");
            root_table.remove("model_reasoning_effort");
            // model 只有在仍指向该平台的模型目录时才随移除清掉。
            if let Some(provider) = provider {
                let model_matches = root_table
                    .get("model")
                    .and_then(TomlValue::as_str)
                    .map(|model| provider.models.iter().any(|m| m.id == model))
                    .unwrap_or(false);
                // 窗口值只有仍等于该模型元数据时才清，避免误删用户自设值。
                let metadata_window = root_table
                    .get("model")
                    .and_then(TomlValue::as_str)
                    .and_then(|model| provider.models.iter().find(|m| m.id == model))
                    .and_then(|model| model.context_window);
                if metadata_window.is_some()
                    && root_table
                        .get("model_context_window")
                        .and_then(TomlValue::as_integer)
                        == metadata_window
                {
                    root_table.remove("model_context_window");
                }
                if model_matches {
                    root_table.remove("model");
                }
            }
        }

        Ok(vec![(path, serialize_toml(&root)?)])
    }

    fn remove_external(&self, env: &ToolEnv, entry_key: &str) -> Result<Vec<(PathBuf, String)>> {
        let path = codex_config_path(env)?;
        if !path.exists() {
            bail!("Codex 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_toml(&path)?;
        if !root.is_table() {
            bail!("Codex 配置顶层必须是表：{}", path.display());
        }
        let root_table = root.as_table_mut().expect("checked above");

        // 与 reins- 条目移除一致：活动 Provider 指向被删条目时随删除清空，
        // 不再要求先切换。model 是裸模型名，不属于特定条目，保持不动。
        if root_table.get("model_provider").and_then(TomlValue::as_str) == Some(entry_key) {
            root_table.remove("model_provider");
            root_table.remove("model_reasoning_effort");
        }

        let Some(providers_table) = root_table
            .get_mut("model_providers")
            .and_then(|value| value.as_table_mut())
        else {
            bail!("Codex 中没有外部条目 {entry_key}。");
        };
        if providers_table.remove(entry_key).is_none() {
            bail!("Codex 中没有外部条目 {entry_key}。");
        }

        Ok(vec![(path, serialize_toml(&root)?)])
    }
}
