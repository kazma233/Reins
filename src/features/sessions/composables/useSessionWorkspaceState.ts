import { ref } from "vue";
import { sessionIdentityKey } from "../session-identity";
import { ALL_SOURCES, type SourceSelection } from "../source-app";
import type {
  DeletePlan,
  SessionOverview,
  SessionSummary,
  SourceStatus
} from "../types";

// 会话工作区的数据状态工厂:由 SessionWorkspace 调用一次,状态随组件实例
// 存活——KeepAlive 保留组件实例,列表数据、选中与筛选因此跨模式保留。
export function useSessionWorkspaceState() {
  const sources = ref<SourceStatus[]>([]);
  // 默认展示跨来源合并视图。
  const selectedSource = ref<SourceSelection>(ALL_SOURCES);
  const sessions = ref<SessionSummary[]>([]);
  const selectedSessionKey = ref<string | null>(null);
  const sessionOverview = ref<SessionOverview | null>(null);
  // 删除预演与 overview 同生命周期,由后端 plan 驱动删除入口与确认框。
  const deletePlan = ref<DeletePlan | null>(null);
  const loadingSources = ref(false);
  const loadingSessions = ref(false);
  const loadingDetail = ref(false);

  function setSources(nextSources: SourceStatus[]) {
    sources.value = nextSources;
  }

  function setSelectedSource(source: SourceSelection) {
    if (selectedSource.value === source) {
      return;
    }

    // Switching source invalidates the current list and detail.
    selectedSource.value = source;
    sessions.value = [];
    selectedSessionKey.value = null;
    sessionOverview.value = null;
    deletePlan.value = null;
  }

  function setSessions(nextSessions: SessionSummary[]) {
    sessions.value = nextSessions;
  }

  function appendSessions(incomingSessions: SessionSummary[]) {
    // Dedup by session identity so paginated loads stay stable.
    const sessionsById = new Map(
      sessions.value.map((session) => [
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

    sessions.value = Array.from(sessionsById.values());
  }

  function removeSession(sessionKey: string) {
    // 删除会话是磁盘上的既成事实,列表内存里同步摘掉即可;整页刷新
    // 交给用户主动的「刷新」或下次来源加载。
    sessions.value = sessions.value.filter(
      (session) =>
        sessionIdentityKey(session.sourceApp, session.sourceSessionId) !==
        sessionKey
    );
  }

  function setSelectedSessionKey(sessionKey: string | null) {
    selectedSessionKey.value = sessionKey;
  }

  function setSessionOverview(overview: SessionOverview | null) {
    sessionOverview.value = overview;
  }

  function setDeletePlan(plan: DeletePlan | null) {
    deletePlan.value = plan;
  }

  function setLoadingSources(value: boolean) {
    loadingSources.value = value;
  }

  function setLoadingSessions(value: boolean) {
    loadingSessions.value = value;
  }

  function setLoadingDetail(value: boolean) {
    loadingDetail.value = value;
  }

  return {
    sources,
    selectedSource,
    sessions,
    selectedSessionKey,
    sessionOverview,
    deletePlan,
    loadingSources,
    loadingSessions,
    loadingDetail,
    setSources,
    setSelectedSource,
    setSessions,
    appendSessions,
    removeSession,
    setSelectedSessionKey,
    setSessionOverview,
    setDeletePlan,
    setLoadingSources,
    setLoadingSessions,
    setLoadingDetail
  };
}
