// providers 域端到级行为测试：适配器「生成 → 反读 → 幂等 → 移除」闭环，
// 全部在虚拟 HOME 隔离目录下进行，密钥明文写在 providers.yaml 里。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value as JsonValue, json};
use toml::Value as TomlValue;

use crate::providers::apps::{MASKED_KEY, ToolEnv, registration_key};
use crate::providers::commands::{
    app_state_inner, apply_provider_inner, delete_provider_inner, preview_apply_inner,
    providers_state_inner, remove_external_entry_inner, remove_provider_from_app_inner,
};
use crate::providers::config::ProviderConfigStore;
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
    env: ToolEnv,
}

impl Isolated {
    fn new() -> Result<Self> {
        let data_dir = TestDir::new("providers-data")?;
        let home_dir = TestDir::new("providers-home")?;
        let guard = TestEnvGuard::set_home(home_dir.path());
        // 先取路径再移动 TestDir，字段求值顺序不保证可用性。
        let data_path = data_dir.path().to_path_buf();
        Ok(Self {
            _guard: guard,
            _data_dir: data_dir,
            _home_dir: home_dir,
            store: ProviderConfigStore::at(data_path),
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
            api_key: "sk-test-secret".to_string(),
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

    // 走产品同一条写路径：upsert 留空即清除已存 Key。因为 upsert 是整条替换，
    // 这里用 provider_input 的单模型夹具重建输入，只适用于 seed_model 造出的平台。
    fn clear_key(&self, provider_id: &str) -> Result<()> {
        let providers = self.store.load()?;
        let provider = providers.get(provider_id).expect("平台应先 seed");
        self.store.upsert(ProviderUpsertInput {
            api_key: String::new(),
            ..provider_input(provider_id, provider.protocol, &provider.base_url)
        })?;
        Ok(())
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
        providers_state_inner(&self.store, &self.env)
    }
}

fn provider_input(id: &str, protocol: ProviderProtocol, base_url: &str) -> ProviderUpsertInput {
    ProviderUpsertInput {
        provider_id: id.to_string(),
        label: format!("Label {id}"),
        protocol,
        base_url: base_url.to_string(),
        api_key: "sk-test-secret".to_string(),
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

fn opencode_jsonc_path() -> PathBuf {
    opencode_path().with_file_name("opencode.jsonc")
}

fn codex_catalog_path() -> PathBuf {
    codex_path().with_file_name("reins-models.json")
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

// 同一平台重新应用（例如只换默认模型）不选思考等级时保留已写入的值；
// 换到另一个平台才清掉，避免旧平台的等级留在唯一的活动配置上。
#[test]
fn codex_reapply_keeps_reasoning_effort_but_switch_clears_it() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.seed_model("p2", "model-a")?;

    isolated.apply(
        "p1",
        ProviderAppId::Codex,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    assert!(read_text(&codex_path()).contains("model_reasoning_effort = \"high\""));

    isolated.apply("p2", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    assert!(!read_text(&codex_path()).contains("model_reasoning_effort"));
    Ok(())
}

// 聚合模型的窗口写入顶层 model_context_window：codex 对不在内置目录的模型
// 会走兜底元数据（272000）并告警 Unknown model。
#[test]
fn codex_apply_writes_and_clears_model_context_window() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;

    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    assert!(read_text(&codex_path()).contains("model_context_window = 200000"));

    // 元数据清空后替换应用不再写该键，避免残留旧窗口。
    isolated.store.upsert(ProviderUpsertInput {
        provider_id: "p1".to_string(),
        label: "Label p1".to_string(),
        protocol: ProviderProtocol::OpenaiResponses,
        base_url: "https://p1.test/v1".to_string(),
        api_key: "sk-test-secret".to_string(),
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
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;

    isolated.remove_from("p1", ProviderAppId::Codex)?;
    assert!(!read_text(&codex_path()).contains("model_context_window"));
    Ok(())
}

// model_catalog_json + reins-models.json：条目带齐 codex 0.145.0 必填字段
// （缺 supports_parallel_tool_calls 等无默认字段会让配置加载整体失败，
// 隔离环境实测过），base_instructions 用内置兜底提示词，visibility=list
// 让模型进选择器。
#[test]
fn codex_apply_writes_model_catalog_and_remove_clears_pointer() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;

    isolated.apply(
        "p1",
        ProviderAppId::Codex,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    let config: TomlValue = toml::from_str(&read_text(&codex_path()))?;
    let catalog_path = codex_catalog_path();
    assert_eq!(
        config
            .get("model_catalog_json")
            .and_then(TomlValue::as_str),
        Some(catalog_path.display().to_string().as_str())
    );
    let catalog: JsonValue = serde_json::from_str(&read_text(&catalog_path))?;
    let entry = &catalog["models"][0];
    assert_eq!(entry["slug"], "model-a");
    assert_eq!(entry["visibility"], "list");
    assert_eq!(entry["supported_in_api"], true);
    assert_eq!(entry["shell_type"], "default");
    assert_eq!(entry["supports_parallel_tool_calls"], false);
    assert_eq!(entry["truncation_policy"]["mode"], "bytes");
    assert_eq!(entry["context_window"], 200_000);
    assert_eq!(entry["input_modalities"][1], "image");
    assert_eq!(entry["default_reasoning_level"], "high");
    // base_instructions 非空：空提示词会让会话直接失焦。
    assert!(entry["base_instructions"].as_str().unwrap().len() > 100);

    // 移除平台：指针随条目清掉；目录文件不再被引用，留在原地无害。
    isolated.remove_from("p1", ProviderAppId::Codex)?;
    assert!(!read_text(&codex_path()).contains("model_catalog_json"));
    assert!(catalog_path.exists());
    Ok(())
}

// 用户自己的 model_catalog_json 指向不归 Reins 管，移除时不动。
#[test]
fn codex_remove_keeps_user_model_catalog_json() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;

    let path = codex_path();
    let mut edited: TomlValue = toml::from_str(&read_text(&path))?;
    edited
        .as_table_mut()
        .unwrap()
        .insert("model_catalog_json".to_string(), TomlValue::String("C:/custom/models.json".to_string()));
    fs::write(&path, toml::to_string(&edited)?)?;

    isolated.remove_from("p1", ProviderAppId::Codex)?;
    let after: TomlValue = toml::from_str(&read_text(&codex_path()))?;
    assert_eq!(
        after.get("model_catalog_json").and_then(TomlValue::as_str),
        Some("C:/custom/models.json")
    );
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
    assert_eq!(applied["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-test-secret");
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

// 认证键为 ANTHROPIC_AUTH_TOKEN（Bearer 头）；旧版写过的 ANTHROPIC_API_KEY
// 随重新应用清理（两个凭据头并存会被部分网关拒绝），反读时给出提示。
#[test]
fn claude_apply_cleans_legacy_api_key_and_notes_it() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = claude_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        json!({"env": {"ANTHROPIC_API_KEY": "old-key"}}).to_string(),
    )?;
    isolated.seed_provider(
        "agg",
        ProviderProtocol::AnthropicMessages,
        "https://agg.test/api",
    )?;

    isolated.apply("agg", ProviderAppId::Claude, &["model-a"], "model-a", None)?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(applied["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-test-secret");
    assert!(applied["env"].get("ANTHROPIC_API_KEY").is_none());

    // 手工塞回遗留键 → 反读出现提示；移除时两个认证键一并清理。
    let with_legacy: JsonValue = serde_json::from_str(&read_text(&path))?;
    let mut env = with_legacy["env"].clone();
    env["ANTHROPIC_API_KEY"] = json!("leftover");
    let mut root = with_legacy.clone();
    root["env"] = env;
    fs::write(&path, serde_json::to_string(&root)?)?;
    let state = isolated.state()?;
    let claude = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Claude)
        .unwrap();
    assert!(claude.entries[0]
        .notes
        .iter()
        .any(|note| note.contains("ANTHROPIC_API_KEY")));

    isolated.remove_from("agg", ProviderAppId::Claude)?;
    let removed: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert!(removed.get("env").is_none());
    Ok(())
}

// CLAUDE_CONFIG_DIR 重定位配置目录（官方支持），与 CODEX_HOME 等同一模式。
#[test]
fn claude_honors_claude_config_dir() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_provider(
        "agg",
        ProviderProtocol::AnthropicMessages,
        "https://agg.test/api",
    )?;
    let custom_dir = TestDir::new("claude-config-dir")?;
    let env = ToolEnv {
        claude_config_dir: Some(custom_dir.path().to_path_buf()),
        ..ToolEnv::default()
    };

    apply_provider_inner(
        &isolated.store,
        &env,
        &plan("agg", ProviderAppId::Claude, &["model-a"], "model-a", None),
    )?;

    let settings = custom_dir.path().join("settings.json");
    let applied: JsonValue = serde_json::from_str(&read_text(&settings))?;
    assert_eq!(applied["env"]["ANTHROPIC_BASE_URL"], "https://agg.test/api");
    // 默认路径不受影响：未在 ~/.claude 下创建文件。
    assert!(!claude_path().exists());
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

// OpenCode 重新应用时未选思考等级：沿用该模型上一次写入的 settings，
// 只换默认模型不会把思考设置写没。
#[test]
fn opencode_reapply_keeps_reasoning_settings() -> Result<()> {
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

    isolated.apply(
        "p1",
        ProviderAppId::Opencode,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    // 反读按同一字段还原等级。
    let state = isolated.state()?;
    let opencode = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Opencode)
        .expect("opencode state");
    assert_eq!(opencode.default_reasoning_level, Some(ReasoningLevel::High));

    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let reapplied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(
        reapplied["providers"]["reins-p1"]["models"]["model-a"]["settings"]["reasoningEffort"],
        "high"
    );
    Ok(())
}

// anthropic 包的思考设置是 thinking 预算，反读要按同一阶梯还原成等级。
#[test]
fn opencode_reads_back_thinking_budget_level() -> Result<()> {
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

    isolated.apply(
        "p1",
        ProviderAppId::Opencode,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::High),
    )?;
    let applied: JsonValue = serde_json::from_str(&read_text(&path))?;
    assert_eq!(
        applied["providers"]["reins-p1"]["models"]["model-a"]["settings"]["thinking"]["budgetTokens"],
        16384
    );

    let state = isolated.state()?;
    let opencode = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Opencode)
        .expect("opencode state");
    assert_eq!(opencode.default_reasoning_level, Some(ReasoningLevel::High));
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
        api_key: "sk-test-secret".to_string(),
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

// 只有 opencode.jsonc（带注释）时：反读可见、apply 写进同一文件，不再
// 另建 opencode.json。
#[test]
fn opencode_reads_and_writes_jsonc_when_only_jsonc_exists() -> Result<()> {
    let isolated = Isolated::new()?;
    let jsonc = opencode_jsonc_path();
    fs::create_dir_all(jsonc.parent().unwrap())?;
    fs::write(
        &jsonc,
        "// 用户注释\n{\"providers\":{\"manual\":{\"name\":\"Manual\",\"package\":\"@opencode/ai/providers/openai-compatible\",\"settings\":{\"baseURL\":\"https://manual.test/v1\"}}}}",
    )?;
    isolated.seed_model("p1", "model-a")?;

    let state = isolated.state()?;
    let opencode = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Opencode)
        .expect("opencode state");
    assert_eq!(opencode.entries.len(), 1);
    assert_eq!(opencode.entries[0].key, "manual");
    assert_eq!(opencode.entries[0].status, ProviderAppEntryStatus::External);

    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let written: JsonValue = serde_json::from_str(&read_text(&jsonc))?;
    assert_eq!(written["model"], "reins-p1/model-a");
    assert!(written["providers"]["reins-p1"].is_object());
    // 未另建 opencode.json。
    assert!(!opencode_path().exists());
    Ok(())
}

// 两个文件并存：.jsonc 顶层键整键覆盖 .json；apply 目标选 .jsonc，并把
// 留在 .json 里的旧注册键清掉，避免同一 reins- 条目散在两个文件。
#[test]
fn opencode_jsonc_takes_precedence_and_apply_migrates_registration() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    // 先只在 opencode.json 里应用一次。
    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;

    // 用户随后创建了 opencode.jsonc（只定义 providers 键）。
    let jsonc = opencode_jsonc_path();
    fs::write(
        &jsonc,
        json!({"providers": {"manual": {
            "name": "Manual",
            "package": "@opencode/ai/providers/openai-compatible",
            "settings": {"baseURL": "https://manual.test/v1"}
        }}})
        .to_string(),
    )?;

    // 反读：.json 里的条目标注被覆盖，不生效。
    let state = isolated.state()?;
    let opencode = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Opencode)
        .expect("opencode state");
    let shadowed = opencode
        .entries
        .iter()
        .find(|entry| entry.key == "reins-p1")
        .expect("reins-p1 in opencode.json");
    assert!(shadowed
        .notes
        .iter()
        .any(|note| note.contains("opencode.jsonc")));

    // 再次应用：写入 .jsonc，.json 里的旧键清掉。
    isolated.apply("p1", ProviderAppId::Opencode, &["model-a"], "model-a", None)?;
    let json_root: JsonValue = serde_json::from_str(&read_text(&opencode_path()))?;
    let jsonc_root: JsonValue = serde_json::from_str(&read_text(&jsonc))?;
    assert!(json_root.get("providers").and_then(|p| p.get("reins-p1")).is_none());
    assert!(jsonc_root["providers"]["reins-p1"].is_object());
    assert_eq!(jsonc_root["model"], "reins-p1/model-a");

    // 移除：从 .jsonc 清掉注册键与默认模型。
    isolated.remove_from("p1", ProviderAppId::Opencode)?;
    let after: JsonValue = serde_json::from_str(&read_text(&jsonc))?;
    assert!(after.get("providers").and_then(|p| p.get("reins-p1")).is_none());
    assert!(after.get("model").is_none());
    Ok(())
}

// ---------------------------------------------------------------------------
// Pi
// ---------------------------------------------------------------------------

#[test]
fn pi_apply_writes_models_and_settings_then_remove_clears_defaults() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;

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

    isolated.apply(
        "p1",
        ProviderAppId::Pi,
        &["model-a"],
        "model-a",
        Some(ReasoningLevel::Off),
    )?;
    let settings: JsonValue = serde_json::from_str(&read_text(&pi_settings_path()))?;
    assert_eq!(settings["defaultThinkingLevel"], "off");

    // 反读要把 "off" 还原成 off，否则应用弹窗预选不出来。
    let state = isolated.state()?;
    let pi = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Pi)
        .expect("pi state");
    assert_eq!(pi.default_reasoning_level, Some(ReasoningLevel::Off));
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

// 多提供商并存：重新应用已应用的平台只重写自己的模型条目与默认值，另一个
// 平台的条目保留；界面上的「切换默认模型」正是走这条路径。
#[test]
fn grok_reapply_switches_default_and_keeps_other_provider() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_provider_with_models(
        "p1",
        ProviderProtocol::OpenaiResponses,
        "https://p1.test/v1",
        vec!["m1".to_string(), "m2".to_string()],
    )?;
    isolated.seed_provider_with_models(
        "p2",
        ProviderProtocol::OpenaiResponses,
        "https://p2.test/v1",
        vec!["m3".to_string()],
    )?;

    isolated.apply("p1", ProviderAppId::Grokbuild, &["m1"], "m1", None)?;
    isolated.apply("p2", ProviderAppId::Grokbuild, &["m3"], "m3", None)?;
    assert!(read_text(&grok_path()).contains("default = \"reins-p2--m3\""));

    // 重新应用 p1 并把默认模型换成自己的另一个模型。
    isolated.apply("p1", ProviderAppId::Grokbuild, &["m1", "m2"], "m2", None)?;
    let content = read_text(&grok_path());
    assert!(content.contains("default = \"reins-p1--m2\""));
    assert!(content.contains("[model_providers.reins-p2]"));
    assert!(content.contains("reins-p2--m3"));

    let state = isolated.state()?;
    let grok = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Grokbuild)
        .expect("grok state");
    assert_eq!(grok.entries.len(), 2, "{:?}", grok.entries);
    let p1 = grok
        .entries
        .iter()
        .find(|entry| entry.key == "reins-p1")
        .expect("p1 条目");
    assert_eq!(p1.status, ProviderAppEntryStatus::Applied);
    assert_eq!(p1.model_ids, vec!["m1", "m2"]);
    assert_eq!(p1.default_model_id.as_deref(), Some("m2"));
    let p2 = grok
        .entries
        .iter()
        .find(|entry| entry.key == "reins-p2")
        .expect("p2 条目");
    assert_eq!(p2.default_model_id, None);
    Ok(())
}

