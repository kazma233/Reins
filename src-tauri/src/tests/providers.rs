// providers 域端到级行为测试：适配器「生成 → 反读 → 幂等 → 移除」闭环，
// 全部在虚拟 HOME 隔离目录下进行，密钥用 FakeKeyBackend。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde_json::{Value as JsonValue, json};
use toml::Value as TomlValue;

use crate::providers::apps::{MASKED_KEY, ToolEnv, registration_key};
use crate::providers::commands::{
    apply_provider_inner, delete_provider_inner, preview_apply_inner, providers_state_inner,
    remove_external_entry_inner, remove_provider_from_app_inner,
};
use crate::providers::config::ProviderConfigStore;
use crate::providers::keychain::{FakeKeyBackend, ProviderKeyStore};
use crate::providers::types::{
    ApplyProviderInput, ProviderAppEntryStatus, ProviderAppId, ProviderModelInput,
    ProviderProtocol, ProviderUpsertInput, ReasoningLevel,
};
use crate::test_support::{TestDir, TestEnvGuard};

struct Isolated {
    _guard: TestEnvGuard,
    // 字段只用于 Drop 清理临时目录。
    _data_dir: TestDir,
    _home_dir: TestDir,
    store: ProviderConfigStore,
    keys: ProviderKeyStore,
    env: ToolEnv,
}

impl Isolated {
    fn new() -> Result<Self> {
        let data_dir = TestDir::new("providers-data")?;
        let home_dir = TestDir::new("providers-home")?;
        let guard = TestEnvGuard::set_home(home_dir.path());
        let keys = ProviderKeyStore::with_backend(Arc::new(FakeKeyBackend::default()));
        // 先取路径再移动 TestDir，字段求值顺序不保证可用性。
        let data_path = data_dir.path().to_path_buf();
        Ok(Self {
            _guard: guard,
            _data_dir: data_dir,
            _home_dir: home_dir,
            store: ProviderConfigStore::at(data_path),
            keys,
            env: ToolEnv::default(),
        })
    }
    fn seed_provider(&self, id: &str, protocol: ProviderProtocol, base_url: &str) -> Result<()> {
        self.store.upsert(provider_input(id, protocol, base_url))?;
        Ok(())
    }

    fn seed_model(&self, provider_id: &str, model_id: &str) -> Result<()> {
        self.seed_provider_with_models(
            provider_id,
            ProviderProtocol::OpenaiResponses,
            &format!("https://{provider_id}.test/v1"),
            vec![model_id.to_string()],
        )
    }

    fn seed_provider_with_models(
        &self,
        id: &str,
        protocol: ProviderProtocol,
        base_url: &str,
        models: Vec<String>,
    ) -> Result<()> {
        self.store.upsert(ProviderUpsertInput {
            provider_id: id.to_string(),
            label: format!("Label {id}"),
            protocol,
            base_url: base_url.to_string(),
            models: models
                .into_iter()
                .map(|model_id| ProviderModelInput {
                    id: model_id.clone(),
                    label: format!("Label {model_id}"),
                    context_window: Some(200_000),
                    max_output_tokens: Some(64_000),
                    supports_images: Some(true),
                    reasoning: Some(true),
                    reasoning_levels: Some(vec![
                        ReasoningLevel::Low,
                        ReasoningLevel::Medium,
                        ReasoningLevel::High,
                    ]),
                })
                .collect(),
        })?;
        Ok(())
    }

    fn set_key(&self, provider_id: &str) -> Result<()> {
        self.keys.set_key(provider_id, "sk-test-secret")
    }

    fn apply(
        &self,
        provider_id: &str,
        app: ProviderAppId,
        models: &[&str],
        default: &str,
        level: Option<ReasoningLevel>,
    ) -> Result<()> {
        apply_provider_inner(
            &self.store,
            &self.keys,
            &self.env,
            &plan(provider_id, app, models, default, level),
        )?;
        Ok(())
    }

    fn remove_from(&self, provider_id: &str, app: ProviderAppId) -> Result<()> {
        remove_provider_from_app_inner(&self.store, &self.env, provider_id, app)?;
        Ok(())
    }

    fn remove_external(&self, app: ProviderAppId, entry_key: &str) -> Result<()> {
        remove_external_entry_inner(&self.env, &app, entry_key)?;
        Ok(())
    }

    fn state(&self) -> Result<crate::providers::types::ProvidersState> {
        providers_state_inner(&self.store, &self.keys, &self.env)
    }
}

fn provider_input(id: &str, protocol: ProviderProtocol, base_url: &str) -> ProviderUpsertInput {
    ProviderUpsertInput {
        provider_id: id.to_string(),
        label: format!("Label {id}"),
        protocol,
        base_url: base_url.to_string(),
        models: vec![ProviderModelInput {
            id: "model-a".to_string(),
            label: "Model A".to_string(),
            context_window: None,
            max_output_tokens: None,
            supports_images: None,
            reasoning: None,
            reasoning_levels: None,
        }],
    }
}

