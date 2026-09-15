import type { ImportPreview, SessionOverview, SourceApp } from "./types";

// --- import target copy ---

export const IMPORT_TARGET_APPS: SourceApp[] = [
  "codex",
  "claude_code",
  "opencode",
  "pi"
];

export type ImportTargetCopy = {
  optionLabel: string;
  methodLabel: string;
  methodDescription: string;
  successTitle: string;
  successNote: string | null;
  manualLabel: string;
};

export const IMPORT_TARGET_COPY: Partial<Record<SourceApp, ImportTargetCopy>> = {
  codex: {
    optionLabel: "Codex",
    methodLabel: "兼容写入",
    methodDescription: "生成 Codex 兼容的 transcript JSONL。",
    successTitle: "已写入 Codex transcript",
    successNote: "已通过 transcript 读回校验。",
    manualLabel: "手动恢复命令"
  },
  claude_code: {
    optionLabel: "Claude Code",
    methodLabel: "兼容写入",
    methodDescription: "生成 Claude Code 项目目录下的会话 JSONL。",
    successTitle: "已导入到 Claude Code",
    successNote: null,
    manualLabel: "手动打开命令"
  },
  opencode: {
    optionLabel: "OpenCode",
    methodLabel: "官方导入",
    methodDescription: "通过 OpenCode 官方 CLI 导入。",
    successTitle: "已导入到 OpenCode",
    successNote: null,
    manualLabel: "手动打开命令"
  },
  pi: {
    optionLabel: "Pi",
    methodLabel: "兼容写入",
    methodDescription: "按工作目录写入 Pi v3 JSONL 会话。",
    successTitle: "已写入 Pi session",
    successNote: "已通过 Pi transcript 读回校验。",
    manualLabel: "手动恢复命令"
  }
};

export function importTargetCopy(app: SourceApp): ImportTargetCopy {
  const copy = IMPORT_TARGET_COPY[app];
  if (!copy) throw new Error("不支持导入到 Grok Build");
  return copy;
}

export function defaultImportTarget(sourceApp: SourceApp): SourceApp {
  return IMPORT_TARGET_APPS.find((app) => app !== sourceApp) ?? sourceApp;
}

// --- manual resume commands ---

export function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'"'"'`)}'`;
}

export function codexResumeCommand(sessionId: string, cwd: string | null): string {
  if (!cwd) {
    return `codex resume ${sessionId}`;
  }
  return `codex resume -C ${shellQuote(cwd)} ${sessionId}`;
}

export function claudeResumeCommand(sessionId: string, cwd: string | null): string {
  if (!cwd) {
    return `claude --resume ${sessionId}`;
  }
  return `cd ${shellQuote(cwd)} && claude --resume ${sessionId}`;
}

export function opencodeResumeCommand(sessionId: string, cwd: string | null): string {
  if (!cwd) {
    return `opencode --session ${sessionId}`;
  }
  return `cd ${shellQuote(cwd)} && opencode --session ${sessionId}`;
}

export function piResumeCommand(
  sessionId: string,
  cwd: string | null,
  transcriptPath?: string | null
): string {
  const target = transcriptPath ?? sessionId;
  if (!cwd) {
    return `pi --session ${shellQuote(target)}`;
  }
  return `cd ${shellQuote(cwd)} && pi --session ${shellQuote(target)}`;
}

export function manualOpenCommand(
  targetApp: SourceApp,
  sessionId: string,
  cwd: string | null,
  transcriptPath?: string | null
): string {
  switch (targetApp) {
    case "codex":
      return codexResumeCommand(sessionId, cwd);
    case "claude_code":
      return claudeResumeCommand(sessionId, cwd);
    case "opencode":
      return opencodeResumeCommand(sessionId, cwd);
    case "grokbuild":
      return `grok${cwd ? ` --cwd ${shellQuote(cwd)}` : ""} --resume ${shellQuote(sessionId)}`;
    case "pi":
      return piResumeCommand(sessionId, cwd, transcriptPath);
    default:
      return sessionId;
  }
}

// --- import preview copy ---

export function formatImportLevel(value: ImportPreview["importLevel"]): string {
  switch (value) {
    case "full":
      return "完整导入";
    case "partial":
      return "部分导入";
    case "unsupported":
      return "不支持导入";
  }
}

