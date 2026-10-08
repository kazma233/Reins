import { defineStore } from "pinia";
import type {
  DeletePlan,
  SessionOverview,
  SessionSummary,
  SourceApp,
  SourceStatus
} from "../types";
import { ALL_SOURCES, type SourceSelection } from "../source-app";

// 会话身份的唯一前端主键:(来源, source_sessionId) 复合键。transcript_path
// 只是展示字段,不参与去重、选中或导航。
export function sessionIdentityKey(
  sourceApp: SourceApp,
  sourceSessionId: string
): string {
  return `${sourceApp}:${sourceSessionId}`;
}

type SessionState = {
  sources: SourceStatus[];
  selectedSource: SourceSelection;
  sessions: SessionSummary[];
  selectedSessionKey: string | null;
  sessionOverview: SessionOverview | null;
  // 删除预演与 overview 同生命周期,由后端 plan 驱动删除入口与确认框。
  deletePlan: DeletePlan | null;
  loadingSources: boolean;
  loadingSessions: boolean;
  loadingDetail: boolean;
};

export const useSessionStore = defineStore("session", {
  state: (): SessionState => ({
    sources: [],
    // 默认展示跨来源合并视图。
    selectedSource: ALL_SOURCES,
    sessions: [],
    selectedSessionKey: null,
    sessionOverview: null,
    deletePlan: null,
    loadingSources: false,
    loadingSessions: false,
    loadingDetail: false
  }),
  actions: {
    setSources(sources: SourceStatus[]) {
      this.sources = sources;
    },
    setSelectedSource(source: SourceSelection) {
      if (this.selectedSource === source) {
        return;
      }

      // Switching source invalidates the current list and detail.
      this.selectedSource = source;
      this.sessions = [];
      this.selectedSessionKey = null;
      this.sessionOverview = null;
      this.deletePlan = null;
    },
    setSessions(sessions: SessionSummary[]) {
      this.sessions = sessions;
    },
    appendSessions(incomingSessions: SessionSummary[]) {
      // Dedup by session identity so paginated loads stay stable.
      const sessionsById = new Map(
        this.sessions.map((session) => [
          sessionIdentityKey(session.sourceApp, session.sourceSessionId),
          session
        ])
      );

      for (const session of incomingSessions) {
        sessionsById.set(
          sessionIdentityKey(session.sourceApp, session.sourceSessionId),
          session
        );
      }

      this.sessions = Array.from(sessionsById.values());
    },
    setSelectedSessionKey(sessionKey: string | null) {
      this.selectedSessionKey = sessionKey;
    },
    setSessionOverview(overview: SessionOverview | null) {
      this.sessionOverview = overview;
    },
    setDeletePlan(plan: DeletePlan | null) {
      this.deletePlan = plan;
    },
    setLoadingSources(value: boolean) {
      this.loadingSources = value;
    },
    setLoadingSessions(value: boolean) {
      this.loadingSessions = value;
    },
    setLoadingDetail(value: boolean) {
      this.loadingDetail = value;
    }
  }
});