fn plan(
    provider_id: &str,
    app: ProviderAppId,
    models: &[&str],
    default: &str,
    level: Option<ReasoningLevel>,
) -> ApplyProviderInput {
    ApplyProviderInput {
        provider_id: provider_id.to_string(),
        app,
        model_ids: models.iter().map(|m| m.to_string()).collect(),
        default_model_id: default.to_string(),
        default_reasoning_level: level,
    }
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn codex_path() -> PathBuf {
    // home override 已生效；直接拼相对路径避免再解析一次。
    crate::support::fs::user_home_dir()
        .unwrap()
        .join(".codex")
        .join("config.toml")
}

fn claude_path() -> PathBuf {
    crate::support::fs::user_home_dir()
        .unwrap()
        .join(".claude")
        .join("settings.json")
}

fn opencode_path() -> PathBuf {
    crate::support::fs::user_home_dir()
        .unwrap()
        .join(".config")
        .join("opencode")
        .join("opencode.json")
}

fn pi_models_path() -> PathBuf {
    crate::support::fs::user_home_dir()
        .unwrap()
        .join(".pi")
        .join("agent")
        .join("models.json")
}

fn pi_settings_path() -> PathBuf {
    crate::support::fs::user_home_dir()
        .unwrap()
        .join(".pi")
        .join("agent")
        .join("settings.json")
}

fn grok_path() -> PathBuf {
    crate::support::fs::user_home_dir()
        .unwrap()
        .join(".grok")
        .join("config.toml")
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

#[test]
fn codex_apply_writes_entry_and_pointer() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    isolated.apply(
        "p1",
        ProviderAppId::Codex,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::Medium),
    )?;

    let content = read_text(&codex_path());
    assert!(content.contains("model_provider = \"reins-p1\""));
    assert!(content.contains("model = \"model-a\""));
    assert!(content.contains("model_reasoning_effort = \"medium\""));
    assert!(content.contains("base_url = \"https://p1.test/v1\""));
    assert!(content.contains("wire_api = \"responses\""));
    assert!(content.contains("experimental_bearer_token = \"sk-test-secret\""));

    // 反读：已应用 + 默认模型。
    let state = isolated.state()?;
    let codex = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Codex)
        .unwrap();
    assert_eq!(codex.entries.len(), 1);
    assert_eq!(codex.entries[0].status, ProviderAppEntryStatus::Applied);
    assert_eq!(
        codex.entries[0].default_model_id.as_deref(),
        Some("model-a")
    );
    Ok(())
}

// off 档无文档依据（写入 "none"），能力表不放行。
#[test]
fn codex_off_reasoning_level_is_rejected() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    let error = isolated
        .apply(
            "p1",
            ProviderAppId::Codex,
            &["model-a"],
            "model-a",
            Some(ReasoningLevel::Off),
        )
        .expect_err("off level should be rejected for codex");
    assert!(error.to_string().contains("不支持思考等级"));
    Ok(())
}

#[test]
fn codex_apply_is_idempotent_and_replaces_previous_platform() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.seed_model("p2", "model-b")?;
    isolated.set_key("p1")?;
    isolated.set_key("p2")?;

    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    let first = read_text(&codex_path());
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    assert_eq!(first, read_text(&codex_path()), "重复应用不应产生变更");

    // 替换语义：应用 p2 后 p1 的条目和指针都应消失。
    isolated.apply("p2", ProviderAppId::Codex, &["model-b"], "model-b", None)?;
    let content = read_text(&codex_path());
    assert!(!content.contains("reins-p1"));
    assert!(content.contains("model_provider = \"reins-p2\""));
    Ok(())
}

#[test]
fn codex_remove_cleans_pointer_and_rejects_modified_entry() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;
    isolated.apply(
        "p1",
        ProviderAppId::Codex,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::Low),
    )?;

    isolated.remove_from("p1", ProviderAppId::Codex)?;
    let content = read_text(&codex_path());
    assert!(!content.contains("reins-p1"));
    assert!(!content.contains("model_provider ="));
    assert!(!content.contains("model_reasoning_effort"));
    assert!(!content.contains("\nmodel ="));

    // 漂移配置（手工删掉 wire_api）不应被自动删。
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    let path = codex_path();
    let edited = read_text(&path).replace("wire_api = \"responses\"", "# removed");
    fs::write(&path, edited)?;
    assert!(isolated.remove_from("p1", ProviderAppId::Codex).is_err());
    Ok(())
}

