<script setup lang="ts">
import { computed, onMounted, onUnmounted, provide, ref, watch } from "vue";
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
import { captureKeepAliveScroll } from "./composables/useKeepAliveScroll";
import { sessionIdentityKey } from "./session-identity";
import { useSessionWorkspaceState } from "./composables/useSessionWorkspaceState";
import { joinClasses } from "@shared/lib/join-classes";
import { useLoadElapsed } from "@shared/lib/use-load-elapsed";
import { ALL_SOURCES, isSourceApp, type SourceSelection } from "./source-app";
import type { SessionPage, SessionSummary, SourceStatus } from "./types";
import { extractErrorMessage } from "@shared/lib/errors";
import { createRequestGuard } from "@shared/lib/request-guard";
import AppLoadError from "@shared/ui/AppLoadError.vue";

const SESSION_PAGE_SIZE = 20;
const SESSION_QUERY_DEBOUNCE_MS = 200;

const emit = defineEmits<{ openUsage: [] }>();

const sessionStore = useSessionWorkspaceState();
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
} = sessionStore;

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

// 列表与详情是两个整屏视图:列表页点会话进详情页,详情页的返回入口回到列表。
// 选中态仍留在 store 里,返回列表时那一行保持高亮,再次进入同一条会话不重拉。
const view = ref<"list" | "detail">("list");

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
// 数据落点是工作区状态工厂的列表字段(列表与选中摘要共用),加载位 / 竞态 / offset
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