// 选「最高 (max)」写出 xhigh：Grok UI 最高档是 xhigh 且请求层两者等价。
#[test]
fn grok_max_reasoning_writes_xhigh() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
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
        api_key: "sk-test-secret".to_string(),
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

// 反读当前默认思考等级：应用弹窗据此预选，重新应用才不会悄悄改掉它。
#[test]
fn applied_reasoning_level_is_read_back_for_each_app() -> Result<()> {
    for (app, protocol) in [
        (ProviderAppId::Codex, ProviderProtocol::OpenaiResponses),
        (ProviderAppId::Claude, ProviderProtocol::AnthropicMessages),
        (
            ProviderAppId::Opencode,
            ProviderProtocol::OpenaiChatCompletions,
        ),
        (ProviderAppId::Pi, ProviderProtocol::OpenaiChatCompletions),
        (
            ProviderAppId::Grokbuild,
            ProviderProtocol::OpenaiChatCompletions,
        ),
    ] {
        let isolated = Isolated::new()?;
        isolated.seed_provider("p1", protocol, "https://p1.test/v1")?;
        isolated.apply(
            "p1",
            app,
            &["model-a"],
            "model-a",
            Some(ReasoningLevel::High),
        )?;

        let state = isolated.state()?;
        let app_state = state
            .apps
            .iter()
            .find(|item| item.app == app)
            .expect("app state");
        assert_eq!(
            app_state.default_reasoning_level,
            Some(ReasoningLevel::High),
            "{app:?}"
        );
    }
    Ok(())
}

