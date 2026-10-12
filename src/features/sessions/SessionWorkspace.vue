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
import { createPagedList, type PagedResult } from "./composables/createPagedList";
import { sessionIdentityKey, useSessionStore } from "./stores/session";
import { ALL_SOURCES, isSourceApp, type SourceSelection } from "./source-app";
import type { SessionPage, SessionSummary, SourceStatus } from "./types";
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
const refreshing = ref(false);
const detailReloadToken = ref(0);
const sessionQuery = ref("");
const debouncedSessionQuery = refDebounced(
  sessionQuery,
  SESSION_QUERY_DEBOUNCE_MS
);
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

// 详情流的竞态守卫;列表流的竞态由 createPagedList 的请求 key 承担。
const detailRequestGuard = createRequestGuard();
// 请求 key 只能区分同一实例内的新旧请求，卸载后仍需阻止在途响应写回 store。
let unmounted = false;
onUnmounted(() => {
  unmounted = true;
});

const selectedSummary = computed(() => {
  if (!selectedSessionKey.value) {
    return null;
  }

  const visibleSummary = sessions.value.find(
    (session) =>
      sessionIdentityKey(session.sourceApp, session.sourceSessionId) ===
      selectedSessionKey.value
  );

  if (visibleSummary) {
    return visibleSummary;
  }

  // 合并视图下保留的详情摘要可能来自任意来源，复合键自带来源，不会再误伤。
  const retainedSummary = sessionOverview.value?.summary ?? null;
  const retainedMatches =
    retainedSummary !== null &&
    sessionIdentityKey(
      retainedSummary.sourceApp,
      retainedSummary.sourceSessionId
    ) === selectedSessionKey.value;
  return retainedMatches ? retainedSummary : null;
});

function clearSelectedSession(options?: { invalidateDetailRequest?: boolean }) {
  if (options?.invalidateDetailRequest) {
    detailRequestGuard.invalidate();
  }
  sessionStore.setSelectedSessionKey(null);
  sessionStore.setSessionOverview(null);
  sessionStore.setDeletePlan(null);
  sessionStore.setLoadingDetail(false);
}

// --- session list: shared paged machine ---
// 数据落点是 pinia store(列表与选中摘要共用),加载位 / 竞态 / offset
// 推进由 createPagedList 承担;refresh 是接管式操作,经 invalidate +
// currentRequestKey 自查 stale。

const sessionList = createPagedList<SessionSummary>({
  requestKey: () =>
    unmounted
      ? null
      : `${selectedSource.value}::${debouncedSessionQuery.value}::${reverseSessions.value ? "desc" : "asc"}`,
  fetchPage: (offset) =>
    listSessions(selectedSource.value, {
      offset,
      limit: SESSION_PAGE_SIZE,
      query: debouncedSessionQuery.value,
      reverse: reverseSessions.value,
      refresh: false
    }).then((nextPage: SessionPage) => ({
      items: nextPage.sessions,
      nextOffset: nextPage.nextOffset ?? offset + nextPage.sessions.length,
      hasMore: nextPage.hasMore,
      totalCount: nextPage.totalCount
    })),
  apply: (result: PagedResult<SessionSummary>, mode) => {
    if (mode === "replace") {
      sessionStore.setSessions(result.items);
    } else {
      sessionStore.appendSessions(result.items);
    }
    sessionTotalCount.value = result.totalCount ?? result.items.length;
  },
  onError: (error, mode) => {
    if (mode === "replace") {
      listError.value = extractErrorMessage(error, "加载会话列表失败。");
    } else {
      loadMoreError.value = extractErrorMessage(error, "加载更多会话失败。");
    }
  }
});

// 模板沿用既有变量名;来源是状态机。
const hasMoreSessions = sessionList.hasMore;
const loadingMoreSessions = sessionList.loadingMore;
const sessionListResultKey = sessionList.resultKey;

// store 的 loadingSessions 是既有展示面(与 sessions 同居),由状态机单向同步。
watch(
  sessionList.loading,
  (value) => sessionStore.setLoadingSessions(value),
  { immediate: true }
);

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
  // 与收编前一致:重拉开始即清两个错误区,并复位可能在途的追加位。
  listError.value = null;
  loadMoreError.value = null;
  await sessionList.replace();
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
    () => selectedSummary.value?.sourceSessionId
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
          summary.sourceSessionId
        ).catch(() => null);
        const nextDetail = await getSessionOverview(
          summary.sourceApp,
          summary.sourceSessionId
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

function handleLoadMoreSessions() {
  if (!sessionList.canLoadMore.value) {
    return;
  }
  loadMoreError.value = null;
  void sessionList.loadMore();
}

async function handleRefresh() {
  if (loadingSources.value || loadingSessions.value || loadingMoreSessions.value || refreshing.value) {
    return;
  }

  // 接管式刷新:作废状态机的在途请求,后续用同一 key 空间自查 stale。
  sessionList.invalidate();
  const requestKey = sessionList.currentRequestKey();
  const isStale = () => sessionList.currentRequestKey() !== requestKey;

  refreshing.value = true;
  sessionList.loading.value = true;
  sessionList.loadingMore.value = false;
  listError.value = null;
  loadMoreError.value = null;
  detailRequestGuard.invalidate();
  detailReloadToken.value += 1;

  try {
    const result = await refreshSessions(selectedSource.value, {
      offset: 0,
      limit: SESSION_PAGE_SIZE,
      query: debouncedSessionQuery.value,
      reverse: reverseSessions.value
    });

    if (isStale()) {
      return;
    }

    sessionStore.setSources(result.sources);

    // 后端合并视图返回 null，映射回前端的"全部"。
    const nextSelection: SourceSelection = result.selectedSource ?? ALL_SOURCES;
    if (nextSelection !== selectedSource.value) {
      sessionStore.setSelectedSource(nextSelection);
      return;
    }

    sessionStore.setSessions(result.page.sessions);
    sessionTotalCount.value = result.page.totalCount;
    sessionList.nextOffset.value =
      result.page.nextOffset ?? result.page.sessions.length;
    sessionList.hasMore.value = result.page.hasMore;
    sessionList.resultKey.value = requestKey;
  } catch (error) {
    if (isStale()) {
      return;
    }
    listError.value = extractErrorMessage(error, "刷新会话失败。");
  } finally {
    if (!isStale()) {
      sessionList.loading.value = false;
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