// 聚合模型的窗口写入顶层 model_context_window：codex 对不在内置目录的模型
// 会走兜底元数据（272000）并告警 Unknown model。
#[test]
fn codex_apply_writes_and_clears_model_context_window() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    assert!(read_text(&codex_path()).contains("model_context_window = 200000"));

    // 元数据清空后替换应用不再写该键，避免残留旧窗口。
    isolated.store.upsert(ProviderUpsertInput {
        provider_id: "p1".to_string(),
        label: "Label p1".to_string(),
        protocol: ProviderProtocol::OpenaiResponses,
        base_url: "https://p1.test/v1".to_string(),
        models: vec![ProviderModelInput {
            id: "model-a".to_string(),
            label: "Model A".to_string(),
            context_window: None,
            max_output_tokens: None,
            supports_images: None,
            reasoning: None,
            reasoning_levels: None,
        }],
    })?;
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    assert!(!read_text(&codex_path()).contains("model_context_window"));
    Ok(())
}

// 移除时窗口随配置清掉（值仍等于该模型元数据 → 归属可确认）。
#[test]
fn codex_remove_clears_model_context_window() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;

    isolated.remove_from("p1", ProviderAppId::Codex)?;
    assert!(!read_text(&codex_path()).contains("model_context_window"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

// 聚合模型对 Claude Code 是「不认识的模型 ID」（输出默认 32000、窗口按内置
// 同名 ID 推断），窗口与输出上限经官方 env 入口显式纠正。
#[test]
fn claude_apply_writes_context_and_output_env_and_remove_clears_them() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_provider_with_models(
        "agg",
        ProviderProtocol::AnthropicMessages,
        "https://agg.test/api",
        vec!["model-a".to_string()],
    )?;
    isolated.set_key("agg")?;

    isolated.apply("agg", ProviderAppId::Claude, &["model-a"], "model-a", None)?;
    let applied: JsonValue = serde_json::from_str(&read_text(&claude_path()))?;
    assert_eq!(applied["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "200000");
    assert_eq!(applied["env"]["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "64000");

    isolated.remove_from("agg", ProviderAppId::Claude)?;
    let removed: JsonValue = serde_json::from_str(&read_text(&claude_path()))?;
    assert!(removed.get("env").is_none());
    Ok(())
}

#[test]
fn claude_apply_preserves_keys_and_remove_matches_base_url() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = claude_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, json!({"theme": "dark"}).to_string())?;
    isolated.seed_provider(
        "agg",
        ProviderProtocol::AnthropicMessages,
        "https://agg.test/api",
    )?;
    isolated.set_key("agg")?;

    isolated.apply(
        "agg",
        ProviderAppId::Claude,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(applied["theme"], "dark");
    assert_eq!(applied["env"]["ANTHROPIC_BASE_URL"], "https://agg.test/api");
    assert_eq!(applied["env"]["ANTHROPIC_API_KEY"], "sk-test-secret");
    assert_eq!(applied["model"], "model-a");
    assert_eq!(applied["effortLevel"], "high");

    // base_url 一致 → 连带清理 model / effortLevel，保留用户自己的键。
    isolated.remove_from("agg", ProviderAppId::Claude)?;
    let removed: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(removed["theme"], "dark");
    assert!(removed.get("env").is_none());
    assert!(removed.get("model").is_none());
    assert!(removed.get("effortLevel").is_none());
    Ok(())
}

#[test]
fn claude_remove_refuses_mismatched_or_orphaned_base_url() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = claude_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        json!({"env": {"ANTHROPIC_BASE_URL": "https://official"}}).to_string(),
    )?;
    isolated.seed_provider(
        "agg",
        ProviderProtocol::AnthropicMessages,
        "https://agg.test/api",
    )?;

    // base_url 不一致：拒绝且不改文件。
    assert!(isolated.remove_from("agg", ProviderAppId::Claude).is_err());
    assert_eq!(
        read_text(&path),
        json!({"env": {"ANTHROPIC_BASE_URL": "https://official"}}).to_string()
    );

    // 平台元数据已删除：无法安全移除，直接报错。
    isolated.store.delete("agg")?;
    assert!(isolated.remove_from("agg", ProviderAppId::Claude).is_err());
    Ok(())
}

#[test]
fn claude_external_base_url_shows_as_external() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = claude_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        json!({"env": {"ANTHROPIC_BASE_URL": "https://someone-else"}}).to_string(),
    )?;
    let state = isolated.state()?;
    let claude = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Claude)
        .unwrap();
    assert_eq!(claude.entries.len(), 1);
    assert_eq!(claude.entries[0].status, ProviderAppEntryStatus::External);
    Ok(())
}

// ---------------------------------------------------------------------------
// OpenCode
// ---------------------------------------------------------------------------

