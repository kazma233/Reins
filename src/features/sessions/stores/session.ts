import { defineStore } from "pinia";
import type {
  SessionOverview,
  SessionSummary,
  SourceStatus
} from "../types";
import { ALL_SOURCES, type SourceSelection } from "../source-app";

type SessionState = {
  sources: SourceStatus[];
  selectedSource: SourceSelection;
  sessions: SessionSummary[];
  selectedSessionKey: string | null;
  sessionOverview: SessionOverview | null;
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
    },
    setSessions(sessions: SessionSummary[]) {
      this.sessions = sessions;
    },
    appendSessions(incomingSessions: SessionSummary[]) {
      // Dedup by transcriptPath so paginated loads stay stable.
      const sessionsById = new Map(
        this.sessions.map((session) => [session.transcriptPath, session])
      );

      for (const session of incomingSessions) {
        sessionsById.set(session.transcriptPath, session);
      }

      this.sessions = Array.from(sessionsById.values());
    },
    setSelectedSessionKey(sessionKey: string | null) {
      this.selectedSessionKey = sessionKey;
    },
    setSessionOverview(overview: SessionOverview | null) {
      this.sessionOverview = overview;
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
