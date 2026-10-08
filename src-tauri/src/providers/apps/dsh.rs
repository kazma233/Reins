// dsh（DeepSeek Harness）：LLM 路由写 Cordis patch 层。
// 桌面版对 $DSH_HOME/settings.yaml 是「一次性导入」语义（启动时改名
// .imported，被运行组合拒绝的节只留日志不落盘），不能作为常驻写入目标；
// 路由走与 dsh 自身设置一致的持久化形态：全局 cordis.patch.yml 里
// id=llm-pi-ai 的行覆盖提供 providers 路由（config 整块替换），desktop
// profile 的 cordis.patch.yml 里 id=agent-default-model 的行提供默认模型
// 与默认思考档。密钥写入 dsh 托管的 .credentials.yaml（名字→值严格映射），
// 路由经 apiKeyEnv 引用——与 dsh Models 页自己的写入方式一致；Reins 只
// 行级 patch 自己的 ref，其余条目原样保留。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_yaml::{Mapping, Value as YamlValue};

use super::{
    AppAdapter, AppCapability, ModelField, ToolEnv, display_path, ensure_protocol_supported,
    external_entry, registration_key,
};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppId, ProviderAppState, ProviderProtocol, ReasoningLevel,
    ResolvedProvider,
};
use crate::support::dsh_patch;
use crate::support::fs::user_home_dir;

pub(crate) struct DshAdapter;

// dsh 的 llm-pi-ai 层与 pi 共用同一套 KnownApi 枚举（协议名同 pi）。
fn api_name(protocol: ProviderProtocol) -> &'static str {
    match protocol {
        ProviderProtocol::OpenaiResponses => "openai-responses",
        ProviderProtocol::OpenaiChatCompletions => "openai-completions",
        ProviderProtocol::AnthropicMessages => "anthropic-messages",
    }
}

// 反读时从 api 字段反推协议；未知取值留空展示。
fn protocol_from_api(api: Option<&str>) -> Option<ProviderProtocol> {
    match api? {
        "openai-responses" => Some(ProviderProtocol::OpenaiResponses),
        "openai-completions" => Some(ProviderProtocol::OpenaiChatCompletions),
        "anthropic-messages" => Some(ProviderProtocol::AnthropicMessages),
        _ => None,
    }
}

fn key(name: &str) -> YamlValue {
    YamlValue::String(name.to_string())
}

// pi-ai 的思考档位首档是 "off"（与 Pi 适配器同一套档位枚举，dsh 的 LLM 层
// 就是 llm-pi-ai）。
fn dsh_thinking_level(level: ReasoningLevel) -> &'static str {
    match level {
        ReasoningLevel::Off => "off",
        _ => level.as_str(),
    }
}

// 反读 reasoningEffort / reasoning 字段：首档同样是 "off"。
fn dsh_thinking_level_from_str(value: &str) -> Option<ReasoningLevel> {
    if value.trim().eq_ignore_ascii_case("off") {
        return Some(ReasoningLevel::Off);
    }
    ReasoningLevel::parse(value)
}

// 路由的凭据引用名：.credentials.yaml 的键与路由 apiKeyEnv 用同一名字。
fn api_key_ref(provider_id: &str) -> String {
    let mut name = String::from("REINS_");
    name.extend(provider_id.chars().map(|ch| {
        if ch.is_ascii_alphanumeric() {
            ch.to_ascii_uppercase()
        } else {
            '_'
        }
    }));
    while name.ends_with('_') {
        name.pop();
    }
    name
}

// ---------------------------------------------------------------------------
// 路径
// ---------------------------------------------------------------------------

pub(crate) fn dsh_home(env: &ToolEnv) -> Result<PathBuf> {
    match &env.dsh_home {
        Some(dir) => Ok(dir.clone()),
        None => user_home_dir()
            .map(|home| home.join(".dsh"))
            .ok_or_else(|| anyhow::anyhow!("无法解析 HOME 目录。")),
    }
}

