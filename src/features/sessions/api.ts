import { invoke } from "@tauri-apps/api/core";
import type { SourceSelection } from "./source-app";
import type {
  DeleteSessionResult,
  ImportResult,
  ImportPreview,
  SessionPage,
  SessionEventPage,
  SessionOverview,
  SessionMessagePage,
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
  sourceSessionId: string,
  transcriptPath?: string
): Promise<SessionOverview> {
  return invoke("get_session_overview", { sourceApp, sourceSessionId, transcriptPath });
}

type GetSessionItemsInput = {
  offset?: number;
  limit?: number;
  transcriptPath?: string;
};

export function getSessionMessages(
  sourceApp: SourceApp,
  sourceSessionId: string,
  input: GetSessionItemsInput = {}
): Promise<SessionMessagePage> {
  return invoke("get_session_messages", {
    sourceApp,
    sourceSessionId,
    transcriptPath: input.transcriptPath,
    offset: input.offset,
    limit: input.limit
  });
}

export function getSessionEvents(
  sourceApp: SourceApp,
  sourceSessionId: string,
  input: GetSessionItemsInput = {}
): Promise<SessionEventPage> {
  return invoke("get_session_events", {
    sourceApp,
    sourceSessionId,
    transcriptPath: input.transcriptPath,
    offset: input.offset,
    limit: input.limit
  });
}

export function previewImport(
  sourceApp: SourceApp,
  sourceSessionId: string,
  targetApp: SourceApp,
  transcriptPath?: string
): Promise<ImportPreview> {
  return invoke("preview_import", {
    sourceApp,
    sourceSessionId,
    targetApp,
    transcriptPath
  });
}

export function importSession(
  sourceApp: SourceApp,
  sourceSessionId: string,
  targetApp: SourceApp,
  transcriptPath?: string
): Promise<ImportResult> {
  return invoke("import_session", {
    sourceApp,
    sourceSessionId,
    targetApp,
    transcriptPath
  });
}

export function deleteSession(
  sourceApp: SourceApp,
  sourceSessionId: string,
  transcriptPath?: string
): Promise<DeleteSessionResult> {
  return invoke("delete_session", {
    sourceApp,
    sourceSessionId,
    transcriptPath
  });
}
