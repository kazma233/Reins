// OpenCode v2：~/.config/opencode/opencode.json(c)。多 Provider 并存：新增
// reins- 条目，不动用户已有 Provider。
//
// opencode.json 与 opencode.jsonc 都会被 OpenCode 加载，顶层键按「后文件
// 整键覆盖先文件」合并，.jsonc 优先（v2.0.18 config/discovery.ts 的
// names 顺序 + config.ts 的 findLast）；OpenCode 自身的配置更新也优先写
// .jsonc。因此读取看两个文件，写入目标选已存在的最高优先级文件，两个
// 都不存在时创建 opencode.json。
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
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::{
    AppAdapter, AppCapability, ToolEnv, ensure_protocol_supported, external_entry,
    opencode_candidate_paths, read_json_object, read_jsonc_object, registration_key,
    serialize_json,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppEntry, ProviderAppId, ProviderAppState, ProviderProtocol,
    ReasoningLevel, ResolvedProvider,
};

pub(crate) struct OpencodeAdapter;

// 按低→高优先级加载两个候选文件；缺失文件返回空对象，是否真实存在以
// path.exists() 判断。.jsonc 走 json5 解析（容忍注释）。
fn load_config_files() -> Result<Vec<(PathBuf, JsonMap<String, JsonValue>)>> {
    opencode_candidate_paths()?
        .into_iter()
        .map(|path| {
            let root = if is_jsonc(&path) {
                read_jsonc_object(&path)?
            } else {
                read_json_object(&path)?
            };
            Ok((path, root))
        })
        .collect()
}

fn is_jsonc(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonc"))
}

fn file_display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

// 顶层键的生效归属：最后定义该键的文件整键生效（findLast 语义）。
fn effective_file<'a>(
    files: &'a [(PathBuf, JsonMap<String, JsonValue>)],
    key: &str,
) -> Option<&'a JsonMap<String, JsonValue>> {
    files
        .iter()
        .rev()
        .find(|(_, root)| root.contains_key(key))
        .map(|(_, root)| root)
}

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

// 思考等级：openai 系包写 settings.reasoningEffort，anthropic 包的
// reasoningEffort 实测不上线，改用显式 thinking 预算，故同档位都能表达。
// OpenCode v2 对 reasoningEffort 只做字符串校验并按原值透传
// （asOpenAIReasoningEffort 恒等，二进制实证），OpenAI 官方 ReasoningEffort
// 枚举含 max（gpt-5.6/gpt-6-astra 同时支持 xhigh 与 max，两档强度不同），
// 因此 max 直写；anthropic 侧落到与 xhigh 相同的预算。
const SUPPORTED_LEVELS: &[ReasoningLevel] = &[
    ReasoningLevel::Off,
    ReasoningLevel::Minimal,
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::Xhigh,
    ReasoningLevel::Max,
];

// anthropic 包的 thinking 必须带预算（实测：{type:"enabled"} 无 budgetTokens
// 时请求不发出，{type:"disabled"} 上线）。官方与 OpenCode 都没有档位到预算的
// 通用映射，这里是 Reins 的固定阶梯；换档位等于换思考预算。
fn thinking_budget_tokens(level: ReasoningLevel) -> i64 {
    match level {
        ReasoningLevel::Off => 0,
        ReasoningLevel::Minimal => 1_024,
        ReasoningLevel::Low => 2_048,
        ReasoningLevel::Medium => 8_192,
        ReasoningLevel::High => 16_384,
        ReasoningLevel::Xhigh | ReasoningLevel::Max => 32_768,
    }
}

// 模型条目里的思考设置；openai 系与 anthropic 包写法不同（见上）。
fn reasoning_settings(protocol: ProviderProtocol, level: ReasoningLevel) -> JsonValue {
    let mut settings = JsonMap::new();
    match protocol {
        ProviderProtocol::AnthropicMessages => {
            let budget = thinking_budget_tokens(level);
            settings.insert(
                "thinking".to_string(),
                if budget == 0 {
                    json_thinking("disabled", None)
                } else {
                    json_thinking("enabled", Some(budget))
                },
            );
        }
        _ => {
            settings.insert(
                "reasoningEffort".to_string(),
                JsonValue::String(level.as_str().to_string()),
            );
        }
    }
    JsonValue::Object(settings)
}

