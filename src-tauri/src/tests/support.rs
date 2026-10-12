use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use anyhow::Result;

// 临时目录夹具：进程级唯一目录 + Drop 自动清理，供各域测试共用。
pub(crate) struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub(crate) fn new(prefix: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!("reins-{prefix}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).ok();
    }
}

// 会被 workspace/providers 路径解析读取的重定向变量：guard 统一接管，
// 避免真实环境里的变量漏进测试（断言 home 兜底路径时尤其致命）。
const TOOL_HOME_VARS: [&str; 5] = [
    "GROK_HOME",
    "PI_CODING_AGENT_DIR",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "DSH_HOME",
];

pub(crate) struct TestEnvGuard {
    original_home: Option<String>,
    original_reins_test_home: Option<std::ffi::OsString>,
    original_path: Option<String>,
    original_pi_session_dir: Option<String>,
    original_tool_home_vars: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnvGuard {
    pub(crate) fn lock() -> Self {
        let guard = test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        Self {
            original_home: env::var("HOME").ok(),
            original_reins_test_home: env::var_os("REINS_TEST_HOME"),
            original_path: env::var("PATH").ok(),
            original_pi_session_dir: env::var("PI_CODING_AGENT_SESSION_DIR").ok(),
            original_tool_home_vars: TOOL_HOME_VARS
                .iter()
                .map(|name| (*name, env::var_os(name)))
                .collect(),
            _guard: guard,
        }
    }

    pub(crate) fn set_home(temp_home: &Path) -> Self {
        let guard = Self::lock();
        // The override isolates the Rust backends on every platform; HOME also
        // redirects child processes (opencode CLI, git) on Unix. Windows
        // ignores HOME in dirs::home_dir(), so the env var below carries the
        // temp home into test child processes (see user_home_dir). 工具重定向
        // 变量一并清空：home 兜底的断言不该被真实环境里的重定向影响。
        for name in TOOL_HOME_VARS {
            unsafe { env::remove_var(name) };
        }
        unsafe { env::set_var("HOME", temp_home) };
        unsafe { env::set_var("REINS_TEST_HOME", temp_home) };
        crate::support::fs::set_home_override(Some(temp_home.to_path_buf()));
        guard
    }

    pub(crate) fn set_pi_session_dir(&self, path: &Path) {
        unsafe { env::set_var("PI_CODING_AGENT_SESSION_DIR", path) };
    }
}

impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        crate::support::fs::set_home_override(None);

        for (name, original) in &self.original_tool_home_vars {
            match original {
                Some(value) => unsafe { env::set_var(name, value) },
                None => unsafe { env::remove_var(name) },
            }
        }
        match &self.original_home {
            Some(home) => unsafe { env::set_var("HOME", home) },
            None => unsafe { env::remove_var("HOME") },
        }

        match &self.original_reins_test_home {
            Some(home) => unsafe { env::set_var("REINS_TEST_HOME", home) },
            None => unsafe { env::remove_var("REINS_TEST_HOME") },
        }

        match &self.original_path {
            Some(path) => unsafe { env::set_var("PATH", path) },
            None => unsafe { env::remove_var("PATH") },
        }

        match &self.original_pi_session_dir {
            Some(path) => unsafe { env::set_var("PI_CODING_AGENT_SESSION_DIR", path) },
            None => unsafe { env::remove_var("PI_CODING_AGENT_SESSION_DIR") },
        }
    }
}

fn test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
