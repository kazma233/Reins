import type { SessionOverview, SourceApp } from "./types";

// --- delete method copy + command preview ---

export type DeleteMethodCopy = {
  description: string;
  details: string[];
  commandLabel: string;
};

export function canDeleteSession(app: SourceApp): boolean {
  return app !== "zcode";
}

export const DELETE_METHOD_COPY: Record<SourceApp, DeleteMethodCopy> = {
  grokbuild: {
    description:
      "Grok Build 主会话走官方删除命令，官方够不到的子代理子会话由本地清理。",
    details: [
      "对主会话执行 grok sessions delete，官方一并清理其目录与搜索索引。",
      "子代理子会话目录及对应搜索索引行由本地清理。",
      "保留工作目录分组下的 prompt_history.jsonl 等共享文件。"
    ],
    commandLabel: "执行动作"
  },
  zcode: {
    description: "暂不支持删除 ZCode 会话。",
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
    description: "OpenCode 调用官方单会话删除命令，会话从列表中移除。",
    details: [
      "对当前会话组里的每个 session id 执行 opencode session delete。",
      "OpenCode v2 会级联删除子会话，已被级联删除的会话自动跳过。"
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

// 删除命令预览里的 POSIX shell 引号：用 '\'' 转义单引号
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
    case "zcode":
      return [];
    case "grokbuild": {
      // sourcePaths 指向各会话目录内的文件，取父目录去重即为子会话目录。
      const sessionDirs = Array.from(
        new Set(detail.sourcePaths.map((path) => path.slice(0, path.lastIndexOf("/"))))
      );
      const childDirs = sessionDirs.filter(
        (dir) => !dir.endsWith(`/${detail.summary.sourceSessionId}`)
      );
      const childIds = sessionIds.filter(
        (sessionId) => sessionId !== detail.summary.sourceSessionId
      );
      return [
        `grok sessions delete ${detail.summary.sourceSessionId}`,
        ...childDirs.map((dir) => `rm -rf ${dialogShellQuote(dir)}`),
        ...(childIds.length > 0
          ? [
              `sqlite3 "$HOME/.grok/sessions/session_search.sqlite" "DELETE FROM session_docs WHERE session_id IN (${childIds
                .map((sessionId) => `'${sqlQuote(sessionId)}'`)
                .join(", ")});"`
            ]
          : [])
      ];
    }
    case "opencode":
      return sessionIds.map(
        (sessionId) => `opencode session delete ${sessionId}`
      );
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