fn json_thinking(kind: &str, budget_tokens: Option<i64>) -> JsonValue {
    let mut thinking = JsonMap::new();
    thinking.insert("type".to_string(), JsonValue::String(kind.to_string()));
    if let Some(budget_tokens) = budget_tokens {
        thinking.insert("budgetTokens".to_string(), JsonValue::from(budget_tokens));
    }
    JsonValue::Object(thinking)
}

// 反向查表，与 thinking_budget_tokens 同源，避免两条阶梯分叉。
fn level_from_thinking_budget(budget_tokens: i64) -> Option<ReasoningLevel> {
    SUPPORTED_LEVELS
        .iter()
        .copied()
        .find(|level| thinking_budget_tokens(*level) == budget_tokens)
}

// 反读默认模型条目里的思考设置（openai 系 settings.reasoningEffort、
// anthropic 包 settings.thinking）；默认模型不是 Reins 条目或没有该字段
// 时留空，不猜等级。
fn current_reasoning_level(root: &JsonMap<String, JsonValue>) -> Option<ReasoningLevel> {
    let (provider_key, model_key) = root
        .get("model")
        .and_then(JsonValue::as_str)?
        .split_once('/')?;
    let settings = root
        .get("providers")
        .and_then(|providers| providers.get(provider_key))
        .and_then(|provider| provider.get("models"))
        .and_then(|models| models.get(model_key))
        .and_then(|model| model.get("settings"))?;
    if let Some(value) = settings.get("reasoningEffort").and_then(JsonValue::as_str) {
        return ReasoningLevel::parse(value);
    }
    let thinking = settings.get("thinking")?;
    if thinking.get("type").and_then(JsonValue::as_str) == Some("disabled") {
        return Some(ReasoningLevel::Off);
    }
    thinking
        .get("budgetTokens")
        .and_then(JsonValue::as_i64)
        .and_then(level_from_thinking_budget)
}

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

