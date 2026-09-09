import type { SourceApp } from "./types";

const SOURCE_APP_LABELS: Record<SourceApp, string> = {
  codex: "Codex",
  claude_code: "Claude Code",
  opencode: "OpenCode",
  pi: "Pi",
};

export function formatSourceAppName(app: SourceApp): string {
  return SOURCE_APP_LABELS[app] ?? app;
}

// 列表请求的来源选择：单一来源，或跨来源合并视图（全局分页）。
// "all" 是前端独有的选择值，与后端 list_sessions 的 source_app="all" 对应。
export type SourceSelection = SourceApp | "all";

// 显式收窄为 "all" 字面量：若声明为 SourceSelection，相等比较无法用于类型收窄。
export const ALL_SOURCES: "all" = "all";

export function isSourceApp(value: SourceSelection): value is SourceApp {
  return value !== ALL_SOURCES;
}

export function formatSourceSelectionLabel(selection: SourceSelection): string {
  return selection === ALL_SOURCES ? "全部" : formatSourceAppName(selection);
}