pub(crate) fn dsh_patch_path(env: &ToolEnv) -> Result<PathBuf> {
    Ok(dsh_home(env)?.join("cordis.patch.yml"))
}

// 默认模型行写在桌面版实际使用的 profile patch 里——profile 层晚于全局层
// 生效，之前 settings 导入落下的 agent-default-model 行就位于此，只有同层
// 覆盖才能纠正它。
pub(crate) fn dsh_profile_patch_path(env: &ToolEnv) -> Result<PathBuf> {
    Ok(dsh_home(env)?.join("profiles/desktop/cordis.patch.yml"))
}

pub(crate) fn dsh_credentials_path(env: &ToolEnv) -> Result<PathBuf> {
    Ok(dsh_home(env)?.join(".credentials.yaml"))
}

// ---------------------------------------------------------------------------
// patch 行操作
// ---------------------------------------------------------------------------

// 行覆盖型 op 的形态是顶层 {id, name, config}；insert 型 op 的条目嵌在
// insert 数组里（顶层无 id 键），天然不会与行覆盖混淆。
fn find_row<'a>(ops: &'a [YamlValue], row_id: &str) -> Option<&'a YamlValue> {
    ops.iter()
        .find(|op| op.get("id").and_then(YamlValue::as_str) == Some(row_id))
}

fn row_config(row: &YamlValue) -> Mapping {
    row.get("config")
        .and_then(YamlValue::as_mapping)
        .cloned()
        .unwrap_or_default()
}

fn row_into_ops(ops: &mut Vec<YamlValue>, row_id: &str, name: &str, config: Mapping) {
    let mut row = Mapping::new();
    row.insert(key("id"), key(row_id));
    row.insert(key("name"), key(name));
    row.insert(key("config"), YamlValue::Mapping(config));
    let row = YamlValue::Mapping(row);
    match ops
        .iter_mut()
        .find(|op| op.get("id").and_then(YamlValue::as_str) == Some(row_id))
    {
        Some(slot) => *slot = row,
        None => ops.push(row),
    }
}

fn remove_row(ops: &mut Vec<YamlValue>, row_id: &str) -> bool {
    let before = ops.len();
    ops.retain(|op| op.get("id").and_then(YamlValue::as_str) != Some(row_id));
    ops.len() != before
}

