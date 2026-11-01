import { invoke } from "@tauri-apps/api/core";
import type { SourceSelection } from "./source-app";
import type {
  DeletePlan,
  DeleteSessionResult,
  SessionMessage,
  SessionPage,
  SessionEvent,
  SessionOverview,
  SessionRefreshResult,
  SourceApp,
  SourceStatus
} from "./types";

export function detectSources(): Promise<SourceStatus[]> {
  return invoke("detect_sources");
}

export function clearSessionCaches(): Promise<void> {
  return invoke("clear_session_caches");
}

type ListSessionsInput = {
  offset?: number;
  limit?: number;
  query?: string;
  reverse?: boolean;
  refresh?: boolean;
};

export function listSessions(
  sourceApp: SourceSelection,
  input: ListSessionsInput = {}
): Promise<SessionPage> {
  return invoke("list_sessions", {
    sourceApp,
    offset: input.offset,
    limit: input.limit,
    query: input.query,
    reverse: input.reverse,
    refresh: input.refresh
  });
}

export function refreshSessions(
  sourceApp: SourceSelection,
  input: Omit<ListSessionsInput, "refresh"> = {}
): Promise<SessionRefreshResult> {
  return invoke("refresh_sessions", {
    sourceApp,
    offset: input.offset,
    limit: input.limit,
    query: input.query,
    reverse: input.reverse
  });
}

export function getSessionOverview(
  sourceApp: SourceApp,
  sourceSessionId: string
): Promise<SessionOverview> {
  return invoke("get_session_overview", { sourceApp, sourceSessionId });
}

// 详情正文全量返回:时间线分组器需要完整消息流,不再分页。
export function getSessionMessages(
  sourceApp: SourceApp,
  sourceSessionId: string
): Promise<SessionMessage[]> {
  return invoke("get_session_messages", {
    sourceApp,
    sourceSessionId
  });
}

// 子代理弹窗取数：一次返回该 agent 的完整消息
export function getSessionAgentMessages(
  sourceApp: SourceApp,
  sourceSessionId: string,
  agentSessionId: string
): Promise<SessionMessage[]> {
  return invoke("get_session_agent_messages", {
    sourceApp,
    sourceSessionId,
    agentSessionId
  });
}

export function getSessionEvents(
  sourceApp: SourceApp,
  sourceSessionId: string
): Promise<SessionEvent[]> {
  return invoke("get_session_events", {
    sourceApp,
    sourceSessionId
  });
}

export function getDeletePlan(
  sourceApp: SourceApp,
  sourceSessionId: string
): Promise<DeletePlan> {
  return invoke("get_delete_plan", { sourceApp, sourceSessionId });
}

export function deleteSession(
  sourceApp: SourceApp,
  sourceSessionId: string
): Promise<DeleteSessionResult> {
  return invoke("delete_session", { sourceApp, sourceSessionId });
}
