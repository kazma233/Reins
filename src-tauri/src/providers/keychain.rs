// 系统密钥管理封装：service 固定 me.kazma.reins，account = providers.yaml
// 中的平台 ID。read 只在进程内部使用，不作为 Tauri 命令暴露给前端。

use anyhow::{Context, Result};

use crate::logger;

// trait 抽象是为了测试用假后端；真实后端只在手动验证时使用。
pub(crate) trait KeyBackend: Send + Sync {
    fn set(&self, account: &str, value: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
    fn exists(&self, account: &str) -> Result<bool>;
    fn read(&self, account: &str) -> Result<Option<String>>;
}

pub(crate) const KEYCHAIN_SERVICE: &str = "me.kazma.reins";

pub(crate) struct NativeKeyBackend;

impl NativeKeyBackend {
    fn entry(account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .with_context(|| format!("无法创建密钥条目 account={account}"))
    }
}

impl KeyBackend for NativeKeyBackend {
    fn set(&self, account: &str, value: &str) -> Result<()> {
        Self::entry(account)?
            .set_password(value)
            .with_context(|| format!("写入系统密钥失败 account={account}"))
    }

    fn delete(&self, account: &str) -> Result<()> {
        match Self::entry(account).and_then(|entry| {
            entry
                .delete_credential()
                .with_context(|| format!("删除系统密钥失败 account={account}"))
        }) {
            Ok(()) => Ok(()),
            Err(error) => {
                if is_no_entry(&error) {
                    Ok(())
                } else {
                    Err(error)
                }
            }
        }
    }

    fn exists(&self, account: &str) -> Result<bool> {
        // 读取失败（密钥服务故障）必须向上传播；返回 false 会把故障
        // 伪装成「未设 Key」，误导用户重设密钥。
        Ok(self.read(account)?.is_some())
    }

    fn read(&self, account: &str) -> Result<Option<String>> {
        match Self::entry(account).and_then(|entry| {
            entry
                .get_password()
                .with_context(|| format!("读取系统密钥失败 account={account}"))
        }) {
            Ok(password) => Ok(Some(password)),
            Err(error) => {
                if is_no_entry(&error) {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }
}

fn is_no_entry(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<keyring::Error>())
        .any(|cause| matches!(cause, keyring::Error::NoEntry))
}

// 测试假后端：内存哈希表。真实后端只在手动验证时使用。
#[derive(Default)]
#[cfg(test)]
pub(crate) struct FakeKeyBackend {
    pub(crate) entries: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
}

#[cfg(test)]
impl KeyBackend for FakeKeyBackend {
    fn set(&self, account: &str, value: &str) -> Result<()> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(account.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<()> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(account);
        Ok(())
    }

    fn exists(&self, account: &str) -> Result<bool> {
        Ok(self.read(account)?.is_some())
    }

    fn read(&self, account: &str) -> Result<Option<String>> {
        Ok(self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(account)
            .cloned())
    }
}

// Tauri 管理的密钥服务状态；克隆共享同一后端实例。
#[derive(Clone)]
pub(crate) struct ProviderKeyStore {
    backend: std::sync::Arc<dyn KeyBackend>,
}

impl ProviderKeyStore {
    pub(crate) fn native() -> Self {
        Self {
            backend: std::sync::Arc::new(NativeKeyBackend),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_backend(backend: std::sync::Arc<dyn KeyBackend>) -> Self {
        Self { backend }
    }

    pub(crate) fn set_key(&self, provider_id: &str, value: &str) -> Result<()> {
        self.backend.set(provider_id, value)?;
        // 只记录平台 ID 与动作，任何情况下不记录密钥内容。
        logger::log_info(format!("set_provider_key provider_id={provider_id}"));
        Ok(())
    }

    pub(crate) fn remove_key(&self, provider_id: &str) -> Result<()> {
        self.backend.delete(provider_id)?;
        logger::log_info(format!("remove_provider_key provider_id={provider_id}"));
        Ok(())
    }

    pub(crate) fn key_present(&self, provider_id: &str) -> Result<bool> {
        self.backend.exists(provider_id)
    }

    pub(crate) fn read_key(&self, provider_id: &str) -> Result<Option<String>> {
        self.backend.read(provider_id)
    }
}
