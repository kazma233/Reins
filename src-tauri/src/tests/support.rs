use std::env;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

pub(crate) struct TestEnvGuard {
    original_home: Option<String>,
    original_path: Option<String>,
    original_pi_agent_dir: Option<String>,
    original_pi_session_dir: Option<String>,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnvGuard {
    pub(crate) fn lock() -> Self {
        let guard = test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        Self {
            original_home: env::var("HOME").ok(),
            original_path: env::var("PATH").ok(),
            original_pi_agent_dir: env::var("PI_CODING_AGENT_DIR").ok(),
            original_pi_session_dir: env::var("PI_CODING_AGENT_SESSION_DIR").ok(),
            _guard: guard,
        }
    }

    pub(crate) fn set_home(temp_home: &Path) -> Self {
        let guard = Self::lock();
        // The override isolates the Rust backends on every platform; HOME also
        // redirects child processes (opencode CLI, git) on Unix.
        unsafe { env::set_var("HOME", temp_home) };
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

        match &self.original_home {
            Some(home) => unsafe { env::set_var("HOME", home) },
            None => unsafe { env::remove_var("HOME") },
        }

        match &self.original_path {
            Some(path) => unsafe { env::set_var("PATH", path) },
            None => unsafe { env::remove_var("PATH") },
        }

        match &self.original_pi_agent_dir {
            Some(path) => unsafe { env::set_var("PI_CODING_AGENT_DIR", path) },
            None => unsafe { env::remove_var("PI_CODING_AGENT_DIR") },
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