// 定点反显（写操作后按工具刷新）与全量状态里的同一张卡片一致。
#[test]
fn app_state_inner_matches_full_state_entry() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;
    isolated.apply(
        "p1",
        ProviderAppId::Grokbuild,
        &["model-a"],
        "model-a",
        None,
    )?;

    let full = isolated.state()?;
    let expected = full
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Grokbuild)
        .expect("grok state");
    let single = app_state_inner(&isolated.store, &isolated.env, ProviderAppId::Grokbuild)?;
    assert_eq!(
        serde_json::to_value(&single)?,
        serde_json::to_value(expected)?
    );
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
    let error = apply_provider_inner(
        &isolated.store,
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
    isolated.apply("agg", ProviderAppId::Claude, &["model-a"], "model-a", None)?;

    let error = delete_provider_inner(&isolated.store, &isolated.env, "agg")
        .expect_err("Claude 引用中的平台不应允许删除");
    assert!(error.to_string().contains("Claude Code"));

    isolated.remove_from("agg", ProviderAppId::Claude)?;
    let result = delete_provider_inner(&isolated.store, &isolated.env, "agg")?;
    assert!(result.detail.contains("已删除"));
    assert!(
        providers_state_inner(&isolated.store, &isolated.env)?
            .providers
            .is_empty()
    );
    // 明文 Key 随平台条目一起从文件里消失。
    assert!(!fs::read_to_string(isolated.store.config_path())?.contains("sk-test-secret"));
    Ok(())
}