// 来源检测是「点开历史会话」后的第一个加载阶段(扫描各来源目录),大库或
// 缓存失效时会等一段时间,这个居中加载页要给出实时耗时。
const sourceLoadElapsed = useLoadElapsed(loadingSources);

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

  // 接管式刷新:请求与旁路副作用(setSources、来源切换)在这里,
  // 加载位与成功回写交给状态机的 takeover。
  const request = sessionList.takeover();
  const isStale = request.isStale;

  refreshing.value = true;
  request.begin();
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

    // 后端合并视图返回 null，映射回前端的"全部"。切换来源时数据由
    // 来源监听自己重拉,不提交本次结果。
    const nextSelection: SourceSelection = result.selectedSource ?? ALL_SOURCES;
    if (nextSelection !== selectedSource.value) {
      sessionStore.setSelectedSource(nextSelection);
      return;
    }

    request.commit({
      items: result.page.sessions,
      nextOffset: result.page.nextOffset ?? result.page.sessions.length,
      hasMore: result.page.hasMore,
      totalCount: result.page.totalCount
    });
  } catch (error) {
    if (isStale()) {
      return;
    }
    listError.value = extractErrorMessage(error, "刷新会话失败。");
  } finally {
    if (!isStale()) {
      request.settle();
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

function handleQueryInput(event: Event) {
  sessionQuery.value = (event.target as HTMLInputElement).value;
}

function handleReverseClick() {
  reverseSessions.value = !reverseSessions.value;
}

function openSession(sessionKey: string) {
  // 列表→详情是组件内 v-if 切换,列表 DOM 会被销毁;趁它还在文档里抓拍滚动
  // 位置,返回列表时由 useKeepAliveScroll 的挂载回放恢复。
  captureKeepAliveScroll();
  sessionStore.setSelectedSessionKey(sessionKey);
  view.value = "detail";
}

function backToList() {
  view.value = "list";
}

// 会话统计是另一个顶层视图(用量统计),这里只做跳转,不复制它的状态。
function openUsageStats() {
  emit("openUsage");
}

function handleSessionDeleted(sessionKey: string | null) {
  clearSelectedSession({ invalidateDetailRequest: true });
  // 删除是磁盘上的既成事实:列表内存里直接摘掉这一条,滚动位置与筛选态
  // 原地保留,不再整页刷新。key 对不上的极端情况由下一次来源刷新兜底。
  if (sessionKey) {
    sessionStore.removeSession(sessionKey);
    sessionTotalCount.value = Math.max(0, sessionTotalCount.value - 1);
  }
  view.value = "list";
}
</script>

<template>
  <main class="sessions-shell">
    <!-- 列表页:整屏宽度,来源与筛选在顶部,列表自身滚动 -->
    <section v-if="view === 'list'" class="sessions-list-page">
      <div class="sessions-list-page__inner">
        <!-- 顶栏一行:来源切换 + 筛选/排序(整屏宽度下不再需要两行) -->
        <header class="sessions-list-header">
          <SourceSwitcher
            :disabled="
              refreshing ||
              loadingSources ||
              loadingSessions ||
              loadingMoreSessions
            "
            :refreshing="refreshing"
            :selected-source="selectedSource"
            :sources="sources"
            @change="handleSourceChange"
            @refresh="handleRefresh"
          />
          <div class="sessions-filter">
            <input
              aria-label="筛选会话"
              placeholder="按标题或 Session ID 模糊搜索"
              type="search"
              :value="sessionQuery"
              @input="handleQueryInput"
            />
            <button
              :aria-label="reverseSessions ? '切换为当前排序' : '切换为反向排序'"
              :class="joinClasses('sessions-filter__sort', reverseSessions && 'active')"
              :title="
                reverseSessions
                  ? '当前为反向排序，点击恢复默认排序'
                  : '当前为默认排序，点击切换为反向排序'
              "
              type="button"
              @click="handleReverseClick"
            >
              <svg
                aria-hidden="true"
                class="sessions-filter__icon"
                fill="none"
                viewBox="0 0 20 20"
              >
                <path
                  d="M7 4V16M7 16L4.5 13.5M7 16L9.5 13.5M13 16V4M13 4L10.5 6.5M13 4L15.5 6.5"
                  stroke="currentColor"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="1.7"
                />
              </svg>
            </button>
          </div>
          <button
            class="secondary-button sessions-stats-button"
            type="button"
            @click="openUsageStats"
          >
            <!-- lucide trending-up:与左栏"用量统计"同一个图标 -->
            <svg
              aria-hidden="true"
              class="sessions-stats-button__icon"
              fill="none"
              stroke="currentColor"
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              viewBox="0 0 24 24"
            >
              <path d="M22 7 13.5 15.5 8.5 10.5 2 17" />
              <path d="M16 7h6v6" />
            </svg>
            会话统计
          </button>
        </header>
        <AppLoadError
          v-if="sourceError"
          :message="sourceError"
          :retrying="loadingSources"
          @retry="retryDetectSources"
        />
        <AppLoadError
          v-if="listError"
          :message="listError"
          :retrying="loadingSessions"
          @retry="reloadSessionList"
        />
        <!-- 来源检测期间:加载动画放正文中心,不再挂在顶栏上 -->
        <div
          v-if="loadingSources"
          aria-label="正在检测会话来源"
          class="sessions-loading"
          role="status"
        >
          <span class="sessions-loading__bar" />
          <p class="sessions-loading__label">
            正在扫描历史会话
            <span class="sessions-loading__elapsed">已用时 {{ sourceLoadElapsed.text }}</span>
          </p>
        </div>
        <SessionList
          v-else
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
          :show-agent="selectedSource === ALL_SOURCES"
          :source-key="selectedSource"
          :total-count="sessionTotalCount"
          @load-more="handleLoadMoreSessions"
          @select="openSession"
        />
      </div>
    </section>

    <!-- 详情页:整屏展示整个会话;返回入口在常驻顶栏,正文自己滚动 -->
    <section v-else class="detail-page">
      <header class="detail-page__bar">
        <div class="detail-page__bar-inner">
          <button class="detail-page__back" type="button" @click="backToList">
            <span aria-hidden="true">←</span>
            返回会话列表
          </button>
        </div>
      </header>
      <div ref="contentPanelRef" class="content-panel">
        <div class="content-panel__inner">
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
        </div>
      </div>
    </section>
  </main>
</template>
