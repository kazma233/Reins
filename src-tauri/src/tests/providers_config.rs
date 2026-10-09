// providers.yaml 存储层测试：往返、校验、CRUD。

use std::fs;

use anyhow::Result;

use crate::providers::config::ProviderConfigStore;
use crate::providers::types::{
    ProviderModelInput, ProviderProtocol, ProviderUpsertInput, ProviderWriteMode, ReasoningLevel,
};
use crate::test_support::TestDir;

fn input(id: &str) -> ProviderUpsertInput {
    ProviderUpsertInput {
        provider_id: id.to_string(),
        mode: ProviderWriteMode::Upsert,
        label: format!("Label {id}"),
        protocol: ProviderProtocol::OpenaiChatCompletions,
        base_url: format!("https://{id}.test/v1"),
        api_key: "sk-test-secret".to_string(),
        models: vec![ProviderModelInput {
            id: "model-a".to_string(),
            label: "Model A".to_string(),
            context_window: Some(128_000),
            max_output_tokens: None,
            supports_images: Some(true),
            reasoning: Some(true),
            reasoning_levels: Some(vec![
                ReasoningLevel::High,
                ReasoningLevel::Low,
                ReasoningLevel::Low,
            ]),
        }],
    }
}

#[test]
fn upsert_normalizes_and_round_trips() -> Result<()> {
    let dir = TestDir::new("providers-store")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());

    let id = store.upsert(input("OpenRouter"))?;
    assert_eq!(id, "openrouter");

    let providers = store.load()?;
    let provider = providers.get("openrouter").expect("provider exists");
    assert_eq!(provider.label, "Label OpenRouter");
    assert_eq!(provider.models.len(), 1);
    // 思考等级去重 + 排序。
    assert_eq!(
        provider.models[0].reasoning_levels,
        Some(vec![ReasoningLevel::Low, ReasoningLevel::High])
    );

    // 落盘内容是 snake_case YAML，且带版本号。
    let raw = fs::read_to_string(store.config_path())?;
    assert!(raw.contains("version: 1"));
    assert!(raw.contains("context_window: 128000"));
    assert!(raw.contains("reasoning_levels:"));
    // API Key 明文落盘（用户确认的取舍，不加密）。
    assert_eq!(provider.stored_api_key().as_deref(), Some("sk-test-secret"));
    assert!(raw.contains("api_key: sk-test-secret"));
    Ok(())
}

// 表单值即落盘值：留空写入即清除 Key，两侧空白不计入。
#[test]
fn upsert_overwrites_and_clears_api_key() -> Result<()> {
    let dir = TestDir::new("providers-store")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());
    store.upsert(input("p1"))?;

    let mut renamed = input("p1");
    renamed.label = "Renamed".to_string();
    store.upsert(renamed)?;
    assert_eq!(
        store.load()?["p1"].stored_api_key().as_deref(),
        Some("sk-test-secret"),
        "元数据更新不应清掉已存 Key"
    );

    let mut blank = input("p1");
    blank.api_key = "   ".to_string();
    store.upsert(blank)?;
    assert_eq!(store.load()?["p1"].stored_api_key(), None);
    assert!(!fs::read_to_string(store.config_path())?.contains("api_key"));
    Ok(())
}

#[test]
fn upsert_rejects_invalid_input() -> Result<()> {
    let dir = TestDir::new("providers-store")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());

    let mut bad = input("ok-id");
    bad.provider_id = "reins-x".to_string();
    assert!(store.upsert(bad).is_err(), "reins- 前缀平台 ID 应被拒绝");

    let mut bad = input("bad id");
    assert!(store.upsert(bad.clone()).is_err(), "带空格的 ID 应被拒绝");

    bad.provider_id = "ok-id".to_string();
    bad.base_url = "ftp://nope".to_string();
    assert!(store.upsert(bad).is_err(), "非 http(s) URL 应被拒绝");

    let mut bad = input("dup-model");
    bad.models.push(ProviderModelInput {
        id: "model-a".to_string(),
        label: String::new(),
        context_window: None,
        max_output_tokens: None,
        supports_images: None,
        reasoning: None,
        reasoning_levels: None,
    });
    assert!(store.upsert(bad).is_err(), "重复模型 ID 应被拒绝");
    Ok(())
}

#[test]
fn delete_removes_entry() -> Result<()> {
    let dir = TestDir::new("providers-store")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());
    store.upsert(input("p1"))?;
    store.upsert(input("p2"))?;

    store.delete("p1")?;
    let providers = store.load()?;
    assert!(providers.contains_key("p2"));
    assert!(!providers.contains_key("p1"));
    assert!(store.delete("p1").is_err());
    Ok(())
}

#[test]
fn missing_file_loads_as_empty() -> Result<()> {
    let dir = TestDir::new("providers-store")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());
    assert!(store.load()?.is_empty());
    assert!(!store.config_path().exists());
    Ok(())
}

#[test]
fn create_mode_rejects_an_existing_provider() -> Result<()> {
    let dir = TestDir::new("providers-create-conflict")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());
    store.upsert(input("p1"))?;

    let mut create = input("p1");
    create.mode = ProviderWriteMode::Create;
    create.label = "覆盖后的名字".to_string();

    assert!(store.upsert(create).is_err(), "新增同 ID 的平台应被拒绝");
    let providers = store.load()?;
    assert_eq!(providers["p1"].label, "Label p1", "原有平台不应被新增覆盖");
    Ok(())
}

#[test]
fn create_mode_accepts_a_new_provider() -> Result<()> {
    let dir = TestDir::new("providers-create-new")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());

    let mut create = input("p2");
    create.mode = ProviderWriteMode::Create;

    assert_eq!(store.upsert(create)?, "p2");
    Ok(())
}

#[test]
fn update_mode_requires_an_existing_provider() -> Result<()> {
    let dir = TestDir::new("providers-update-missing")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());

    let mut update = input("missing");
    update.mode = ProviderWriteMode::Update;

    assert!(store.upsert(update).is_err(), "更新不存在的平台应被拒绝");
    Ok(())
}

#[test]
fn default_upsert_mode_still_replaces_in_place() -> Result<()> {
    let dir = TestDir::new("providers-upsert-default")?;
    let store = ProviderConfigStore::at(dir.path().to_path_buf());
    store.upsert(input("p1"))?;

    let mut next = input("p1");
    next.label = "改名后的平台".to_string();
    store.upsert(next)?;

    assert_eq!(store.load()?["p1"].label, "改名后的平台");
    Ok(())
}