#[test]
fn preview_masks_key_and_does_not_write() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_model("p1", "model-a")?;

    let preview = preview_apply_inner(
        &isolated.store,
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
    isolated.clear_key("p1")?;
    assert!(
        preview_apply_inner(
            &isolated.store,
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

    isolated.apply("p1", ProviderAppId::Codex, &["model-a"], "model-a", None)?;
    let raw = std::fs::read_to_string(codex_path())?;
    assert!(raw.contains("sk-test-secret"), "apply 应写入真实密钥");

    let preview = preview_apply_inner(
        &isolated.store,
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

// 用户手工写的官方简化式 [model.<键>]（自带 base_url、不挂 model_provider）
// 不落在 model_providers 里，按模型键反显为外部配置；reins- 模型条目仍归入
// 对应 Provider 条目，不重复列出。
#[test]
fn grok_standalone_model_entry_shows_as_external() -> Result<()> {
    let isolated = Isolated::new()?;
    isolated.seed_provider(
        "acme",
        ProviderProtocol::OpenaiResponses,
        "https://gw.test/v1",
    )?;
    let path = grok_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        "[model.\"my-model\"]\n\
         api_backend = \"responses\"\napi_key = \"sk-test-secret\"\n\
         base_url = \"https://gw.test/v1\"\nmodel = \"chat-a\"\nname = \"My Model\"\n\
         [model.\"reins-acme--chat-b\"]\n\
         model = \"chat-b\"\nmodel_provider = \"reins-acme\"\n\
         [model_providers.reins-acme]\nbase_url = \"https://gw.test/v1\"\napi_backend = \"responses\"\n\
         [models]\ndefault = \"reins-acme--chat-b\"\n",
    )?;

    let state = isolated.state()?;
    let grok = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Grokbuild)
        .expect("grok state");
    assert_eq!(grok.entries.len(), 2, "{:?}", grok.entries);
    let applied = grok
        .entries
        .iter()
        .find(|entry| entry.key == "reins-acme")
        .expect("reins 条目");
    assert_eq!(applied.status, ProviderAppEntryStatus::Applied);
    assert_eq!(applied.model_ids, vec!["chat-b"]);
    assert_eq!(applied.default_model_id.as_deref(), Some("chat-b"));
    let standalone = grok
        .entries
        .iter()
        .find(|entry| entry.key == "my-model")
        .expect("独立模型条目");
    assert_eq!(standalone.status, ProviderAppEntryStatus::External);
    assert_eq!(standalone.base_url.as_deref(), Some("https://gw.test/v1"));
    assert_eq!(standalone.protocol, Some(ProviderProtocol::OpenaiResponses));
    Ok(())
}

// 挂在外部 Provider 条目下的模型条目由该 Provider 条目代表，不单独列为外部条目。
#[test]
fn grok_model_mounted_on_provider_is_not_listed_separately() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = grok_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        "[model_providers.my-gateway]\nbase_url = \"https://gw.test/v1\"\n\
         [model.my-gateway--m1]\nmodel_provider = \"my-gateway\"\nmodel = \"m1\"\n",
    )?;

    let state = isolated.state()?;
    let grok = state
        .apps
        .iter()
        .find(|app| app.app == ProviderAppId::Grokbuild)
        .expect("grok state");
    assert_eq!(grok.entries.len(), 1, "{:?}", grok.entries);
    assert_eq!(grok.entries[0].key, "my-gateway");
    assert_eq!(grok.entries[0].status, ProviderAppEntryStatus::External);
    Ok(())
}

// 独立模型条目按模型键结构化删除；被 [models].default 引用时一并清默认值，
// 其余模型键与 Provider 条目不受影响。
#[test]
fn grok_remove_external_removes_standalone_model_and_clears_default() -> Result<()> {
    let isolated = Isolated::new()?;
    let path = grok_path();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        "models.default = \"standalone-a\"\nmodels.default_reasoning_effort = \"high\"\n\
         [model.\"standalone-a\"]\nbase_url = \"https://gw.test/v1\"\nmodel = \"chat-a\"\n\
         [model.\"standalone-b\"]\napi_key = \"sk-test-secret\"\n\
         [model_providers.my-gateway]\nbase_url = \"https://gw.test/v1\"\n",
    )?;

    isolated.remove_external(ProviderAppId::Grokbuild, "standalone-a")?;

    let content = read_text(&path);
    assert!(!content.contains("standalone-a"));
    assert!(content.contains("standalone-b"));
    assert!(content.contains("my-gateway"));
    let root: TomlValue = toml::from_str(&content)?;
    let models = root.get("models").and_then(TomlValue::as_table);
    assert!(models.is_some_and(|models| !models.contains_key("default")));
    assert!(models.is_some_and(|models| !models.contains_key("default_reasoning_effort")));
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
