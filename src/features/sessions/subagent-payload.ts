import type { SessionMessage } from "./types";

// 对应 pi.rs parse_subagent_run_block 写入的 payload 结构;
// details 来自 subagent 扩展,字段缺失时按未知处理。

export type SubagentRunUsage = {
  input?: number;
  output?: number;
  cacheRead?: number;
  cacheWrite?: number;
  contextTokens?: number;
  turns?: number;
  cost?: number | { total?: number };
};

export type SubagentRun = {
  agent?: string;
  agentSource?: string;
  task?: string;
  model?: string;
  stopReason?: string;
  exitCode?: number;
  // 两种失败形态存不同字段：启动失败进 stderr（exitCode 非 0），
  // 运行中报错进 errorMessage（exitCode 仍为 0，stopReason=error）
  stderr?: string;
  errorMessage?: string;
  usage?: SubagentRunUsage;
  nestedMessages: SessionMessage[];
};

export type SubagentRunPayload = {
  runs: SubagentRun[];
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function parseSubagentRunPayload(
  payload: unknown
): SubagentRunPayload | null {
  if (!isRecord(payload) || !Array.isArray(payload.runs) || payload.runs.length === 0) {
    return null;
  }

  const runs = payload.runs.filter(isRecord).map((run) => ({
    agent: typeof run.agent === "string" ? run.agent : undefined,
    agentSource: typeof run.agentSource === "string" ? run.agentSource : undefined,
    task: typeof run.task === "string" ? run.task : undefined,
    model: typeof run.model === "string" ? run.model : undefined,
    stopReason: typeof run.stopReason === "string" ? run.stopReason : undefined,
    exitCode: typeof run.exitCode === "number" ? run.exitCode : undefined,
    stderr: typeof run.stderr === "string" ? run.stderr : undefined,
    errorMessage:
      typeof run.errorMessage === "string" ? run.errorMessage : undefined,
    usage: isRecord(run.usage) ? (run.usage as SubagentRunUsage) : undefined,
    nestedMessages: Array.isArray(run.nestedMessages)
      ? (run.nestedMessages as SessionMessage[])
      : []
  }));

  if (runs.length === 0) {
    return null;
  }

  return { runs };
}

// exitCode 是子代理进程的真实退出码（实测取值 0/1/127），非 0 即失败。
// 入口行的聚合状态与弹窗内每个 run 的状态必须同源，否则两边会说不一致的话。
export function isFailedSubagentRun(run: SubagentRun): boolean {
  return (
    run.stopReason === "error" || (run.exitCode !== undefined && run.exitCode !== 0)
  );
}
