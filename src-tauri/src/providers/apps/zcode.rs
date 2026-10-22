// ZCode：~/.zcode/v2/provider_config.json（v2 布局，无官方重定向变量）。
// 多 Provider 并存：providerRules 数组内 reins- 条目与用户自配条目互不
// 影响；官方 OAuth 登录不经该文件（凭据在 ~/.zcode/v2/credentials.json，
// 属 ZCode 自管的敏感存储，不读写）。
// 格式实证（ZCode.app bundle 的 zod schema + 本机文件，2026-10-13）：
// - api.type ∈ anthropic-messages / openai-chat-completions / openai-responses；
// - group ∈ standard-personal（自配）/ zai-family / bigmodel-family（官方
//   家族），Reins 只写 standard-personal；
// - 模型目录 = personalModelIds ∪ 模板内置（模板平台才有）；模型元数据落
//   modelConfigRules.providerModelRules 的 config（enabled /
//   properties.contextWindow / properties.inputFormat.supportsImage /
//   optionSpecs.maxOutputTokens.max）；optionSpecs.reasoningLevel 的内部
//   结构未实证，思考档位与默认模型（ZCode 的模型选择是会话级，不落盘）
//   均不写。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use serde_json::{Map as JsonMap, Value as JsonValue, json};

use super::dialect::ZCODE_PROTOCOL;
use super::{
    AppAdapter, AppCapability, Environment, classify_reins_entry, display_path,
    ensure_protocol_supported, entry_not_found_error, external_entry, modified_entry_error,
    read_json_object, registration_key, serialize_json, unrecognized_entry_error,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppId, ProviderAppState, ProviderModelRecord, ProviderProtocol,
    ReasoningLevel, ResolvedProvider,
};

pub(crate) struct ZcodeAdapter;

// provider 配置在 v2 布局（~/.zcode/v2/），与清单登记的 MCP 配置（cli/）互
// 不隶属；根从清单的 GlobalRoot 解析，目录段在此声明。
pub(crate) fn provider_config_path(env: &Environment) -> Result<PathBuf> {
    let spec = crate::agents::spec_by_provider_app(ProviderAppId::Zcode)
        .ok_or_else(|| anyhow!("zcode 未登记 agents 清单。"))?;
    spec.global
        .root
        .resolve(env)
        .map(|root| root.join(".zcode/v2/provider_config.json"))
        .ok_or_else(|| anyhow!("无法解析 zcode 主目录。"))
}

// 深取数组段：缺失时就地建空数组容器，损坏类型报错不吞。
fn array_section_mut<'a>(
    root: &'a mut JsonMap<String, JsonValue>,
    section_names: [&'static str; 2],
    list_name: &str,
) -> Result<&'a mut Vec<JsonValue>> {
    let mut current: &mut JsonValue = root
        .entry(section_names[0].to_string())
        .or_insert_with(|| json!({}));
    for name in &section_names[1..] {
        let JsonValue::Object(map) = current else {
            bail!("provider_config.json 的 {} 段必须是对象。", section_names[0]);
        };
        current = map
            .entry(name.to_string())
            .or_insert_with(|| json!({}));
    }
    let JsonValue::Object(map) = current else {
        bail!("provider_config.json 的 {} 段必须是对象。", section_names[1]);
    };
    let list = map
        .entry(list_name.to_string())
        .or_insert_with(|| json!([]));
    let JsonValue::Array(list) = list else {
        bail!("provider_config.json 的 {list_name} 必须是数组。");
    };
    Ok(list)
}

fn provider_rules_mut(root: &mut JsonMap<String, JsonValue>) -> Result<&mut Vec<JsonValue>> {
    array_section_mut(root, ["config", "providerConfigRules"], "providerRules")
}

fn model_rules_mut(root: &mut JsonMap<String, JsonValue>) -> Result<&mut Vec<JsonValue>> {
    array_section_mut(root, ["config", "modelConfigRules"], "providerModelRules")
}

