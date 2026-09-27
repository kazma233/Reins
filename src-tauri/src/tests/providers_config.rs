// providers.yaml 存储层测试：往返、校验、CRUD。

use std::fs;

use anyhow::Result;

use crate::providers::config::ProviderConfigStore;
use crate::providers::types::{
    ProviderModelInput, ProviderProtocol, ProviderUpsertInput, ReasoningLevel,
};
use crate::test_support::TestDir;

fn input(id: &str) -> ProviderUpsertInput {
    ProviderUpsertInput {
        provider_id: id.to_string(),
        label: format!("Label {id}"),
        protocol: ProviderProtocol::OpenaiChatCompletions,
        base_url: format!("https://{id}.test/v1"),
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