// Keys are the Rust-side warning strings. Editing a warning sentence on the
// Rust side silently falls back to the raw English text here — the long-term
// fix is stable warning codes from the backend (contracts ADR, step 3).
export function translateWarning(warning: string): string {
  switch (warning) {
    case "This target exporter preserves tool output text but not tool failure flags.":
      return "该目标导出器会保留工具输出文本，但不保留工具失败标记。";
    case "Import to Grok Build is unsupported":
      return "暂不支持导入到 Grok Build。";
    case "The source session has no importable messages.":
      return "源会话中没有可导入的消息。";
    case "OpenCode support is limited to source detection until real transcript samples are available.":
      return "在拿到真实转录样本前，OpenCode 目前只支持来源检测。";
    case "The current MVP only exposes cross-program import. Same-app cloning is intentionally disabled.":
      return "当前 MVP 仅支持跨程序导入，暂不支持同程序克隆。";
    case "Only the normalized message timeline is imported. Side-channel runtime events are skipped.":
      return "仅导入标准化后的消息时间线，旁路运行事件会被跳过。";
    case "The target program may render the imported conversation differently from the original UI.":
      return "目标程序对导入会话的显示效果，可能与原始界面不同。";
    case "This import path creates a brand-new session file, so backup paths are usually empty.":
      return "该导入方式会创建全新的会话文件，因此备份路径通常为空。";
    case "The source session does not expose a cwd, so the importer will fall back to the home directory.":
      return "源会话未提供工作目录，导入器将回退到用户主目录。";
    case "Non-message events are not recreated in the target program.":
      return "非消息类事件不会在目标程序中重建。";
    case "Codex function calls are mapped into Claude tool_use/tool_result records on a best-effort basis.":
      return "Codex 的函数调用会尽力映射为 Claude 的 tool_use/tool_result 记录。";
    case "Tool calls are mapped into Claude tool_use/tool_result records on a best-effort basis.":
      return "工具调用会尽力映射为 Claude 的 tool_use/tool_result 记录。";
    case "Codex import writes transcript JSONL only. SQLite state is expected to be populated lazily by Codex when the session is resumed.":
      return "导入到 Codex 时只会写入 transcript JSONL；SQLite 状态预计会在恢复会话时由 Codex 延迟补齐。";
    case "Codex import writes transcript JSONL only. Reins validates the written transcript, but Codex still needs to resume the session once before its internal SQLite state is populated.":
      return "导入到 Codex 时只会写入 transcript JSONL；本工具只校验写入后的 transcript 可读，仍需要在 Codex 中至少恢复一次会话。恢复时建议显式带上工作目录，避免 Codex 询问使用当前目录还是程序目录。";
    case "OpenCode import target is not supported yet. Export from OpenCode is supported.":
      return "暂不支持导入到 OpenCode，但已支持从 OpenCode 导出到其他程序。";
    case "OpenCode import is delegated to the official CLI importer using generated session JSON.":
      return "导入到 OpenCode 将通过官方 CLI 的 import 命令完成，使用生成的会话 JSON。";
    case "Pi import writes a v3 JSONL session under the cwd-specific Pi session directory.":
      return "导入到 Pi 时会按工作目录写入 v3 JSONL 会话文件。";
    default:
      return warning;
  }
}

// --- delete method copy + command preview ---

export type DeleteMethodCopy = {
  description: string;
  details: string[];
  commandLabel: string;
};

export function canDeleteSession(app: SourceApp): boolean {
  return app !== "grokbuild";
}