fn manual_model_rules_mut(root: &mut JsonMap<String, JsonValue>) -> Result<&mut Vec<JsonValue>> {
    array_section_mut(
        root,
        ["config", "modelConfigRules"],
        "manualProviderModelRules",
    )
}

fn provider_order_mut(root: &mut JsonMap<String, JsonValue>) -> Result<&mut Vec<JsonValue>> {
    let config = root
        .entry("config".to_string())
        .or_insert_with(|| json!({}));
    let JsonValue::Object(config) = config else {
        bail!("provider_config.json 的 config 段必须是对象。");
    };
    let order = config
        .entry("providerOrder".to_string())
        .or_insert_with(|| json!([]));
    let JsonValue::Array(order) = order else {
        bail!("provider_config.json 的 providerOrder 必须是数组。");
    };
    Ok(order)
}

fn rule_provider_id(rule: &JsonValue) -> Option<&str> {
    rule.get("providerId").and_then(JsonValue::as_str)
}

fn rule_model_id(rule: &JsonValue) -> Option<&str> {
    rule.get("modelId").and_then(JsonValue::as_str)
}

// reins 拥有的模型规则叶子；apply 只改写这些路径，其余（enabled、用户自配的
// reasoningLevel.map 等）原样保留。optionSpecs.reasoningLevel.map 不写：
// 内置的按 API 类型兜底 map 会把档位名转成各协议的正确请求字段
// （anthropic-messages 写 thinking.type/output_config.effort，chat-completions
// 写 reasoning_effort，responses 写 reasoning.effort），reins 自编 map 只会
// 引入第二份协议知识。
const OWNED_LEAVES: &[&[&str]] = &[
    &["properties", "contextWindow"],
    &["properties", "inputFormat", "supportsImage"],
    &["optionSpecs", "maxOutputTokens", "max"],
    &["optionSpecs", "reasoningLevel", "values"],
];

// reasoningLevel.values 的取值：模型声明的档位优先；只声明 reasoning 布尔时
// 给保守三档（对齐 codex 目录条目的口径）；显式 false 只有关闭档；无任何
// 信息时不写该叶子（继承内置兜底）。ZCode 的 values 要求按语义强度低到高、
// 非空且不重复，故按归一化枚举顺序排序去重。
fn reasoning_values(model: &ProviderModelRecord) -> Option<Vec<ReasoningLevel>> {
    let levels = match &model.reasoning_levels {
        Some(levels) if !levels.is_empty() => levels.clone(),
        _ => match model.reasoning {
            Some(true) => vec![
                ReasoningLevel::Low,
                ReasoningLevel::Medium,
                ReasoningLevel::High,
            ],
            Some(false) => vec![ReasoningLevel::Off],
            None => return None,
        },
    };
    let rank = |level: ReasoningLevel| {
        ReasoningLevel::all()
            .iter()
            .position(|&candidate| candidate == level)
            .expect("all 覆盖全部枚举值")
    };
    let mut sorted = levels;
    sorted.sort_by_key(|&level| rank(level));
    sorted.dedup();
    Some(sorted)
}

