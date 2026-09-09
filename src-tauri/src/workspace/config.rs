use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result, anyhow, bail};
use dirs::config_dir;

use super::types::{RawManagerConfig, ResolvedManagerConfig, WorkspaceConfigDocument};
use super::{
    default_config_template, document_from_content, parse_git_owner_repo, parse_manager_config,
};

const APP_DIR_NAME: &str = "reins";
const DEFAULT_CONFIG_NAME: &str = "config.yaml";

// Deep module that owns every disk interaction with the global config.yaml:
// path resolution, read-modify-write mutual exclusion and atomic writes.
// A single instance is managed by Tauri; tests build isolated ones via `at`.
// Cloning shares the same lock and paths, so handles can be moved into
// spawn_blocking closures freely.
#[derive(Clone)]
pub(crate) struct WorkspaceConfigStore(Arc<StoreInner>);

struct StoreInner {
    app_dir: PathBuf,
    config_path: PathBuf,
    lock: Mutex<()>,
}

impl WorkspaceConfigStore {
    // The only place that knows where the app lives in the system config dir.
    pub(crate) fn app() -> Self {
        Self::at(
            config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(APP_DIR_NAME),
        )
    }

    // Test/injection constructor: everything (config.yaml, git cache) is
    // resolved under `app_dir`, never touching the real user directory.
    pub(crate) fn at(app_dir: impl Into<PathBuf>) -> Self {
        let app_dir = app_dir.into();
        let config_path = app_dir.join(DEFAULT_CONFIG_NAME);
        Self(Arc::new(StoreInner {
            app_dir,
            config_path,
            lock: Mutex::new(()),
        }))
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.0.config_path
    }

    pub(crate) fn git_cache_root(&self) -> PathBuf {
        self.0.app_dir.join("git")
    }

    // Cache identity must stay stable across import/sync/delete, so malformed
    // repo strings are rejected instead of silently hashed somewhere else.
    pub(crate) fn git_cache_dir_for_repo(&self, repo: &str) -> Result<PathBuf> {
        let (owner, name) = parse_git_owner_repo(repo)
            .ok_or_else(|| anyhow!("无法解析 git 仓库 owner/repo：{repo}"))?;
        Ok(self.git_cache_root().join(owner).join(name))
    }

    pub(crate) fn read_raw(&self) -> Result<String> {
        let config_path = self.config_path();
        if config_path.exists() {
            fs::read_to_string(config_path)
                .with_context(|| format!("Failed to read config file {}", config_path.display()))
        } else {
            bail!("配置文件不存在：{}", config_path.display())
        }
    }

    // pub(super): ResolvedManagerConfig stays inside the workspace module tree.
    pub(super) fn parse(&self) -> Result<ResolvedManagerConfig> {
        let raw_content = self.read_raw()?;
        parse_manager_config(&raw_content, self.config_path())
    }

    // Read the config document for the frontend, bootstrapping the default
    // template on first launch so the user has something to edit.
    pub(crate) fn load_document(&self) -> Result<WorkspaceConfigDocument> {
        let config_path = self.config_path();
        let mut exists = config_path.exists();
        let raw_content = if exists {
            fs::read_to_string(config_path)
                .with_context(|| format!("Failed to read config file {}", config_path.display()))?
        } else {
            let template = default_config_template();
            write_atomic(config_path, &template)?;
            // The template has just been persisted to disk, so the document
            // is no longer "missing" — flip the flag so document_from_content
            // takes the parse branch instead of returning config: None.
            exists = true;
            template
        };

        Ok(document_from_content(
            self,
            config_path,
            exists,
            raw_content,
        ))
    }

    // Serialize a whole read-modify-write sequence under the store lock so
    // concurrent commands cannot interleave read/write and drop updates.
    // The handle handed to `body` is the only sanctioned way to write the
    // config file while the lock is held.
    pub(super) fn locked<T>(&self, body: impl FnOnce(&ConfigLock<'_>) -> Result<T>) -> Result<T> {
        // A poisoned lock only means a mutation panicked mid-way; the file
        // itself is either the old or the new content (atomic write), so it is
        // safe to keep going instead of bricking every later command.
        let guard = self
            .0
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let lock = ConfigLock {
            store: self,
            _guard: guard,
        };
        body(&lock)
    }
}

// Lock-scoped handle for mutation sequences. Reads and writes issued through
// it happen while the store-wide mutex is held.
pub(super) struct ConfigLock<'a> {
    store: &'a WorkspaceConfigStore,
    _guard: MutexGuard<'a, ()>,
}

impl ConfigLock<'_> {
    pub(super) fn config_path(&self) -> &Path {
        self.store.config_path()
    }

    pub(super) fn read_raw(&self) -> Result<String> {
        self.store.read_raw()
    }

    // No extra context on purpose: the raw deserialize step never added one,
    // and parse errors from serde_yaml are already self-describing.
    pub(super) fn parse_raw(&self) -> Result<RawManagerConfig> {
        let raw_content = self.read_raw()?;
        Ok(serde_yaml::from_str(&raw_content)?)
    }

    pub(super) fn write_raw(&self, config: &RawManagerConfig) -> Result<()> {
        let serialized = serde_yaml::to_string(config)?;
        let config_path = self.config_path();
        write_atomic(config_path, &serialized)
            .with_context(|| format!("Failed to write config file {}", config_path.display()))
    }
}

// Write through a same-directory temp file + rename, so a crash mid-write can
// never leave a half-written config behind. std::fs::rename replaces existing
// files on Windows as well.
pub(super) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory)?;

    let file_name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(DEFAULT_CONFIG_NAME);
    // Unique sibling name: concurrent writers must not clobber each other's
    // temp file, and the dot prefix keeps it out of glob listings.
    let tmp_path = directory.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));

    let result = fs::write(&tmp_path, contents).and_then(|()| fs::rename(&tmp_path, path));
    if result.is_err() {
        // Best-effort cleanup; a leftover temp file is harmless but noisy.
        let _ = fs::remove_file(&tmp_path);
    }
    // No context added here on purpose: every caller attaches its own
    // user-facing message, and stacking contexts would only obscure it.
    result?;

    Ok(())
}