#[test]
fn opencode_apply_adds_entry_and_remove_clears_default_model() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        json!({
            "model": "other/model-x",
            "providers": {
                "other": {
                    "package": "@opencode/ai/providers/openai",
                    "settings": {"baseURL": "https://other.test/v1"}
                }
            }
        })
        .to_string(),
    )?;
    // chat_completions 平台应用到 OpenCode：写入 v2 规范 providers 节点。
    isolated.seed_provider_with_models(
        "p1",
        ProviderProtocol::OpenaiChatCompletions,
        "https://p1.test/v1",
        vec!["model-a".to_string()],
    )?;
    isolated.set_key("p1")?;

    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(applied["model"], "reins-p1/model-a");
    assert_eq!(
        applied["providers"]["reins-p1"]["package"],
        "@opencode/ai/providers/openai-compatible"
    );
    assert_eq!(
        applied["providers"]["other"]["package"],
        "@opencode/ai/providers/openai"
    );

    // 默认模型仍引用 → 随移除清掉 model 键，条目正常移除，外部条目保留。
    isolated.remove_from("p1", ProviderAppId::Opencode)?;
    let removed: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert!(removed["providers"].get("reins-p1").is_none());
    assert!(removed["providers"].get("other").is_some());
    assert!(removed.get("model").is_none());
    Ok(())
}

// anthropic_messages 平台也允许应用到 OpenCode，package 选 v2 原生
// anthropic 运行时，反读条目回显协议。
#[test]
fn opencode_apply_anthropic_uses_native_anthropic_package() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, json!({}).to_string())?;
    isolated.seed_provider_with_models(
        "p1",
        ProviderProtocol::AnthropicMessages,
        "https://p1.test/anthropic",
        vec!["model-a".to_string()],
    )?;
    isolated.set_key("p1")?;

    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(
        applied["providers"]["reins-p1"]["package"],
        "@opencode/ai/providers/anthropic"
    );
    assert_eq!(applied["model"], "reins-p1/model-a");

    let state = isolated.state()?;
    let opencode = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Opencode)
        .expect("opencode state");
    let entry = &opencode.entries[0];
    assert_eq!(entry.status, ProviderAppEntryStatus::Applied);
    assert_eq!(entry.protocol, Some(ProviderProtocol::AnthropicMessages));
    Ok(())
}

// openai_responses 平台应用到 OpenCode：写入 openai/responses 运行时包
// （compatible/responses 在 opencode v2.0.18 有包解析缺陷，不采用）。
#[test]
fn opencode_apply_responses_uses_openai_responses_package() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, json!({}).to_string())?;
    isolated.seed_provider_with_models(
        "p1",
        ProviderProtocol::OpenaiResponses,
        "https://p1.test/v1",
        vec!["model-a".to_string()],
    )?;
    isolated.set_key("p1")?;

    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(
        applied["providers"]["reins-p1"]["package"],
        "@opencode/ai/providers/openai/responses"
    );
    assert_eq!(applied["model"], "reins-p1/model-a");

    // 反读自身写入的条目，协议回显为 openai_responses。
    let state = isolated.state()?;
    let opencode = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Opencode)
        .expect("opencode state");
    let entry = &opencode.entries[0];
    assert_eq!(entry.status, ProviderAppEntryStatus::Applied);
    assert_eq!(entry.protocol, Some(ProviderProtocol::OpenaiResponses));
    Ok(())
}

// 模型级元数据写 v2 的 limit 与 capabilities：官方对目录外模型按 200000
// 上下文 / 32000 输出 / text+image 输入兜底，有元数据就显式声明。
#[test]
fn opencode_apply_writes_model_limit_and_capabilities() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, json!({}).to_string())?;
    isolated.seed_provider_with_models(
        "p1",
        ProviderProtocol::OpenaiChatCompletions,
        "https://p1.test/v1",
        vec!["model-a".to_string()],
    )?;
    isolated.set_key("p1")?;

    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    let model = &applied["providers"]["reins-p1"]["models"]["model-a"];
    assert_eq!(model["limit"]["context"], 200_000);
    assert_eq!(model["limit"]["output"], 64_000);
    assert_eq!(model["capabilities"]["tools"], true);
    assert_eq!(model["capabilities"]["input"], json!(["text", "image"]));
    assert_eq!(model["capabilities"]["output"], json!(["text"]));
    Ok(())
}

// limit 的 context/output 在官方 schema 里成对，缺一项时整段不写；
// supports_images 为假时输入模态收窄为 text。
#[test]
fn opencode_apply_omits_incomplete_limit_and_narrows_modalities() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, json!({}).to_string())?;
    isolated.store.upsert(ProviderUpsertInput {
        provider_id: "p1".to_string(),
        label: "Label p1".to_string(),
        protocol: ProviderProtocol::OpenaiChatCompletions,
        base_url: "https://p1.test/v1".to_string(),
        models: vec![ProviderModelInput {
            id: "model-a".to_string(),
            label: "Model A".to_string(),
            context_window: Some(128_000),
            max_output_tokens: None,
            supports_images: Some(false),
            reasoning: None,
            reasoning_levels: None,
        }],
    })?;
    isolated.set_key("p1")?;

    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    let model = &applied["providers"]["reins-p1"]["models"]["model-a"];
    assert!(model.get("limit").is_none());
    assert_eq!(model["capabilities"]["input"], json!(["text"]));
    Ok(())
}

