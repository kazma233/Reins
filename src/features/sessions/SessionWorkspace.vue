<script setup lang="ts">
import { computed, onMounted, onUnmounted, provide, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { refDebounced } from "@vueuse/core";
import "./styles/workspace.css";
import {
  detectSources,
  getDeletePlan,
  getSessionOverview,
  listSessions,
  refreshSessions
} from "./api";
import SessionDetail from "./components/SessionDetail.vue";
import SessionList from "./components/SessionList.vue";
import SourceSwitcher from "./components/SourceSwitcher.vue";
import { useSessionStore } from "./stores/session";
import { ALL_SOURCES, isSourceApp, type SourceSelection } from "./source-app";
import type { SessionPage, SourceStatus } from "./types";
import { extractErrorMessage } from "@shared/lib/errors";
import { createRequestGuard } from "@shared/lib/request-guard";
import AppLoadError from "@shared/ui/AppLoadError.vue";

const SESSION_PAGE_SIZE = 20;
const SESSION_QUERY_DEBOUNCE_MS = 200;

const sessionStore = useSessionStore();
const {
  deletePlan,
  loadingDetail,
  loadingSessions,
  loadingSources,
  selectedSessionKey,
  selectedSource,
  sessionOverview,
  sessions,
  sources
} = storeToRefs(sessionStore);

// --- local UI state ---

const sessionTotalCount = ref(0);
const nextSessionOffset = ref(0);
const hasMoreSessions = ref(false);
const loadingMoreSessions = ref(false);
const refreshing = ref(false);
const detailReloadToken = ref(0);
const sessionQuery = ref("");
const debouncedSessionQuery = refDebounced(
  sessionQuery,
  SESSION_QUERY_DEBOUNCE_MS
);
const sessionListResultKey = ref("");
const reverseSessions = ref(false);
// 错误态常驻在各自出错区域，直到重试成功或下一次加载开始。
const sourceError = ref<string | null>(null);
const listError = ref<string | null>(null);
const detailError = ref<string | null>(null);
const loadMoreError = ref<string | null>(null);
// Force-trigger the session list watcher once after source detection completes,
// so the list actually loads when the initial selectedSource happens to equal
// the store default (e.g. "codex") — setSelectedSource no-ops on equal values,
// so the watcher would otherwise never fire on first mount.
const bootstrapToken = ref(0);

const contentPanelRef = ref<HTMLElement | null>(null);
// Share the scroll container with SessionDetail via provide/inject — template
// refs auto-unwrap in <script setup>, so passing the ref as a prop would only
// forward the current value, not the reactive ref.
provide("sessionScrollContainer", contentPanelRef);

// Race-condition guards for the session list and detail loaders.
const sessionListRequestKeyRef = ref("");
const sessionListRequestVersionRef = ref(0);
const detailRequestGuard = createRequestGuard();
// 请求 key 只能区分同一实例内的新旧请求，卸载后仍需阻止在途响应写回 store。
let listRequestCancelled = false;
onUnmounted(() => {
  listRequestCancelled = true;
});

const selectedSummary = computed(() => {
  if (!selectedSessionKey.value) {
    return null;
  }

  const visibleSummary = sessions.value.find(
    (session) => session.transcriptPath === selectedSessionKey.value
  );

  if (visibleSummary) {
    return visibleSummary;
  }

  // 合并视图下保留的详情摘要可能来自任意来源，按当前来源过滤会误伤。
  const retainedSummary = sessionOverview.value?.summary;
  const retainedMatches =
    selectedSource.value === ALL_SOURCES
      ? retainedSummary?.transcriptPath === selectedSessionKey.value
      : retainedSummary?.sourceApp === selectedSource.value &&
        retainedSummary.transcriptPath === selectedSessionKey.value;
  return retainedMatches ? retainedSummary : null;
});

function nextSessionListRequestKey(
  source: SourceSelection,
  query: string,
  reverse: boolean
): string {
  sessionListRequestVersionRef.value += 1;
  return `${source}::${query}::${reverse ? "desc" : "asc"}::${sessionListRequestVersionRef.value}`;
}

function clearSelectedSession(options?: { invalidateDetailRequest?: boolean }) {
  if (options?.invalidateDetailRequest) {
    detailRequestGuard.invalidate();
  }
  sessionStore.setSelectedSessionKey(null);
  sessionStore.setSessionOverview(null);
  sessionStore.setDeletePlan(null);
  sessionStore.setLoadingDetail(false);
}

function applySessionPage(nextPage: SessionPage) {
  sessionStore.setSessions(nextPage.sessions);
  sessionTotalCount.value = nextPage.totalCount;
  nextSessionOffset.value = nextPage.nextOffset ?? nextPage.sessions.length;
  hasMoreSessions.value = nextPage.hasMore;
}

// --- bootstrap: detect sources on mount ---

async function retryDetectSources() {
  sessionStore.setLoadingSources(true);
  sourceError.value = null;

  try {
    const nextSources = await detectSources();
    sessionStore.setSources(nextSources);
  } catch (error) {
    sourceError.value = extractErrorMessage(error, "加载来源失败。");
  } finally {
    sessionStore.setLoadingSources(false);
  }
}

onMounted(async () => {
  await retryDetectSources();
  // Bump after source detection so the watcher fires even when the initial
  // source equals the store default (setSelectedSource no-ops on equal values).
  bootstrapToken.value += 1;
});

// --- load sessions when source / query / order changes ---

async function reloadSessionList() {
  const source = selectedSource.value;
  const query = debouncedSessionQuery.value;
  const reverse = reverseSessions.value;
  const requestKey = nextSessionListRequestKey(source, query, reverse);
  sessionListRequestKeyRef.value = requestKey;

  const isStale = () =>
    listRequestCancelled || sessionListRequestKeyRef.value !== requestKey;

  sessionStore.setLoadingSessions(true);
  loadingMoreSessions.value = false;
  listError.value = null;
  loadMoreError.value = null;

  try {
    const nextPage = await listSessions(source, {
      offset: 0,
      limit: SESSION_PAGE_SIZE,
      query,
      reverse,
      refresh: false
    });

    if (isStale()) {
      return;
    }

    applySessionPage(nextPage);
    sessionListResultKey.value = requestKey;
  } catch (error) {
    if (isStale()) {
      return;
    }
    listError.value = extractErrorMessage(error, "加载会话列表失败。");
  } finally {
    if (!isStale()) {
      sessionStore.setLoadingSessions(false);
    }
  }
}

watch(
  [selectedSource, debouncedSessionQuery, reverseSessions, bootstrapToken],
  () => {
    void reloadSessionList();
  }
);

// --- load detail when selection changes ---

watch(
  [
    detailReloadToken,
    selectedSessionKey,
    selectedSource,
    () => selectedSummary.value?.sourceSessionId,
    () => selectedSummary.value?.transcriptPath
  ],
  (_new, _old, onCleanup) => {
    let cancelled = false;
    const requestId = detailRequestGuard.next();
    detailError.value = null;

    async function loadDetail() {
      if (!selectedSessionKey.value) {
        sessionStore.setSessionOverview(null);
        sessionStore.setDeletePlan(null);
        sessionStore.setLoadingDetail(false);
        return;
      }

      const summary = selectedSummary.value;
      // 合并视图下详情按会话自身的来源加载，只有单一来源才要求来源一致。
      if (
        !summary ||
        (isSourceApp(selectedSource.value) &&
          summary.sourceApp !== selectedSource.value)
      ) {
        sessionStore.setSessionOverview(null);
        sessionStore.setDeletePlan(null);
        sessionStore.setLoadingDetail(false);
        return;
      }

      sessionStore.setLoadingDetail(true);
      sessionStore.setSessionOverview({
        summary,
        sourcePaths: [summary.transcriptPath],
        messageCount: null,
        eventCount: null,
        agents: []
      });
      sessionStore.setDeletePlan(null);

      try {
        // 删除计划与 overview 并行获取；计划失败只隐藏删除入口，不阻断详情。
        const planPromise = getDeletePlan(
          summary.sourceApp,
          summary.sourceSessionId,
          summary.transcriptPath
        ).catch(() => null);
        const nextDetail = await getSessionOverview(
          summary.sourceApp,
          summary.sourceSessionId,
          summary.transcriptPath
        );
        const nextPlan = await planPromise;

        if (cancelled || !detailRequestGuard.isLatest(requestId)) {
          return;
        }

        sessionStore.setSessionOverview(nextDetail);
        sessionStore.setDeletePlan(nextPlan);
      } catch (error) {
        if (cancelled || !detailRequestGuard.isLatest(requestId)) {
          return;
        }
        detailError.value = extractErrorMessage(
          error,
          "加载会话详情失败。"
        );
      } finally {
        if (!cancelled && detailRequestGuard.isLatest(requestId)) {
          sessionStore.setLoadingDetail(false);
        }
      }
    }

    void loadDetail();

    onCleanup(() => {
      cancelled = true;
    });
  }
);

// --- user actions ---

function retryLoadDetail() {
  detailReloadToken.value += 1;
}

async function handleLoadMoreSessions() {
  if (loadingSessions.value || loadingMoreSessions.value || !hasMoreSessions.value) {
    return;
  }

  const sourceAtRequest = selectedSource.value;
  const offsetAtRequest = nextSessionOffset.value;
  const queryAtRequest = debouncedSessionQuery.value;
  const reverseAtRequest = reverseSessions.value;
  const requestKey = sessionListRequestKeyRef.value;

  if (!requestKey) {
    return;
  }

  loadMoreError.value = null;
  loadingMoreSessions.value = true;

  try {
    const nextPage = await listSessions(sourceAtRequest, {
      offset: offsetAtRequest,
      limit: SESSION_PAGE_SIZE,
      query: queryAtRequest,
      reverse: reverseAtRequest,
      refresh: false
    });

    if (sessionListRequestKeyRef.value !== requestKey) {
      return;
    }

    sessionStore.appendSessions(nextPage.sessions);
    sessionTotalCount.value = nextPage.totalCount;
    nextSessionOffset.value =
      nextPage.nextOffset ?? offsetAtRequest + nextPage.sessions.length;
    hasMoreSessions.value = nextPage.hasMore;
  } catch (error) {
    if (sessionListRequestKeyRef.value !== requestKey) {
      return;
    }
    loadMoreError.value = extractErrorMessage(error, "加载更多会话失败。");
  } finally {
    if (sessionListRequestKeyRef.value === requestKey) {
      loadingMoreSessions.value = false;
    }
  }
}

async function handleRefresh() {
  if (loadingSources.value || loadingSessions.value || loadingMoreSessions.value || refreshing.value) {
    return;
  }

  const sourceAtRequest = selectedSource.value;
  const queryAtRequest = debouncedSessionQuery.value;
  const reverseAtRequest = reverseSessions.value;
  const requestKey = nextSessionListRequestKey(
    sourceAtRequest,
    queryAtRequest,
    reverseAtRequest
  );
  sessionListRequestKeyRef.value = requestKey;

  refreshing.value = true;
  sessionStore.setLoadingSessions(true);
  loadingMoreSessions.value = false;
  listError.value = null;
  loadMoreError.value = null;
  detailRequestGuard.invalidate();
  detailReloadToken.value += 1;

  try {
    const result = await refreshSessions(sourceAtRequest, {
      offset: 0,
      limit: SESSION_PAGE_SIZE,
      query: queryAtRequest,
      reverse: reverseAtRequest
    });

    if (sessionListRequestKeyRef.value !== requestKey) {
      return;
    }

    sessionStore.setSources(result.sources);

    // 后端合并视图返回 null，映射回前端的"全部"。
    const nextSelection: SourceSelection = result.selectedSource ?? ALL_SOURCES;
    if (nextSelection !== sourceAtRequest) {
      sessionStore.setSelectedSource(nextSelection);
      return;
    }

    applySessionPage(result.page);
    sessionListResultKey.value = requestKey;
  } catch (error) {
    if (sessionListRequestKeyRef.value !== requestKey) {
      return;
    }
    listError.value = extractErrorMessage(error, "刷新会话失败。");
  } finally {
    if (sessionListRequestKeyRef.value === requestKey) {
      sessionStore.setLoadingSessions(false);
    }
    refreshing.value = false;
  }
}

function handleSourceChange(nextSource: SourceSelection) {
  if (nextSource === selectedSource.value) {
    return;
  }
  sessionStore.setSelectedSource(nextSource);
}

async function handleSessionDeleted() {
  clearSelectedSession({ invalidateDetailRequest: true });
  await handleRefresh();
}
</script>

<template>
  <main class="app-shell">
    <aside class="sidebar">
      <div class="sidebar-fixed">
        <header class="sidebar-header">
          <SourceSwitcher
            :disabled="
              refreshing ||
              loadingSources ||
              loadingSessions ||
              loadingMoreSessions
            "
            :loading="loadingSources"
            :refreshing="refreshing"
            :selected-source="selectedSource"
            :sources="sources"
            @change="handleSourceChange"
            @refresh="handleRefresh"
          />
        </header>
        <AppLoadError
          v-if="sourceError"
          :message="sourceError"
          :retrying="loadingSources"
          @retry="retryDetectSources"
        />
      </div>
      <AppLoadError
        v-if="listError"
        :message="listError"
        :retrying="loadingSessions"
        @retry="reloadSessionList"
      />
      <SessionList
        :has-more="hasMoreSessions"
        :load-more-error="loadMoreError"
        :loading="loadingSessions"
        :loading-more="loadingMoreSessions"
        :query="sessionQuery"
        :query-key="debouncedSessionQuery"
        :result-key="sessionListResultKey"
        :reverse="reverseSessions"
        :selected-session-key="selectedSessionKey"
        :sessions="sessions"
        :source-key="selectedSource"
        :total-count="sessionTotalCount"
        @load-more="handleLoadMoreSessions"
        @query-change="(q: string) => (sessionQuery = q)"
        @reverse-change="(r: boolean) => (reverseSessions = r)"
        @select="(k: string) => sessionStore.setSelectedSessionKey(k)"
      />
    </aside>
    <section ref="contentPanelRef" class="content-panel">
      <AppLoadError
        v-if="detailError"
        :message="detailError"
        :retrying="loadingDetail"
        @retry="retryLoadDetail"
      />
      <SessionDetail
        v-else
        :delete-plan="deletePlan"
        :loading="loadingDetail"
        :overview="sessionOverview"
        @deleted="handleSessionDeleted"
      />
    </section>
  </main>
</template>