// 从单个文件根里移除注册键：v2 providers 与 v1 provider 两个节点都查，
// 并清掉指向该键的顶层 model。调用方负责可识别性校验。
fn remove_registration_from(
    root: &mut JsonMap<String, JsonValue>,
    registration: &str,
) -> Result<()> {
    for node in ["providers", "provider"] {
        let Some(table) = root.get_mut(node).and_then(JsonValue::as_object_mut) else {
            continue;
        };
        if table.remove(registration).is_some() && table.is_empty() {
            root.remove(node);
        }
    }
    let prefix = format!("{registration}/");
    if root
        .get("model")
        .and_then(JsonValue::as_str)
        .is_some_and(|model| model.starts_with(&prefix))
    {
        root.remove("model");
    }
    Ok(())
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
            // 四类元数据都有落点：limit.context/output、capabilities.input、
            // settings 的思考等级。
            unwritten_model_fields: &[],
            supported_reasoning_levels: SUPPORTED_LEVELS,
        }
    }

    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let _ = env;
        let files = load_config_files()?;
        let existing: Vec<PathBuf> = files
            .iter()
            .filter(|(path, _)| path.exists())
            .map(|(path, _)| path.clone())
            .collect();
        let state_paths = if existing.is_empty() {
            // 两个候选都不存在时展示默认写入目标。
            vec![opencode_candidate_paths()?.first().expect("候选非空").clone()]
        } else {
            existing
        };
        let mut state = super::empty_state(ProviderAppId::Opencode, state_paths);
        if files.iter().all(|(path, _)| !path.exists()) {
            return Ok(state);
        }
        state.config_exists = true;

        // model 与 providers 各自取生效文件的值组成合并视图，供默认档位
        // 反读（current_reasoning_level 只读这两个键）。
        let mut merged = JsonMap::new();
        for key in ["model", "providers"] {
            if let Some(root) = effective_file(&files, key) {
                merged.insert(key.to_string(), root.get(key).expect("checked").clone());
            }
        }
        // 应用弹窗据此预选；反读值只用于回显，不参与漂移判定。
        state.default_reasoning_level = current_reasoning_level(&merged);
        let default_model = merged.get("model").and_then(JsonValue::as_str);

        for (path, root) in &files {
            if !path.exists() {
                continue;
            }
            let file_name = file_display_name(path);
            for (node, is_v2) in [("providers", true), ("provider", false)] {
                let Some(table) = root.get(node).and_then(JsonValue::as_object) else {
                    continue;
                };
                // 该文件在此节点被更高优先级文件覆盖时，条目仍列出但注明
                // 不生效，避免用户在低优先级文件里改了个寂寞。
                let file_index = files
                    .iter()
                    .position(|(other_path, _)| other_path == path)
                    .expect("iterating over files");
                let shadowed_by = files[file_index + 1..]
                    .iter()
                    .rev()
                    .find(|(_, other_root)| other_root.contains_key(node))
                    .map(|(other_path, _)| file_display_name(other_path));
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
                    let mut entry = entry_from_provider_value(
                        key,
                        base_url,
                        protocol,
                        models,
                        default_model,
                        providers,
                    );
                    if let Some(shadow_name) = &shadowed_by {
                        entry.notes.push(format!(
                            "{file_name} 的 {node} 键被 {shadow_name} 覆盖，不生效。"
                        ));
                    }
                    state.entries.push(entry);
                }
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
        let _ = env;
        ensure_protocol_supported(self, provider)?;
        let mut files = load_config_files()?;
        let registration = registration_key(&provider.id);
        // 写入目标：已存在的最高优先级文件（与 OpenCode 自身更新器的选择
        // 一致）；都不存在时创建 opencode.json。
        let target_index = files
            .iter()
            .rposition(|(path, _)| path.exists())
            .unwrap_or(0);

        let mut outputs: Vec<(PathBuf, String)> = Vec::new();
        {
            let (target_path, root) = &mut files[target_index];
            // 该平台上一次写入的模型条目：本次没选思考等级时沿用其中的
            // settings，重新应用不会把上次选的等级写没。
            let previous_models = root
                .get("providers")
                .and_then(|providers| providers.get(registration.as_str()))
                .and_then(|provider| provider.get("models"))
                .and_then(JsonValue::as_object)
                .cloned();

            let mut models = JsonMap::new();
            for model_id in &plan.model_ids {
                let model = provider.models.iter().find(|m| &m.id == model_id);
                let label = model
                    .map(|m| m.label.clone())
                    .unwrap_or_else(|| model_id.clone());
                let mut model_entry = JsonMap::new();
                model_entry.insert("name".to_string(), JsonValue::String(label));
                if let Some(model) = model {
                    // limit 的 context/output 在官方 schema 里成对必填；只有一项时
                    // 整段不写，避免半截限制参与压缩与输出截断判断。
                    if let (Some(context_window), Some(max_output_tokens)) =
                        (model.context_window, model.max_output_tokens)
                    {
                        let mut limit = JsonMap::new();
                        limit.insert("context".to_string(), JsonValue::from(context_window));
                        limit.insert("output".to_string(), JsonValue::from(max_output_tokens));
                        model_entry.insert("limit".to_string(), JsonValue::Object(limit));
                    }
                    // capabilities 同时表达工具支持与输入模态；官方对目录外模型
                    // 默认假设 text+image 输入，有元数据就显式声明。
                    if let Some(supports_images) = model.supports_images {
                        let mut input = vec![JsonValue::String("text".to_string())];
                        if supports_images {
                            input.push(JsonValue::String("image".to_string()));
                        }
                        let mut capabilities = JsonMap::new();
                        capabilities.insert("tools".to_string(), JsonValue::Bool(true));
                        capabilities.insert("input".to_string(), JsonValue::Array(input));
                        capabilities.insert(
                            "output".to_string(),
                            JsonValue::Array(vec![JsonValue::String("text".to_string())]),
                        );
                        model_entry.insert("capabilities".to_string(), JsonValue::Object(capabilities));
                    }
                    if let Some(level) = plan.default_reasoning_level {
                        model_entry.insert(
                            "settings".to_string(),
                            reasoning_settings(provider.protocol, level),
                        );
                    } else if let Some(settings) = previous_models
                        .as_ref()
                        .and_then(|models| models.get(model_id))
                        .and_then(|model| model.get("settings"))
                    {
                        model_entry.insert("settings".to_string(), settings.clone());
                    }
                }
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
                bail!("providers 段不是对象：{}", target_path.display());
            }
            root.get_mut("providers")
                .and_then(JsonValue::as_object_mut)
                .expect("checked above")
                .insert(registration.clone(), JsonValue::Object(provider_entry));
            root.insert(
                "model".to_string(),
                JsonValue::String(format!("{registration}/{}", plan.default_model_id)),
            );
            outputs.push((target_path.clone(), serialize_json(root)?));
        }

        // 写入目标之外还留有本平台旧注册键的文件：清掉旧键，避免同一个
        // reins- 条目散在两个文件里误导反读。
        for (index, (path, root)) in files.iter_mut().enumerate() {
            if index == target_index || !path.exists() {
                continue;
            }
            let has_registration = ["providers", "provider"].iter().any(|node| {
                root.get(*node)
                    .and_then(|node| node.get(registration.as_str()))
                    .is_some()
            });
            if !has_registration {
                continue;
            }
            remove_registration_from(root, &registration)?;
            outputs.push((path.clone(), serialize_json(root)?));
        }

        Ok(outputs)
    }

    fn remove(
        &self,
        env: &ToolEnv,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>> {
        let _ = env;
        let _ = provider;
        let registration = registration_key(provider_id);
        let files = load_config_files()?;
        let mut outputs = Vec::new();
        let mut found = false;
        for (path, mut root) in files {
            if !path.exists() {
                continue;
            }
            let contains_registration = ["providers", "provider"].iter().any(|node| {
                root.get(*node)
                    .and_then(|node| node.get(registration.as_str()))
                    .is_some()
            });
            if !contains_registration {
                continue;
            }
            // 只删内容仍可识别的条目：baseURL 必须是字符串；两个节点里的
            // 同名条目都要通过校验后再动文件。
            for node in ["providers", "provider"] {
                if let Some(entry) = root
                    .get(node)
                    .and_then(|node| node.get(registration.as_str()))
                {
                    let recognized = ["settings", "options"].iter().any(|section| {
                        entry
                            .get(*section)
                            .and_then(|settings| settings.get("baseURL"))
                            .and_then(JsonValue::as_str)
                            .is_some()
                    });
                    if !recognized {
                        bail!("条目 {registration} 已被手工修改，无法自动识别，请手动处理。");
                    }
                }
            }
            found = true;
            // 默认 Provider 仍指向该平台时随移除清掉；model 是顶层单值键，
            // 留空后由用户在 OpenCode 内重选，不再要求先切换。
            remove_registration_from(&mut root, &registration)?;
            outputs.push((path, serialize_json(&root)?));
        }
        if !found {
            bail!("OpenCode 中没有可识别的 {registration} 条目。");
        }
        Ok(outputs)
    }

    fn remove_external(&self, env: &ToolEnv, entry_key: &str) -> Result<Vec<(PathBuf, String)>> {
        let _ = env;
        let files = load_config_files()?;
        let mut outputs = Vec::new();
        let mut removed = false;
        for (path, mut root) in files {
            if !path.exists() {
                continue;
            }
            // 与 reins- 条目移除一致：默认模型指向被删条目时随删除清空。
            // 外部条目可能同时出现在两个文件里（低优先级那份本就不生效），
            // 一并删除。
            let mut changed = false;
            let prefix = format!("{entry_key}/");
            if root
                .get("model")
                .and_then(JsonValue::as_str)
                .is_some_and(|model| model.starts_with(&prefix))
            {
                root.remove("model");
                changed = true;
            }
            for node in ["providers", "provider"] {
                let Some(table) = root.get_mut(node).and_then(JsonValue::as_object_mut) else {
                    continue;
                };
                if table.remove(entry_key).is_some() {
                    changed = true;
                    if table.is_empty() {
                        root.remove(node);
                    }
                }
            }
            if changed {
                outputs.push((path, serialize_json(&root)?));
                removed = true;
            }
        }
        if !removed {
            bail!("OpenCode 中没有外部条目 {entry_key}。");
        }
        Ok(outputs)
    }
}