// 思考等级：openai 系包写 settings.reasoningEffort（实测请求体带
// reasoning_effort），anthropic 包写 settings.thinking 预算（实测
// reasoningEffort 不上线，thinking 必须带 budgetTokens）。
#[test]
fn opencode_apply_writes_reasoning_effort_or_thinking_budget() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, json!({}).to_string())?;

    isolated.seed_provider_with_models(
        "oai",
        ProviderProtocol::OpenaiChatCompletions,
        "https://oai.test/v1",
        vec!["model-a".to_string()],
    )?;
    isolated.set_key("oai")?;
    isolated.apply(
        "oai",
        ProviderAppId::Opencode,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(
        applied["providers"]["reins-oai"]["models"]["model-a"]["settings"]["reasoningEffort"],
        "high"
    );

    isolated.seed_provider_with_models(
        "ant",
        ProviderProtocol::AnthropicMessages,
        "https://ant.test/anthropic",
        vec!["model-a".to_string()],
    )?;
    isolated.set_key("ant")?;
    isolated.apply(
        "ant",
        ProviderAppId::Opencode,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    let thinking = &applied["providers"]["reins-ant"]["models"]["model-a"]["settings"]["thinking"];
    assert_eq!(thinking["type"], "enabled");
    assert_eq!(thinking["budgetTokens"], 16_384);

    // 「无」档写入 disabled，不臆造预算。
    isolated.apply(
        "ant",
        ProviderAppId::Opencode,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::Off),
    )?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    let thinking = &applied["providers"]["reins-ant"]["models"]["model-a"]["settings"]["thinking"];
    assert_eq!(thinking["type"], "disabled");
    assert!(thinking.get("budgetTokens").is_none());
    Ok(())
}

// v2 providers 节点与 v1 遗留 provider 节点都参与反读，
// v2 原生包名与 v1 npm 包名都能反推协议。
#[test]
fn opencode_inspect_reads_both_v1_and_v2_nodes() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        json!({
            "providers": {
                "gateway-a": {
                    "package": "@opencode/ai/providers/anthropic-compatible",
                    "settings": {"baseURL": "https://a.test/anthropic"},
                    "models": {"m1": {}}
                }
            },
            "provider": {
                "gateway-b": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "https://b.test/v1"},
                    "models": {"m2": {}}
                }
            }
        })
        .to_string(),
    )?;

    let state = isolated.state()?;
    let opencode = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Opencode)
        .expect("opencode state");
    assert_eq!(opencode.entries.len(), 2);
    let a = opencode
        .entries
        .iter()
        .find(|entry| entry.key == "gateway-a")
        .expect("gateway-a");
    assert_eq!(a.status, ProviderAppEntryStatus::External);
    assert_eq!(a.protocol, Some(ProviderProtocol::AnthropicMessages));
    assert_eq!(a.base_url.as_deref(), Some("https://a.test/anthropic"));
    let b = opencode
        .entries
        .iter()
        .find(|entry| entry.key == "gateway-b")
        .expect("gateway-b");
    assert_eq!(b.protocol, Some(ProviderProtocol::OpenaiChatCompletions));
    Ok(())
}

// ---------------------------------------------------------------------------
// Pi
// ---------------------------------------------------------------------------

#[test]
fn pi_apply_writes_models_and_settings_then_remove_clears_defaults() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    isolated.apply(
        "p1",
        ProviderAppId::Pi,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::Low),
    )?;
    let models: JsonValue = serde_json::from_str(&read_text(&pi_models_path()))?;
    assert_eq!(
        models["providers"]["reins-p1"]["baseUrl"],
        "https://p1.test/v1"
    );
    assert_eq!(models["providers"]["reins-p1"]["apiKey"], "sk-test-secret");
    let settings: JsonValue = serde_json::from_str(&read_text(&pi_settings_path()))?;
    assert_eq!(settings["defaultProvider"], "reins-p1");
    assert_eq!(settings["defaultThinkingLevel"], "low");

    // 默认 Provider 仍指向 → 随移除清掉默认三元组。
    isolated.remove_from("p1", ProviderAppId::Pi)?;
    let models: JsonValue = serde_json::from_str(&read_text(&pi_models_path()))?;
    assert!(models["providers"].get("reins-p1").is_none());
    let settings: JsonValue = serde_json::from_str(&read_text(&pi_settings_path()))?;
    assert!(settings.get("defaultProvider").is_none());
    assert!(settings.get("defaultModel").is_none());
    assert!(settings.get("defaultThinkingLevel").is_none());
    Ok(())
}

// Pi 的思考等级首档写 "off" 而非归一化集合的 "none"（官方枚举）。
#[test]
fn pi_off_level_writes_off() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    isolated.apply(
        "p1",
        ProviderAppId::Pi,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::Off),
    )?;
    let settings: JsonValue = serde_json::from_str(&read_text(&pi_settings_path()))?;
    assert_eq!(settings["defaultThinkingLevel"], "off");
    Ok(())
}

// ---------------------------------------------------------------------------
// Grok Build
// ---------------------------------------------------------------------------