export const DELETE_METHOD_COPY: Record<SourceApp, DeleteMethodCopy> = {
  grokbuild: {
    description: "暂不支持删除 Grok Build 会话。",
    details: [],
    commandLabel: "不支持删除"
  },
  codex: {
    description:
      "Codex 目前没有确认可用的单会话官方删除命令，这里按本地 transcript 和状态库清理。",
    details: [
      "删除当前会话组对应的 transcript JSONL 文件。",
      "同步清理 state_*.sqlite 里的 thread 和 subagent 关系记录。",
      "同步清理 logs_2.sqlite 里的 thread 日志。"
    ],
    commandLabel: "等价执行动作"
  },
  claude_code: {
    description:
      "Claude Code 目前没有合适的单会话官方删除命令，这里按本地会话文件和附属目录清理。",
    details: [
      "删除当前会话组对应的 JSONL 会话文件。",
      "子 Agent 会额外删除对应的 .meta.json 元数据。",
      "同步清理 session-env、file-history 等附属目录。"
    ],
    commandLabel: "等价执行动作"
  },
  opencode: {
    description:
      "OpenCode 会先调用官方单会话删除，再补删官方不会自动清理的本地 diff 拷留。",
    details: [
      "对当前会话组里的每个 session id 执行 opencode session delete。",
      "session 会从 OpenCode 列表中删除。",
      "storage/session_diff/<sessionId>.json 会额外清理。"
    ],
    commandLabel: "执行命令"
  },
  pi: {
    description:
      "Pi 会话是工作目录下的独立 JSONL 文件，这里直接清理对应 transcript。",
    details: [
      "删除当前 Pi session 对应的 JSONL 文件。",
      "保留其它工作目录下的 Pi session 文件。",
      "同步清理本工具的 Pi 索引和时间线缓存。"
    ],
    commandLabel: "等价执行动作"
  }
};

export function deleteMethodCopy(app: SourceApp): DeleteMethodCopy {
  return DELETE_METHOD_COPY[app];
}

function sqlQuote(value: string): string {
  return value.replace(/'/g, "''");
}

// Dialogs-local shell quote: uses '\'' escaping (distinct from shellQuote
// which uses '"'"' escaping for resume commands).
function dialogShellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function deleteTargetSessionIds(detail: SessionOverview): string[] {
  const sessionIds =
    detail.agents.length > 0
      ? detail.agents.map((agent) => agent.sessionId)
      : [detail.summary.sourceSessionId];
  return Array.from(new Set(sessionIds));
}

function claudeSidecarPaths(sessionId: string): string[] {
  return [
    `~/.claude/projects/${sessionId}`,
    `~/.claude/session-env/${sessionId}`,
    `~/.claude/file-history/${sessionId}`
  ];
}

function claudeSubagentMetaPaths(sourcePaths: string[]): string[] {
  return sourcePaths.flatMap((path) =>
    path.includes("/subagents/") && path.endsWith(".jsonl")
      ? [path.replace(/\.jsonl$/, ".meta.json")]
      : []
  );
}

// Front-of-house description of what the Rust delete engine does per app.
// Shadow contract: the backend delete semantics own the truth; if they change,
// this preview must follow (long-term: backend returns the command list).
export function deleteCommandPreview(detail: SessionOverview): string[] {
  const sessionIds = deleteTargetSessionIds(detail);

  switch (detail.summary.sourceApp) {
    case "grokbuild":
      return [];
    case "opencode":
      return [
        ...sessionIds.map(
          (sessionId) => `opencode session delete ${sessionId}`
        ),
        ...sessionIds.map(
          (sessionId) =>
            `rm "$HOME/.local/share/opencode/storage/session_diff/${sessionId}.json"`
        )
      ];
    case "claude_code":
      return [
        ...detail.sourcePaths.map((path) => `rm ${dialogShellQuote(path)}`),
        ...claudeSubagentMetaPaths(detail.sourcePaths).map(
          (path) => `rm ${dialogShellQuote(path)}`
        ),
        ...claudeSidecarPaths(detail.summary.sourceSessionId).map(
          (path) => `rm -rf "$HOME/${path.replace(/^~\//, "")}"`
        )
      ];
    case "codex":
      return [
        ...detail.sourcePaths.map((path) => `rm ${dialogShellQuote(path)}`),
        ...sessionIds.map(
          (sessionId) =>
            `for db in "$HOME"/.codex/state_*.sqlite; do sqlite3 "$db" "DELETE FROM thread_spawn_edges WHERE child_thread_id = '${sqlQuote(sessionId)}' OR parent_thread_id = '${sqlQuote(sessionId)}'; DELETE FROM threads WHERE id = '${sqlQuote(sessionId)}';"; done`
        ),
        ...sessionIds.map(
          (sessionId) =>
            `sqlite3 "$HOME/.codex/logs_2.sqlite" "DELETE FROM logs WHERE thread_id = '${sqlQuote(sessionId)}';"`
        )
      ];
    case "pi":
      return detail.sourcePaths.map((path) => `rm ${dialogShellQuote(path)}`);
    default:
      return [];
  }
}
