//! 来源注册表:一个会话来源的全部静态分发知识(根目录、探测文案、读取器、
//! 删除策略、用量采集方式)收敛为一张表。detect / available / clear /
//! usage 四条流程都由它驱动;新增来源 = 新 reader 文件 + 这里一条注册,
//! 按来源的 match 不得在 catalog.rs / usage_stats.rs / mod.rs 复活。

use std::path::{Path, PathBuf};
use std::thread;

use anyhow::{Result, anyhow};

use super::model::{SourceApp, UsageSourceStats};
use super::usage_stats::{self, UsageKind};
use super::{SessionReader, claude_code, codex, dsh, grokbuild, opencode, pi, zcode};

/// 一个来源的删除策略:能删的挂删除函数;不能删的说明原因,文案直达前端
/// 错误提示。
#[derive(Clone, Copy)]
pub(crate) enum DeletePolicy {
    Deleter(fn(&Path) -> Result<()>),
    Unsupported(&'static str),
}

/// 纯静态分发数据,按值复制进注册表。
#[derive(Clone, Copy)]
pub(crate) struct SourceSpec {
    pub(crate) app: SourceApp,
    /// 来源根目录,detect 与 available 的可用性判断共用。
    pub(crate) root: fn() -> Result<PathBuf>,
    /// detect 结果里的说明文案;grokbuild 沿用无说明。
    pub(crate) detect_note: Option<&'static str>,
    pub(crate) reader: &'static dyn SessionReader,
    pub(crate) delete: DeletePolicy,
    pub(crate) usage: UsageKind,
}

impl SourceSpec {
    pub(crate) fn usage_days(&self) -> Result<Option<UsageSourceStats>> {
        let root = (self.root)()?;

        match &self.usage {
            UsageKind::CachedJsonl { subdir, extract } => {
                let scan_root = match subdir {
                    Some(subdir) => root.join(subdir),
                    None => root,
                };
                usage_stats::days_from_cached_jsonl(self.app, scan_root, *extract)
            }
            UsageKind::RawFiles { paths, extract } => {
                usage_stats::days_from_paths(self.app, &root, *paths, *extract)
            }
            UsageKind::Sql { collect } => usage_stats::days_from_sql(self.app, *collect),
        }
    }
}

static CODEX: SourceSpec = SourceSpec {
    app: SourceApp::Codex,
    root: codex::root,
    detect_note: Some("Using ~/.codex/sessions as the primary transcript source."),
    reader: &codex::BACKEND,
    delete: DeletePolicy::Deleter(codex::delete_session),
    usage: UsageKind::CachedJsonl {
        subdir: Some("sessions"),
        extract: codex::usage_hours,
    },
};

static CLAUDE_CODE: SourceSpec = SourceSpec {
    app: SourceApp::ClaudeCode,
    root: claude_code::root,
    detect_note: Some("Reading project-level session JSONL files."),
    reader: &claude_code::BACKEND,
    delete: DeletePolicy::Deleter(claude_code::delete_session),
    usage: UsageKind::CachedJsonl {
        subdir: Some("projects"),
        extract: claude_code::usage_hours,
    },
};

static OPENCODE: SourceSpec = SourceSpec {
    app: SourceApp::OpenCode,
    root: opencode::root,
    detect_note: Some("Reading sessions from ~/.local/share/opencode/opencode.db."),
    reader: &opencode::BACKEND,
    delete: DeletePolicy::Deleter(opencode::delete_session),
    usage: UsageKind::Sql {
        collect: opencode::usage_hours,
    },
};

static PI: SourceSpec = SourceSpec {
    app: SourceApp::Pi,
    root: pi::sessions_root,
    detect_note: Some("Reading ~/.pi/agent/sessions JSONL session files."),
    reader: &pi::BACKEND,
    delete: DeletePolicy::Deleter(pi::delete_session),
    usage: UsageKind::CachedJsonl {
        subdir: None,
        extract: pi::usage_hours,
    },
};

static GROKBUILD: SourceSpec = SourceSpec {
    app: SourceApp::GrokBuild,
    root: grokbuild::root,
    detect_note: None,
    reader: &grokbuild::BACKEND,
    delete: DeletePolicy::Deleter(grokbuild::delete_session),
    usage: UsageKind::RawFiles {
        paths: grokbuild::session_summary_paths,
        extract: grokbuild::usage_hours,
    },
};

// zcode CLI 不随桌面版安装、无官方单会话删除命令,直接删库又与常驻
// 进程的写入冲突,所以整体不提供删除。
static ZCODE: SourceSpec = SourceSpec {
    app: SourceApp::Zcode,
    root: zcode::root,
    detect_note: Some("Reading sessions from ~/.zcode/cli/db/db.sqlite."),
    reader: &zcode::BACKEND,
    delete: DeletePolicy::Unsupported("ZCode session deletion is unsupported"),
    usage: UsageKind::Sql {
        collect: zcode::usage_hours,
    },
};

// dsh 无官方单会话删除命令(桌面版删除只是 workspace.json 的 archive
// 标记),删除转录文件会与常驻进程的写入冲突。
static DSH: SourceSpec = SourceSpec {
    app: SourceApp::Dsh,
    root: dsh::sessions_root,
    detect_note: Some("Reading ~/.dsh/sessions transcript files."),
    reader: &dsh::BACKEND,
    delete: DeletePolicy::Unsupported("DSH session deletion is unsupported"),
    usage: UsageKind::RawFiles {
        paths: dsh::session_transcript_paths,
        extract: dsh::usage_hours,
    },
};

// 顺序是可观察行为:detect 输出与 usage sources 都按该顺序产出,不得随意
// 重排;与 sources_registry_covers_every_source_app_once_in_order 测试互为约束。
pub(crate) static SOURCES: &[SourceSpec] =
    &[CODEX, CLAUDE_CODE, OPENCODE, PI, GROKBUILD, ZCODE, DSH];

pub(crate) fn spec(app: SourceApp) -> &'static SourceSpec {
    match app {
        SourceApp::Codex => &CODEX,
        SourceApp::ClaudeCode => &CLAUDE_CODE,
        SourceApp::OpenCode => &OPENCODE,
        SourceApp::Pi => &PI,
        SourceApp::GrokBuild => &GROKBUILD,
        SourceApp::Zcode => &ZCODE,
        SourceApp::Dsh => &DSH,
    }
}

/// 按 SOURCES 声明顺序并行采集每个来源并同序返回;线程 panic 折叠为携带
/// label 的错误,与逐来源 spawn 时的文案一致。
pub(crate) fn parallel_collect<T, F>(label: &'static str, collect: F) -> Vec<Result<T>>
where
    F: Fn(&SourceSpec) -> Result<T> + Send + Sync,
    T: Send,
{
    thread::scope(|scope| {
        let collect = &collect;
        let handles: Vec<_> = SOURCES
            .iter()
            .map(|spec| scope.spawn(move || collect(spec)))
            .collect();

        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| anyhow!("{label} thread panicked"))?
            })
            .collect()
    })
}