// llm 行 config.providers 映射；行缺失时返回空表。
fn llm_providers(row: Option<&YamlValue>) -> Mapping {
    row.and_then(|row| row.get("config"))
        .and_then(|config| config.get("providers"))
        .and_then(YamlValue::as_mapping)
        .cloned()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 凭据文档 patch（version:1 布局感知）
// ---------------------------------------------------------------------------

// .credentials.yaml 是 version:1 分层文档：顶层只允许 version/refs/records，
// 顶层出现其它键会让 dsh 的凭据服务启动失败并级联拖垮整个应用（实测）。
// 这里做结构化往返：refs 段下只增删自己的 ref，version/records 与其它键
// 原样保留。代价是丢掉文档内注释（dsh 明确该文件只装凭据，可接受）；
// 解析失败时大声报错，绝不静默重写。
pub(crate) fn patch_credential_ref(
    contents: &str,
    name: &str,
    api_key: Option<&str>,
) -> Result<String> {
    let serialize = |root: &YamlValue| -> Result<String> {
        serde_yaml::to_string(root).context("dsh 凭据文档序列化失败")
    };
    // 空文档从零建立 version:1 布局。
    if contents.trim().is_empty() {
        let mut refs = Mapping::new();
        let mut root = Mapping::new();
        if let Some(api_key) = api_key {
            refs.insert(key(name), YamlValue::String(api_key.to_string()));
        }
        root.insert(key("version"), YamlValue::Number(1.into()));
        root.insert(key("refs"), YamlValue::Mapping(refs));
        let root = YamlValue::Mapping(root);
        return serialize(&root);
    }
    let mut root: YamlValue = serde_yaml::from_str(contents).context("解析 dsh 凭据文档失败")?;
    let Some(mapping) = root.as_mapping_mut() else {
        bail!("dsh 凭据文档必须是映射。");
    };

    // 平铺旧布局迁移（与 dsh renderFlatLayoutMigration 同语义）：无 version
    // 键时把顶层条目全部嵌进 refs:，补 version: 1；非字符串/空值按 dsh 的
    // loud 拒绝语义报错。
    if !mapping.contains_key(&key("version")) && !mapping.is_empty() {
        let mut refs = Mapping::new();
        for (key, value) in std::mem::take(mapping) {
            let Some(text) = value.as_str() else {
                bail!("凭据文档是平铺旧布局且含非字符串条目，请手动迁移后再应用。");
            };
            if text.is_empty() {
                bail!("凭据文档是平铺旧布局且含空值条目，请手动删除后再应用。");
            }
            refs.insert(key, YamlValue::String(text.to_string()));
        }
        mapping.insert(key("version"), YamlValue::Number(1.into()));
        mapping.insert(key("refs"), YamlValue::Mapping(refs));
    }
    if mapping.get(&key("version")).and_then(YamlValue::as_i64) != Some(1) {
        bail!("dsh 凭据文档的 version 不是 1，无法自动写入。");
    }

    match api_key {
        Some(api_key) => {
            let refs = mapping
                .entry(key("refs"))
                .or_insert_with(|| YamlValue::Mapping(Mapping::new()));
            let Some(refs_mapping) = refs.as_mapping_mut() else {
                bail!("dsh 凭据文档的 refs 段必须是映射。");
            };
            refs_mapping.insert(key(name), YamlValue::String(api_key.to_string()));
        }
        None => {
            if let Some(refs) = mapping.get_mut(&key("refs")).and_then(YamlValue::as_mapping_mut) {
                refs.remove(&key(name));
            }
        }
    }

    // refs 清空后补空映射，避免序列化成 null（dsh 要求 refs 是映射）。
    if let Some(refs) = mapping.get_mut(&key("refs")) {
        if refs.as_mapping().is_some_and(Mapping::is_empty) {
            *refs = YamlValue::Mapping(Mapping::new());
        }
    }

    serialize(&root)
}

// ---------------------------------------------------------------------------
// 路由条目组装
// ---------------------------------------------------------------------------

fn route_entry(provider: &ResolvedProvider, plan: &ApplyProviderInput) -> Mapping {
    let mut models = serde_yaml::Sequence::new();
    for model_id in &plan.model_ids {
        let model = provider
            .models
            .iter()
            .find(|m| &m.id == model_id)
            .expect("validated by validate_plan");
        let mut model_entry = Mapping::new();
        model_entry.insert(key("id"), key(&model.id));
        if let Some(context_window) = model.context_window {
            model_entry.insert(key("contextWindow"), YamlValue::Number(context_window.into()));
        }
        if let Some(max_tokens) = model.max_output_tokens {
            model_entry.insert(key("maxTokens"), YamlValue::Number(max_tokens.into()));
        }
        // 请求模态：显式声明 image 才让手工声明的视觉模型可用（官方
        // PiAiModelProfile.input 语义），false/未填时回落 [text]。
        if let Some(supports_images) = model.supports_images {
            let mut input = vec![YamlValue::String("text".into())];
            if supports_images {
                input.push(YamlValue::String("image".into()));
            }
            model_entry.insert(key("input"), YamlValue::Sequence(input));
        }
        // 推理元数据：显式非推理用 false 剥离目录模型自带的推理能力；
        // 有等级集则只声明勾选的档位——pi-ai 规定字典键缺席即不提供该档，
        // 未勾选档写成 null 会因"没有线上值"被拒绝（仅 off 允许空值）。
        if model.reasoning == Some(false) {
            model_entry.insert(key("reasoningEfforts"), YamlValue::Bool(false));
        } else if let Some(levels) = model
            .reasoning_levels
            .as_ref()
            .filter(|levels| !levels.is_empty())
        {
            let mut efforts = Mapping::new();
            for level in levels {
                let wire = if *level == ReasoningLevel::Off {
                    YamlValue::Null
                } else {
                    YamlValue::String(dsh_thinking_level(*level).to_string())
                };
                efforts.insert(key(dsh_thinking_level(*level)), wire);
            }
            model_entry.insert(key("reasoningEfforts"), YamlValue::Mapping(efforts));
        }
        models.push(YamlValue::Mapping(model_entry));
    }

    let mut route = Mapping::new();
    route.insert(key("apiKeyEnv"), key(&api_key_ref(&provider.id)));
    route.insert(key("baseURL"), key(&provider.base_url));
    route.insert(key("api"), key(api_name(provider.protocol)));
    route.insert(key("models"), YamlValue::Sequence(models));
    // 路由级默认思考档；会话实际采用的默认档来自 agent-default-model 行。
    if let Some(level) = plan.default_reasoning_level {
        route.insert(
            key("reasoning"),
            YamlValue::String(dsh_thinking_level(level).to_string()),
        );
    }
    route
}

fn default_model_row(plan: &ApplyProviderInput, registration: &str) -> Mapping {
    let mut config = Mapping::new();
    config.insert(key("provider"), key(registration));
    config.insert(key("model"), key(&plan.default_model_id));
    if let Some(level) = plan.default_reasoning_level {
        // dsh 的默认档插件值是 high；不写该键时回落它，Reins 的选择必须显式
        // 落在这里才能纠正 settings 导入留下的旧行。
        config.insert(
            key("reasoningEffort"),
            YamlValue::String(dsh_thinking_level(level).to_string()),
        );
    }
    config
}

impl AppAdapter for DshAdapter {
    fn id(&self) -> ProviderAppId {
        ProviderAppId::Dsh
    }

    fn capability(&self) -> AppCapability {
        AppCapability {
            supported_protocols: ProviderProtocol::all(),
            additive: true,
            required_model_fields: &[],
            // settings.yaml 的模型条目（PiAiModelProfile）有 contextWindow/
            // maxTokens/input/reasoningEfforts 落点（见官方 config-catalog）；
            // 唯一没有独立字段的是"推理能力"布尔，只能经思考等级字典或
            // 显式 false 间接表达（见 apply），真值+空等级组合无法表达。
            unwritten_model_fields: &[ModelField::Reasoning],
            supported_reasoning_levels: ReasoningLevel::all(),
        }
    }

    fn inspect(
        &self,
        env: &ToolEnv,
        providers: &BTreeMap<String, ResolvedProvider>,
    ) -> Result<ProviderAppState> {
        let global_patch = dsh_patch_path(env)?;
        let profile_patch = dsh_profile_patch_path(env)?;
        let credentials = dsh_credentials_path(env)?;
        let mut state = super::empty_state(
            ProviderAppId::Dsh,
            vec![
                global_patch.clone(),
                profile_patch.clone(),
                credentials.clone(),
            ],
        );
        if !global_patch.exists() && !profile_patch.exists() && !credentials.exists() {
            return Ok(state);
        }
        state.config_exists = true;

        let profile_ops = dsh_patch::read_ops(&profile_patch)
            .with_context(|| format!("读取失败：{}", display_path(&profile_patch)))?;
        // 路由与默认模型都在 profile patch 层（与 dsh Models 页同层）。
        let llm_row = find_row(&profile_ops, "llm-pi-ai");
        // 默认模型行在 profile patch 层（晚于全局层生效），不能从全局找。
        let default_row = find_row(&profile_ops, "agent-default-model");
        let (default_provider, default_model, default_level) = match default_row {
            Some(row) => {
                let config = row_config(row);
                (
                    config
                        .get(&key("provider"))
                        .and_then(YamlValue::as_str)
                        .map(str::to_string),
                    config
                        .get(&key("model"))
                        .and_then(YamlValue::as_str)
                        .map(str::to_string),
                    config
                        .get(&key("reasoningEffort"))
                        .and_then(YamlValue::as_str)
                        .and_then(dsh_thinking_level_from_str),
                )
            }
            None => (None, None, None),
        };

        for (entry_key, value) in &llm_providers(llm_row) {
            let Some(entry_key) = entry_key.as_str() else {
                continue;
            };
            let base_url = value.get("baseURL").and_then(YamlValue::as_str);
            let protocol = protocol_from_api(value.get("api").and_then(YamlValue::as_str));
            let mut entry = if entry_key.starts_with(super::REINS_PREFIX) {
                super::classify_reins_entry(entry_key, base_url, protocol, providers)
            } else {
                external_entry(
                    entry_key.to_string(),
                    base_url.map(str::to_string),
                    vec!["外部 Provider。".to_string()],
                    protocol,
                )
            };
            if let Some(models) = value.get("models").and_then(YamlValue::as_sequence) {
                entry.model_ids = models
                    .iter()
                    .filter_map(|model| model.get("id").and_then(YamlValue::as_str))
                    .map(str::to_string)
                    .collect();
            }
            if default_provider.as_deref() == Some(entry_key) {
                entry.default_model_id = default_model.clone();
            }
            state.entries.push(entry);
        }

        state.default_reasoning_level = default_level;
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
        let registration = registration_key(&provider.id);

        // LLM 路由与默认模型都写 desktop profile patch 层——与 dsh Models 页
        // 的读写同一层，设置页回显才与生效配置一致；全局层只保留 MCP 条目。
        // （patch 对同 id 行是整块替换；bundle 层的 llm-pi-ai 行会被本层覆盖。）
        let profile_patch = dsh_profile_patch_path(env)?;
        let mut profile_ops = dsh_patch::read_ops(&profile_patch)?;

        // 迁移清理：早前实验/旧版本写在全局层的 llm-pi-ai 行（仅含 reins-*
        // 路由或为空）由本层覆盖后已失效，这里顺手删掉；含外部路由的行保留。
        let global_patch = dsh_patch_path(env)?;
        if global_patch.exists() {
            let mut global_ops = dsh_patch::read_ops(&global_patch)?;
            if let Some(row) = find_row(&global_ops, "llm-pi-ai") {
                let providers = llm_providers(Some(row));
                let only_reins = providers.keys().all(|key| {
                    key.as_str().is_some_and(|name| name.starts_with(super::REINS_PREFIX))
                });
                if providers.is_empty() || only_reins {
                    remove_row(&mut global_ops, "llm-pi-ai");
                    dsh_patch::write_ops(&global_patch, &global_ops)?;
                }
            }
        }

        let mut providers_table = llm_providers(find_row(&profile_ops, "llm-pi-ai"));
        providers_table.insert(key(&registration), YamlValue::Mapping(route_entry(provider, plan)));
        let mut config = Mapping::new();
        config.insert(key("providers"), YamlValue::Mapping(providers_table));
        row_into_ops(
            &mut profile_ops,
            "llm-pi-ai",
            "@deepseek-ai/dsh-llm-pi-ai",
            config,
        );
        row_into_ops(
            &mut profile_ops,
            "agent-default-model",
            "@deepseek-ai/dsh-agent-default-model",
            default_model_row(plan, &registration),
        );
        let profile_content = dsh_patch::serialize_ops(&profile_ops)?;

        // 凭据：refs 段级 patch 自己的 ref。
        let credentials = dsh_credentials_path(env)?;
        let existing_credentials = std::fs::read_to_string(&credentials).unwrap_or_default();
        let credentials_content = patch_credential_ref(
            &existing_credentials,
            &api_key_ref(&provider.id),
            Some(api_key),
        )?;

        Ok(vec![
            (profile_patch, profile_content),
            (credentials, credentials_content),
        ])
    }

    fn remove(
        &self,
        env: &ToolEnv,
        provider_id: &str,
        provider: Option<&ResolvedProvider>,
    ) -> Result<Vec<(PathBuf, String)>> {
        let _ = provider;
        let registration = registration_key(provider_id);

        let profile_patch = dsh_profile_patch_path(env)?;
        if !profile_patch.exists() {
            bail!("dsh 配置文件不存在：{}", display_path(&profile_patch));
        }
        let mut profile_ops = dsh_patch::read_ops(&profile_patch)?;
        let mut providers_table = llm_providers(find_row(&profile_ops, "llm-pi-ai"));
        if providers_table.get(&key(&registration)).is_none() {
            bail!("dsh 中没有可识别的 {registration} 条目。");
        }
        let mut files = Vec::new();
        providers_table.remove(&key(&registration));
        if providers_table.is_empty() {
            // providers 清空后删整行，恢复组合里的休眠挂载。
            remove_row(&mut profile_ops, "llm-pi-ai");
        } else {
            let mut config = Mapping::new();
            config.insert(key("providers"), YamlValue::Mapping(providers_table));
            row_into_ops(
                &mut profile_ops,
                "llm-pi-ai",
                "@deepseek-ai/dsh-llm-pi-ai",
                config,
            );
        }
        files.push((profile_patch.clone(), dsh_patch::serialize_ops(&profile_ops)?));

        // 凭据随路由一起清掉（refs 段删自己的 ref）。
        let credentials = dsh_credentials_path(env)?;
        if credentials.exists() {
            let existing = std::fs::read_to_string(&credentials).unwrap_or_default();
            let updated = patch_credential_ref(&existing, &api_key_ref(provider_id), None)?;
            if updated != existing {
                files.push((credentials, updated));
            }
        }

        // 默认模型行若指向被删路由，一并清掉（回落组合默认）。
        let points_at_route = find_row(&profile_ops, "agent-default-model").is_some_and(|row| {
            row_config(row)
                .get(&key("provider"))
                .and_then(YamlValue::as_str)
                == Some(registration.as_str())
        });
        if points_at_route && remove_row(&mut profile_ops, "agent-default-model") {
            files.push((profile_patch, dsh_patch::serialize_ops(&profile_ops)?));
        }

        Ok(files)
    }

    fn remove_external(&self, env: &ToolEnv, entry_key: &str) -> Result<Vec<(PathBuf, String)>> {
        let profile_patch = dsh_profile_patch_path(env)?;
        if !profile_patch.exists() {
            bail!("dsh 配置文件不存在：{}", display_path(&profile_patch));
        }
        let mut profile_ops = dsh_patch::read_ops(&profile_patch)?;
        let mut providers_table = llm_providers(find_row(&profile_ops, "llm-pi-ai"));
        if providers_table.remove(&key(entry_key)).is_none() {
            bail!("dsh 路由里没有条目 {entry_key}。");
        }
        let mut files = Vec::new();
        if providers_table.is_empty() {
            remove_row(&mut profile_ops, "llm-pi-ai");
        } else {
            let mut config = Mapping::new();
            config.insert(key("providers"), YamlValue::Mapping(providers_table));
            row_into_ops(
                &mut profile_ops,
                "llm-pi-ai",
                "@deepseek-ai/dsh-llm-pi-ai",
                config,
            );
        }
        files.push((profile_patch.clone(), dsh_patch::serialize_ops(&profile_ops)?));

        let points_at_route = find_row(&profile_ops, "agent-default-model").is_some_and(|row| {
            row_config(row)
                .get(&key("provider"))
                .and_then(YamlValue::as_str)
                == Some(entry_key)
        });
        if points_at_route && remove_row(&mut profile_ops, "agent-default-model") {
            files.push((profile_patch, dsh_patch::serialize_ops(&profile_ops)?));
        }
        Ok(files)
    }
}