#[test]
fn grok_anthropic_apply_uses_extra_headers_and_remove_clears_default() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_provider(
        "p1",
        ProviderProtocol::AnthropicMessages,
        "https://p1.test/v1",
    )?;
    isolated.set_key("p1")?;

    isolated.apply(
        "p1",
        ProviderAppId::Grokbuild,
        &["model-a"],
        "model-a",
        None,
    )?;
    let content = read_text(&grok_path());
    assert!(content.contains("[model_providers.reins-p1]"));
    assert!(content.contains("api_backend = \"messages\""));
    // base_url 已带 /v1 时原样写入，不重复追加。
    assert!(content.contains("base_url = \"https://p1.test/v1\""));
    assert!(content.contains("reins-p1--model-a"));
    assert!(content.contains("extra_headers"));
    assert!(content.contains("x-api-key = \"sk-test-secret\""));
    assert!(!content.contains("api_key = "));
    assert!(content.contains("default = \"reins-p1--model-a\""));

    // 默认模型仍引用该平台 → 随移除清掉 models.default。
    let path = grok_path();
    isolated.remove_from("p1", ProviderAppId::Grokbuild)?;
    let content = read_text(&path);
    assert!(!content.contains("reins-p1"));
    let root: TomlValue = toml::from_str(&content)?;
    assert!(
        root.get("models")
            .and_then(|models| models.get("default"))
            .is_none()
    );
    Ok(())
}

// 选「最高 (max)」写出 xhigh：Grok UI 最高档是 xhigh 且请求层两者等价。
#[test]
fn grok_max_reasoning_writes_xhigh() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;
    isolated.apply(
        "p1",
        ProviderAppId::Grokbuild,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::Max),
    )?;
    let content = read_text(&grok_path());
    assert!(content.contains("default_reasoning_effort = \"xhigh\""));
    Ok(())
}

// 模型元数据齐全时逐模型写 context_window 与 supports_reasoning_effort：
// 缺 context_window 时 grok 按 200000 兜底计算自动压缩时机；未声明
// supports_reasoning_effort 的模型会被判为不支持思考等级，[models] 里写的
// default_reasoning_effort 被静默丢弃（grok 1.0.41 debug 日志实测）。
#[test]
fn grok_apply_writes_context_window_and_reasoning_support() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    isolated.apply(
        "p1",
        ProviderAppId::Grokbuild,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    let content = read_text(&grok_path());
    assert!(content.contains("context_window = 200000"));
    assert!(content.contains("supports_reasoning_effort = true"));
    assert!(content.contains("default_reasoning_effort = \"high\""));
    // 新增字段不影响反读归类。
    let state = isolated.state()?;
    let grok = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Grokbuild)
        .expect("grok state");
    assert_eq!(grok.entries[0].status, ProviderAppEntryStatus::Applied);
    Ok(())
}

// 元数据缺失或明确不支持思考时两个字段都不写，不落无依据的猜测值；
// 最大输出（max_completion_tokens）与图像输入本就没有落点。
#[test]
fn grok_apply_omits_absent_model_metadata() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.store.upsert(ProviderUpsertInput {
        provider_id: "p1".to_string(),
        label: "Label p1".to_string(),
        protocol: ProviderProtocol::OpenaiChatCompletions,
        base_url: "https://p1.test/v1".to_string(),
        models: vec![ProviderModelInput {
            id: "model-a".to_string(),
            label: "Model A".to_string(),
            context_window: None,
            max_output_tokens: Some(64_000),
            supports_images: Some(true),
            reasoning: Some(false),
            reasoning_levels: None,
        }],
    })?;
    isolated.set_key("p1")?;

    isolated.apply(
        "p1",
        ProviderAppId::Grokbuild,
        &["model-a"],
        "model-a",
        None,
    )?;
    let content = read_text(&grok_path());
    assert!(!content.contains("context_window"));
    assert!(!content.contains("supports_reasoning_effort"));
    assert!(!content.contains("max_completion_tokens"));
    Ok(())
}

// 应用弹窗的「不写入的模型元数据」清单来自能力表：四个工具都写窗口，Claude
// 另写最大输出，OpenCode 与 Pi 四类全覆盖，Grok 缺最大输出与图像输入。
#[test]
fn app_states_declare_unwritten_model_fields() -> Result<()> {
    let isolated = Isolated::new()?;
    let state = isolated.state()?;
    let fields = |app: ProviderAppId| {
        state
            .apps
            .iter()
            .find(|item| item.app == app)
            .expect("app state")
            .unwritten_model_fields
            .clone()
    };

    assert_eq!(
        fields(ProviderAppId::Grokbuild),
        vec!["最大输出", "图像输入"]
    );
    assert_eq!(
        fields(ProviderAppId::Codex),
        vec!["最大输出", "图像输入", "推理能力"]
    );
    assert_eq!(fields(ProviderAppId::Claude), vec!["图像输入", "推理能力"]);
    assert!(fields(ProviderAppId::Opencode).is_empty());
    assert!(fields(ProviderAppId::Pi).is_empty());
    Ok(())
}

