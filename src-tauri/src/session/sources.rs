//! 来源注册表:一个会话来源的全部静态分发知识(根目录、探测文案、读取器、
//! 删除策略、用量采集方式)收敛为一张表。detect / available / clear /
//! usage 四条流程都由它驱动;新增来源 = 新 reader 文件 + 这里一条注册,
//! 按来源的 match 不得在 catalog.rs / usage_stats.rs / mod.rs 复活。

use std::path::{Path, PathBuf};
use std::thread;

use anyhow::{Result, anyhow};

use super::model::{DeletePlanAction, SessionOverview, SourceApp, UsageSourceStats};
use super::usage_stats::{self, UsageKind};
use super::{SessionReader, claude_code, codex, dsh, grokbuild, opencode, pi, zcode};

/// 可删来源的删除说明文案(逐字来自前端原 DELETE_METHOD_COPY),与 plan
/// 动作一起构成删除预演的全部 UI 内容。
#[derive(Clone, Copy)]
pub(crate) struct DeleteCopy {
    pub(crate) description: &'static str,
    pub(crate) details: &'static [&'static str],
    pub(crate) command_label: &'static str,
}

// 不支持删除的来源共用这一句 command 栏文案。
pub(crate) const DELETE_UNSUPPORTED_LABEL: &str = "不支持删除";

/// 一个来源的删除策略,同时承载执行与预演:能删的挂删除函数与 plan 构造;
/// 不能删的说明原因(reason 直达前端错误提示,notice 是删除计划 UI 文案)。
#[derive(Clone, Copy)]
pub(crate) enum DeletePolicy {
    Deleter {
        delete: fn(&Path) -> Result<()>,
        plan: fn(&SessionOverview) -> Result<Vec<DeletePlanAction>>,
        copy: DeleteCopy,
    },
    Unsupported {
        reason: &'static str,
        notice: &'static str,
    },
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
    delete: DeletePolicy::Deleter {
        delete: codex::delete_session,
        plan: codex::delete_plan,
        copy: DeleteCopy {
            description: "Codex 调用官方单会话删除命令（codex delete --force），会话从列表中移除。",
            details: &[
                "对当前会话组里每个仍在 state 库中的会话执行 codex delete --force。",
                "官方命令会级联删除子代理会话文件及 threads、spawn 关系、日志等关联记录。",
                "已不在 state 库中的会话（如已被级联删除）自动跳过。",
            ],
            command_label: "执行命令",
        },
    },
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
    delete: DeletePolicy::Deleter {
        delete: claude_code::delete_session,
        plan: claude_code::delete_plan,
        copy: DeleteCopy {
            description: "Claude Code 目前没有合适的单会话官方删除命令，这里按本地会话文件和附属目录清理。",
            details: &[
                "删除当前会话组对应的 JSONL 会话文件。",
                "子 Agent 会额外删除对应的 .meta.json 元数据。",
                "同步清理 session-env、file-history 等附属目录。",
            ],
            command_label: "等价执行动作",
        },
    },
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
    delete: DeletePolicy::Deleter {
        delete: opencode::delete_session,
        plan: opencode::delete_plan,
        copy: DeleteCopy {
            description: "OpenCode 调用官方单会话删除命令，会话从列表中移除。",
            details: &[
                "对当前会话组里的每个 session id 执行 opencode session delete。",
                "OpenCode v2 会级联删除子会话，已被级联删除的会话自动跳过。",
            ],
            command_label: "执行命令",
        },
    },
    usage: UsageKind::Sql {
        collect: opencode::usage_hours,
    },
};

static PI: SourceSpec = SourceSpec {
    app: SourceApp::Pi,
    root: pi::sessions_root,
    detect_note: Some("Reading ~/.pi/agent/sessions JSONL session files."),
    reader: &pi::BACKEND,
    delete: DeletePolicy::Deleter {
        delete: pi::delete_session,
        plan: pi::delete_plan,
        copy: DeleteCopy {
            description: "Pi 会话是工作目录下的独立 JSONL 文件，这里直接清理对应 transcript。",
            details: &[
                "删除当前 Pi session 对应的 JSONL 文件。",
                "保留其它工作目录下的 Pi session 文件。",
                "同步清理本工具的 Pi 索引和时间线缓存。",
            ],
            command_label: "等价执行动作",
        },
    },
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
    delete: DeletePolicy::Deleter {
        delete: grokbuild::delete_session,
        plan: grokbuild::delete_plan,
        copy: DeleteCopy {
            description: "Grok Build 主会话走官方删除命令，官方够不到的子代理子会话由本地清理。",
            details: &[
                "对主会话执行 grok sessions delete，官方一并清理其目录与搜索索引。",
                "子代理子会话目录及对应搜索索引行由本地清理。",
                "保留工作目录分组下的 prompt_history.jsonl 等共享文件。",
            ],
            command_label: "执行动作",
        },
    },
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
    delete: DeletePolicy::Unsupported {
        reason: "ZCode session deletion is unsupported",
        notice: "暂不支持删除 ZCode 会话。",
    },
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
    delete: DeletePolicy::Unsupported {
        reason: "DSH session deletion is unsupported",
        notice: "暂不支持删除 DeepSeek Harness 会话。",
    },
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
