use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use dirs::config_dir;

use super::types::{
    ProviderModelInput, ProviderModelRecord, ProviderUpsertInput, RawProvider,
    RawProvidersDocument, ResolvedProvider,
};
use crate::support::fs::write_atomic;

const PROVIDERS_CONFIG_NAME: &str = "providers.yaml";
const DOCUMENT_VERSION: i64 = 1;

// 平台 ID 与 workspace target id 同一字符集：小写字母、数字和 -。
fn normalize_provider_id(value: &str) -> Result<String> {
    let trimmed = value.trim().to_lowercase().replace('_', "-");
    if trimmed.is_empty() {
        bail!("平台 ID 不能为空。");
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        bail!("平台 ID 只能包含小写字母、数字和 -：{}", value.trim());
    }
    if trimmed.starts_with("reins-") {
        // reins- 前缀是工具配置里的内部注册键空间，平台 ID 不允许占用。
        bail!("平台 ID 不能以 reins- 开头。");
    }
    Ok(trimmed)
}

fn validate_base_url(value: &str) -> Result<String> {
    let trimmed = value.trim().to_string();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        bail!("Base URL 必须以 http:// 或 https:// 开头：{trimmed}");
    }
    Ok(trimmed)
}

fn normalize_model_record(input: ProviderModelInput) -> Result<ProviderModelRecord> {
    let id = input.id.trim().to_string();
    if id.is_empty() {
        bail!("模型 ID 不能为空。");
    }
    let label = normalize_model_label(&input.label, &id);
    let reasoning_levels = input.reasoning_levels.map(|mut levels| {
        levels.sort();
        levels.dedup();
        levels
    });
    Ok(ProviderModelRecord {
        id,
        label,
        context_window: input.context_window,
        max_output_tokens: input.max_output_tokens,
        supports_images: input.supports_images,
        reasoning: input.reasoning,
        reasoning_levels,
    })
}

fn normalize_model_label(label: &str, id: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        id.to_string()
    } else {
        trimmed.to_string()
    }
}

// providers.yaml 的唯一读写入口：与 WorkspaceConfigStore 相同的模式——
// 单实例由 Tauri 管理，进程内锁串行化 read-modify-write，原子写落盘。
#[derive(Clone)]
pub(crate) struct ProviderConfigStore(Arc<StoreInner>);

struct StoreInner {
    config_path: PathBuf,
    lock: Mutex<()>,
}

impl ProviderConfigStore {
    pub(crate) fn app() -> Self {
        Self::at(
            config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("reins"),
        )
    }

    pub(crate) fn at(app_dir: impl Into<PathBuf>) -> Self {
        let config_path = app_dir.into().join(PROVIDERS_CONFIG_NAME);
        Self(Arc::new(StoreInner {
            config_path,
            lock: Mutex::new(()),
        }))
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.0.config_path
    }

    pub(crate) fn app_dir(&self) -> &Path {
        self.0
            .config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
    }

    // 文件不存在视为空目录，而不是错误；首次写入时才创建文件。
    pub(crate) fn load(&self) -> Result<BTreeMap<String, ResolvedProvider>> {
        let guard = self.lock();
        self.load_locked(&guard)
    }

    fn load_locked(&self, _guard: &()) -> Result<BTreeMap<String, ResolvedProvider>> {
        let config_path = &self.0.config_path;
        if !config_path.exists() {
            return Ok(BTreeMap::new());
        }
        let raw_content = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        self.parse_content(&raw_content)
    }

    fn parse_content(&self, raw_content: &str) -> Result<BTreeMap<String, ResolvedProvider>> {
        if raw_content.trim().is_empty() {
            return Ok(BTreeMap::new());
        }
        let document: RawProvidersDocument =
            serde_yaml::from_str(raw_content).with_context(|| {
                format!("providers.yaml 解析失败：{}", self.0.config_path.display())
            })?;
        let mut providers = BTreeMap::new();
        for (id, raw) in document.providers {
            let resolved = ResolvedProvider {
                id: id.clone(),
                label: raw.label,
                protocol: raw.protocol,
                base_url: raw.base_url,
                models: raw.models,
            };
            providers.insert(id, resolved);
        }
        Ok(providers)
    }

    fn save_locked(&self, providers: &BTreeMap<String, ResolvedProvider>) -> Result<()> {
        let document = RawProvidersDocument {
            version: DOCUMENT_VERSION,
            providers: providers
                .iter()
                .map(|(id, provider)| {
                    (
                        id.clone(),
                        RawProvider {
                            label: provider.label.clone(),
                            protocol: provider.protocol,
                            base_url: provider.base_url.clone(),
                            models: provider.models.clone(),
                        },
                    )
                })
                .collect(),
        };
        let serialized = serde_yaml::to_string(&document)?;
        write_atomic(&self.0.config_path, &serialized)
            .with_context(|| format!("Failed to write {}", self.0.config_path.display()))
    }

    pub(crate) fn upsert(&self, input: ProviderUpsertInput) -> Result<String> {
        let provider_id = normalize_provider_id(&input.provider_id)?;
        let label = input.label.trim();
        if label.is_empty() {
            bail!("平台名称不能为空。");
        }
        let base_url = validate_base_url(&input.base_url)?;

        let mut models = Vec::with_capacity(input.models.len());
        let mut seen_models = std::collections::BTreeSet::new();
        for model in input.models {
            let record = normalize_model_record(model)?;
            if !seen_models.insert(record.id.clone()) {
                bail!("模型 ID 重复：{}", record.id);
            }
            models.push(record);
        }

        let guard = self.lock();
        let mut providers = self.load_locked(&guard)?;
        providers.insert(
            provider_id.clone(),
            ResolvedProvider {
                id: provider_id.clone(),
                label: label.to_string(),
                protocol: input.protocol,
                base_url,
                models,
            },
        );
        self.save_locked(&providers)?;
        Ok(provider_id)
    }

    pub(crate) fn delete(&self, provider_id: &str) -> Result<()> {
        let guard = self.lock();
        let mut providers = self.load_locked(&guard)?;
        if providers.remove(provider_id).is_none() {
            bail!("平台不存在：{provider_id}");
        }
        self.save_locked(&providers)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.0
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
#[path = "../tests/providers_config.rs"]
mod tests;