// Grok 的 messages 后端固定请求 {base_url}/messages 不补 /v1（grok 1.0.41
// 实测）：providers.yaml 按 anthropic 生态习惯不带 /v1 时，写入补 /v1，
// 且反读按同一口径比对、不误判漂移。
#[test]
fn grok_anthropic_base_url_gets_v1_suffix_and_inspect_stays_applied() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_provider(
        "p1",
        ProviderProtocol::AnthropicMessages,
        "https://p1.test/anthropic",
    )?;
    isolated.set_key("p1")?;

    isolated.apply(
        "p1",
        ProviderAppId::Grokbuild,
        &["model-a"],
        "model-a",
        None,
    )?;
    let content = read_text(&grok_path());
    assert!(content.contains("base_url = \"https://p1.test/anthropic/v1\""));

    let state = isolated.state()?;
    let grok = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Grokbuild)
        .expect("grok state");
    assert_eq!(grok.entries[0].status, ProviderAppEntryStatus::Applied);
    Ok(())
}

// ---------------------------------------------------------------------------
// 漂移、协议门控、平台删除、预览脱敏
// ---------------------------------------------------------------------------

#[test]
fn drifted_base_url_is_reported_and_protocol_gate_blocks_apply() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;

    // 平台 base_url 变化 → 反显为漂移。
    isolated.seed_model("p1", "model-a")?;
    isolated.store.upsert(provider_input(
        "p1",
        ProviderProtocol::OpenaiResponses,
        "https://p1.test/v2",
    ))?;
    let state = isolated.state()?;
    let codex = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Codex)
        .unwrap();
    assert_eq!(codex.entries[0].status, ProviderAppEntryStatus::Drifted);

    // 协议不兼容：chat_completions 平台不能应用到 Codex。
    isolated.seed_provider(
        "p2",
        ProviderProtocol::OpenaiChatCompletions,
        "https://p2.test/v1",
    )?;
    isolated.set_key("p2")?;
    let error = apply_provider_inner(
        &isolated.store,
        &isolated.keys,
        &isolated.env,
        &plan("p2", ProviderAppId::Codex, &["model-a"], "model-a", None),
    )
    .expect_err("协议不兼容应报错");
    assert!(error.to_string().contains("不支持"));
    Ok(())
}

#[test]
fn delete_provider_blocked_by_claude_reference_then_succeeds() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_provider(
        "agg",
        ProviderProtocol::AnthropicMessages,
        "https://agg.test/api",
    )?;
    isolated.set_key("agg")?;
    isolated.apply("agg", ProviderAppId::Claude, &["model-a"], "model-a", None)?;

    let error = delete_provider_inner(&isolated.store, &isolated.keys, &isolated.env, "agg")
        .expect_err("Claude 引用中的平台不应允许删除");
    assert!(error.to_string().contains("Claude Code"));

    isolated.remove_from("agg", ProviderAppId::Claude)?;
    let result = delete_provider_inner(&isolated.store, &isolated.keys, &isolated.env, "agg")?;
    assert!(result.detail.contains("已删除"));
    assert!(
        providers_state_inner(&isolated.store, &isolated.keys, &isolated.env)?
            .providers
            .is_empty()
    );
    // 密钥也一并清理。
    assert!(!isolated.keys.key_present("agg")?);
    Ok(())
}

#[test]
fn preview_masks_key_and_does_not_write() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    let preview = preview_apply_inner(
        &isolated.store,
        &isolated.keys,
        &isolated.env,
        &plan("p1", ProviderAppId::Codex, &["model-a"], "model-a", None),
    )?;
    let rendered = preview
        .files
        .iter()
        .map(|file| {
            file.diff
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains(MASKED_KEY));
    assert!(!rendered.contains("sk-test-secret"));
    assert!(!codex_path().exists(), "预览不应写文件");

    // 缺 Key 时预览直接报错。
    isolated.keys.remove_key("p1")?;
    assert!(
        preview_apply_inner(
            &isolated.store,
            &isolated.keys,
            &isolated.env,
            &plan("p1", ProviderAppId::Codex, &["model-a"], "model-a", None),
        )
        .is_err()
    );
    Ok(())
}

// 先 apply 再 preview：旧文件带着上次写入的真实密钥，diff 的
// Context/Remove 行也只允许出现掩码。
#[test]
fn preview_after_apply_masks_secret_in_old_file() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.set_key("p1")?;

    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    let raw = std::fs::read_to_string(codex_path())?;
    assert!(raw.contains("sk-test-secret"), "apply 应写入真实密钥");

    let preview = preview_apply_inner(
        &isolated.store,
        &isolated.keys,
        &isolated.env,
        &plan("p1", ProviderAppId::Codex, &["model-a"], "model-a", None),
    )?;
    let rendered = preview
        .files
        .iter()
        .map(|file| {
            file.diff
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered.contains("sk-test-secret"));
    assert!(rendered.contains(MASKED_KEY));
    Ok(())
}