// 深合并：对象递归，其余后者覆盖前者。用于折叠同一 (provider, model) 的
// 多条既有规则——ZCode 自己的 upsert 会收敛成一条，这里防御手工编辑出的多条。
fn merge_json(base: &mut JsonValue, next: &JsonValue) {
    match (base, next) {
        (JsonValue::Object(base), JsonValue::Object(next)) => {
            for (key, value) in next {
                match base.get_mut(key) {
                    Some(slot) => merge_json(slot, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, next) => *base = next.clone(),
    }
}

// 删除一个叶子路径；中间节点缺失或类型不符时空操作。
fn remove_path(root: &mut JsonValue, path: &[&str]) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut current = root;
    for key in parents {
        let Some(next) = current.get_mut(*key) else {
            return;
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(*last);
    }
}

// 自底向上清掉空对象容器（删空叶子后残留的 {}），避免写出无意义噪声。
fn prune_empty_objects(root: &mut JsonValue) {
    let Some(object) = root.as_object_mut() else {
        return;
    };
    for value in object.values_mut() {
        prune_empty_objects(value);
    }
    object.retain(|_, value| !matches!(value, JsonValue::Object(map) if map.is_empty()));
}

// 生成/更新一条模型规则：以既有规则为底（保留非自有叶子），去掉自有叶子后
// 按当前模型元数据重写。无任何叶子可写时返回 None，不产生空规则。
// 不写 enabled：内置 `.*` 兜底默认启用，显式写反而会覆盖用户在 ZCode 里对该
// 模型的启停选择。
fn model_rule(
    registration: &str,
    model: &ProviderModelRecord,
    mut config: JsonValue,
    dialect: &super::dialect::ReasoningDialect,
) -> Option<JsonValue> {
    if !config.is_object() {
        config = json!({});
    }
    for path in OWNED_LEAVES {
        remove_path(&mut config, path);
    }
    prune_empty_objects(&mut config);

    let mut owned = JsonMap::new();
    if let Some(context_window) = model.context_window {
        owned.insert("contextWindow".to_string(), json!(context_window));
    }
    if let Some(supports_images) = model.supports_images {
        owned.insert(
            "inputFormat".to_string(),
            json!({ "supportsImage": supports_images }),
        );
    }
    if !owned.is_empty() {
        merge_json(
            &mut config,
            &json!({ "properties": JsonValue::Object(owned) }),
        );
    }
    if let Some(max_output) = model.max_output_tokens {
        merge_json(
            &mut config,
            &json!({ "optionSpecs": { "maxOutputTokens": { "max": max_output } } }),
        );
    }
    if let Some(levels) = reasoning_values(model) {
        let values: Vec<&str> = levels.iter().map(|&level| dialect.spell(level)).collect();
        merge_json(
            &mut config,
            &json!({ "optionSpecs": { "reasoningLevel": { "values": values } } }),
        );
    }

    if config.as_object().is_some_and(serde_json::Map::is_empty) {
        return None;
    }
    Some(json!({
        "providerId": registration,
        "modelId": model.id,
        "config": config,
    }))
}

// 应用前收集本 providerId 的模型规则现状：智能规则按模型折叠成底（保留非
// 自有叶子），手动规则的模型名集合（跳过写入的依据）。
fn collect_model_rule_state(
    root: &JsonMap<String, JsonValue>,
    registration: &str,
) -> (BTreeMap<String, JsonValue>, std::collections::BTreeSet<String>) {
    let mut existing_by_model: BTreeMap<String, JsonValue> = BTreeMap::new();
    let mut manual_models = std::collections::BTreeSet::new();
    let section = root
        .get("config")
        .and_then(|config| config.get("modelConfigRules"));
    let rules_of = |name: &str| {
        section
            .and_then(|section| section.get(name))
            .and_then(JsonValue::as_array)
    };
    for rule in rules_of("providerModelRules").into_iter().flatten() {
        if rule_provider_id(rule) != Some(registration) {
            continue;
        }
        let Some(model_id) = rule_model_id(rule) else {
            continue;
        };
        let config = rule.get("config").cloned().unwrap_or_else(|| json!({}));
        match existing_by_model.get_mut(model_id) {
            Some(base) => merge_json(base, &config),
            None => {
                existing_by_model.insert(model_id.to_string(), config);
            }
        }
    }
    for rule in rules_of("manualProviderModelRules").into_iter().flatten() {
        if rule_provider_id(rule) == Some(registration)
            && let Some(model_id) = rule_model_id(rule)
        {
            manual_models.insert(model_id.to_string());
        }
    }
    (existing_by_model, manual_models)
}

impl AppAdapter for ZcodeAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Zcode
    }

    fn config_paths(&self, env: &Environment) -> Result<Vec<PathBuf>> {
        Ok(vec![provider_config_path(env)?])
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: ProviderProtocol::all(),
            additive: true,
            required_model_fields: &[],
            // 四类元数据都有落点（上下文窗口、最大输出、图像输入、思考档位
            // 经 optionSpecs.reasoningLevel.values 写入）；无工具级默认档位，
            // 档位是每模型值域 + 会话级选择。
            unwritten_model_fields: &[],
            supported_reasoning_levels: &[],
            behavior_notes: &[
                "ZCode 的模型与思考档选择是会话级、不落盘：Reins 不写这两项。",
                "官方 OAuth 登录不经 Reins（凭据由 ZCode 自管的 credentials.json 存储，不读写）。",
            ],
        }
    }

    fn reasoning_dialect(&self) -> &'static super::dialect::ReasoningDialect {
        &super::dialect::ZCODE_REASONING
    }

    fn protocol_dialect(&self) -> Option<&'static super::dialect::ProtocolDialect> {
        Some(&ZCODE_PROTOCOL)
    }

    fn inspect(
        &self,
        env: &Environment,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let path = provider_config_path(env)?;
        let mut state = super::empty_state(ProviderAppId::Zcode, vec![path.clone()]);
        if !path.exists() {
            return Ok(state);
        }
        state.config_exists = true;
        let root = read_json_object(&path)?;
        let Some(rules) = root
            .get("config")
            .and_then(|config| config.get("providerConfigRules"))
            .and_then(|rules| rules.get("providerRules"))
            .and_then(JsonValue::as_array)
        else {
            return Ok(state);
        };
        for rule in rules {
            // providerId 是 zod 必填；缺失的条目无法寻址，跳过展示。
            let Some(key) = rule_provider_id(rule) else {
                continue;
            };
            let api = rule.get("config").and_then(|config| config.get("api"));
            let base_url = api
                .and_then(|api| api.get("baseUrl"))
                .and_then(JsonValue::as_str);
            let protocol = api
                .and_then(|api| api.get("type"))
                .and_then(JsonValue::as_str)
                .and_then(|value| ZCODE_PROTOCOL.parse(value));
            let entry = if key.starts_with(super::REINS_PREFIX) {
                classify_reins_entry(key, base_url, protocol, providers)
            } else {
                let mut notes = Vec::new();
                if let Some(name) = rule.get("providerName").and_then(JsonValue::as_str) {
                    notes.push(format!("外部 Provider：{name}"));
                }
                if let Some(template) = rule.get("templateId").and_then(JsonValue::as_str) {
                    notes.push(format!("官方模板：{template}"));
                }
                external_entry(
                    key.to_string(),
                    base_url.map(str::to_string),
                    notes,
                    protocol,
                )
            };
            state.entries.push(entry);
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
        let path = provider_config_path(env)?;
        let mut root = read_json_object(&path)?;
        // 新建文件时补 schemaVersion；既有值（未来升级）保持不动。
        root.entry("schemaVersion".to_string())
            .or_insert_with(|| json!(1));

        let registration = registration_key(&provider.id);

        // 条目级替换语义：同名 reins- 条目覆盖，用户自配条目不动。
        let entry = json!({
            "providerId": registration,
            "providerName": provider.label,
            "config": {
                "group": "standard-personal",
                "access": { "type": "api-key", "apiKey": api_key },
                "api": {
                    "type": ZCODE_PROTOCOL.spell(provider.protocol),
                    "baseUrl": provider.base_url,
                },
                "personalModelIds": plan.model_ids,
                "modelOrder": plan.model_ids,
            },
        });
        {
            let rules = provider_rules_mut(&mut root)?;
            match rules
                .iter_mut()
                .find(|rule| rule_provider_id(rule) == Some(registration.as_str()))
            {
                Some(slot) => *slot = entry,
                None => rules.push(entry),
            }
        }

        // providerOrder 是选择器里的平台顺序；reins 条目不在列时追加到末尾。
        {
            let order = provider_order_mut(&mut root)?;
            if !order
                .iter()
                .any(|value| value.as_str() == Some(registration.as_str()))
            {
                order.push(json!(registration));
            }
        }

        // 模型级规则：本 providerId 的既有智能规则按模型折叠成底（保留
        // enabled 等非自有叶子），再按当前元数据重写自有叶子；未选中的模型
        // 整体丢弃。用户已手动配置的模型跳过不写——schema 禁止同一
        // (provider, model) 同时存在智能与手动两类规则，写了会让整份配置
        // 校验失败，用户的显式选择优先。
        {
            let (existing_by_model, manual_models) = collect_model_rule_state(&root, &registration);
            let model_rules = model_rules_mut(&mut root)?;
            model_rules.retain(|rule| rule_provider_id(rule) != Some(registration.as_str()));
            for model_id in &plan.model_ids {
                if manual_models.contains(model_id.as_str()) {
                    continue;
                }
                let model = provider
                    .models
                    .iter()
                    .find(|model| &model.id == model_id)
                    .expect("validated by validate_plan");
                let base = existing_by_model
                    .get(model_id)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if let Some(rule) =
                    model_rule(&registration, model, base, self.reasoning_dialect())
                {
                    model_rules.push(rule);
                }
            }
        }

        Ok(vec![(path, serialize_json(&root)?)])
    }

    fn remove(
        &self,
        env: &Environment,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>> {
        let _ = provider;
        let path = provider_config_path(env)?;
        if !path.exists() {
            bail!("ZCode 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_json_object(&path)?;
        let registration = registration_key(provider_id);

        // 只删内容仍可识别的条目：必须带 api.baseUrl 且 api.type 是协议表
        // 拼写；用户改过的漂移配置不自动删。
        let rule = {
            let rules = root
                .get("config")
                .and_then(|config| config.get("providerConfigRules"))
                .and_then(|rules| rules.get("providerRules"))
                .and_then(JsonValue::as_array)
                .ok_or_else(|| unrecognized_entry_error("ZCode", &registration))?;
            rules
                .iter()
                .find(|rule| rule_provider_id(rule) == Some(registration.as_str()))
                .cloned()
                .ok_or_else(|| unrecognized_entry_error("ZCode", &registration))?
        };
        let api = rule.get("config").and_then(|config| config.get("api"));
        let recognizable = api
            .and_then(|api| api.get("baseUrl"))
            .and_then(JsonValue::as_str)
            .is_some()
            && api
                .and_then(|api| api.get("type"))
                .and_then(JsonValue::as_str)
                .and_then(|value| ZCODE_PROTOCOL.parse(value))
                .is_some();
        if !recognizable {
            return Err(modified_entry_error(&registration));
        }

        provider_rules_mut(&mut root)?.retain(|rule| {
            rule_provider_id(rule) != Some(registration.as_str())
        });
        provider_order_mut(&mut root)?.retain(|value| {
            value.as_str() != Some(registration.as_str())
        });
        model_rules_mut(&mut root)?.retain(|rule| {
            rule_provider_id(rule) != Some(registration.as_str())
        });
        // 手动规则同样按 providerId 清掉：对齐 ZCode 自己的
        // deletePersonalProvider（deleteExactForProvider 覆盖智能与手动两类），
        // 平台已移除时残留的模型规则没有意义。
        manual_model_rules_mut(&mut root)?.retain(|rule| {
            rule_provider_id(rule) != Some(registration.as_str())
        });

        Ok(vec![(path, serialize_json(&root)?)])
    }

    fn remove_external(
        &self,
        env: &Environment,
        entry_key: &str,
    ) -> Result<Vec<(PathBuf, String)>> {
        let path = provider_config_path(env)?;
        if !path.exists() {
            bail!("ZCode 配置文件不存在：{}", display_path(&path));
        }
        let mut root = read_json_object(&path)?;

        // 外部条目内容任意，跳过内容识别校验。
        let removed = {
            let rules = provider_rules_mut(&mut root)?;
            let before = rules.len();
            rules.retain(|rule| rule_provider_id(rule) != Some(entry_key));
            before != rules.len()
        };
        if !removed {
            return Err(entry_not_found_error("ZCode", entry_key));
        }
        provider_order_mut(&mut root)?.retain(|value| value.as_str() != Some(entry_key));
        model_rules_mut(&mut root)?.retain(|rule| rule_provider_id(rule) != Some(entry_key));
        // 外部条目的手动规则没有 reins 的稳定归属，不代用户清理。
        Ok(vec![(path, serialize_json(&root)?)])
    }
}