#[test]
fn diff_lines_marks_add_remove_context() -> Result<()> {
    use crate::providers::apps::diff_lines;
    use crate::providers::types::DiffLineKind;

    let diff = diff_lines("a\nb\nc\n", "a\nx\nc\nd\n");
    let marks: Vec<(DiffLineKind, &str)> = diff
        .iter()
        .map(|line| (line.kind, line.text.as_str()))
        .collect();
    assert!(marks.contains(&(DiffLineKind::Context, "a")));
    assert!(marks.contains(&(DiffLineKind::Remove, "b")));
    assert!(marks.contains(&(DiffLineKind::Add, "x")));
    assert!(marks.contains(&(DiffLineKind::Context, "c")));
    assert!(marks.contains(&(DiffLineKind::Add, "d")));
    Ok(())
}

#[test]
fn registration_key_uses_reins_prefix() -> Result<()> {
    assert_eq!(registration_key("openrouter"), "reins-openrouter");
    Ok(())
}

// ---------------------------------------------------------------------------
// External entries
// ---------------------------------------------------------------------------

#[test]
fn codex_remove_external_clears_active_provider() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = codex_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        "model_provider = \"my-gateway\"\nmodel_reasoning_effort = \"low\"\nmodel = \"m1\"\n\
         [model_providers.my-gateway]\nname = \"Gateway\"\nbase_url = \"https://gw.test/v1\"\n",
    )?;

    // 活动 Provider 仍指向该条目时随删除清空；model 是裸模型名，保持不动。
    isolated.remove_external(ProviderAppId::Codex, "my-gateway")?;

    let content = read_text(&path);
    assert!(!content.contains("my-gateway"));
    let root: TomlValue = toml::from_str(&content)?;
    assert!(root.get("model_provider").is_none());
    assert!(root.get("model_reasoning_effort").is_none());
    assert_eq!(root.get("model").and_then(TomlValue::as_str), Some("m1"));
    Ok(())
}

#[test]
fn opencode_remove_external_clears_default_model() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = opencode_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        json!({
            "model": "my-gateway/m1",
            "provider": {
                "my-gateway": {
                    "npm": "@ai-sdk/openai",
                    "options": { "baseURL": "https://gw.test/v1" }
                }
            }
        })
        .to_string(),
    )?;

    // 默认模型指向被删条目时随删除清空。
    isolated.remove_external(ProviderAppId::Opencode, "my-gateway")?;

    let removed: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert!(removed["provider"].get("my-gateway").is_none());
    assert!(removed.get("model").is_none());
    Ok(())
}

#[test]
fn pi_remove_external_clears_default_provider() -> Result<()> {
    let isolated = Isolated::new()?;
    let models = pi_models_path();
    let settings = pi_settings_path();
    fs::create_dir_all(models.parent().unwrap())?;
    fs::write(
        &models,
        json!({
            "providers": {
                "my-gateway": {
                    "api": "openai-completions",
                    "baseUrl": "https://gw.test/v1",
                    "apiKey": "sk-test"
                }
            }
        })
        .to_string(),
    )?;
    fs::write(
        &settings,
        json!({"defaultProvider": "my-gateway"}).to_string(),
    )?;

    // 默认 Provider 指向被删条目时随删除清空。
    isolated.remove_external(ProviderAppId::Pi, "my-gateway")?;

    let content = read_text(&models);
    assert!(!content.contains("my-gateway"));
    let settings: JsonValue = serde_json::from_str(&read_text(&settings))?;
    assert!(settings.get("defaultProvider").is_none());
    Ok(())
}

#[test]
fn grok_remove_external_removes_models_and_clears_default() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = grok_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        "models.default = \"my-gateway--m1\"\n\
         [model_providers.my-gateway]\nname = \"Gateway\"\nbase_url = \"https://gw.test/v1\"\n\
         [model.my-gateway--m1]\nmodel_provider = \"my-gateway\"\nmodel = \"m1\"\n\
         [model.other--m]\nmodel_provider = \"other\"\nmodel = \"m2\"\n",
    )?;

    // 默认模型仍引用该条目时随删除清空。
    isolated.remove_external(ProviderAppId::Grokbuild, "my-gateway")?;

    let content = read_text(&path);
    assert!(!content.contains("my-gateway"));
    assert!(content.contains("other--m"));
    let root: TomlValue = toml::from_str(&content)?;
    assert!(
        root.get("models")
            .and_then(|models| models.get("default"))
            .is_none()
    );
    Ok(())
}

#[test]
fn claude_remove_external_unsupported() -> Result<()> {
    let isolated = Isolated::new()?;
    // Claude 外部配置内嵌在 env 中，没有可结构化删除的条目。
    assert!(
        isolated
            .remove_external(ProviderAppId::Claude, "env")
            .is_err()
    );
    Ok(())
}
