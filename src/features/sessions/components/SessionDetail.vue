<script setup lang="ts">
import { computed, inject, ref, toRef, watch, type Ref } from "vue";
import { refDebounced } from "@vueuse/core";
import { getSessionAgentMessages } from "../api";
import { useSessionDetailActions } from "../composables/useSessionDetailActions";
import { useSessionTimeline } from "../composables/useSessionTimeline";
import {
  agentDisplayLabel,
  emptyEventText,
  emptyMessageText,
  formatVisibleEventLabel,
  formatVisibleMessageLabel,
  isNearBottom,
  normalizeFilterValue,
  prepareTimelineEvent
} from "../composables/session-detail-helpers";
import {
  buildTimelineItems,
  itemSearchText,
  type TimelineItem
} from "../timeline-group";
import { extractErrorMessage } from "@shared/lib/errors";
import { createRequestGuard } from "@shared/lib/request-guard";
import { formatTimestamp } from "@shared/lib/format";
import { canDeleteSession } from "../model";
import { formatSourceAppName } from "../source-app";
import SessionDetailDialogs from "./SessionDetailDialogs.vue";
import MessageTimeline from "./MessageTimeline.vue";
import EventTimeline from "./EventTimeline.vue";
import type { SessionAgent, SessionOverview, SourceApp } from "../types";
import SubagentGroupDialog from "./SubagentGroupDialog.vue";
import "./session-detail.css";

type SessionDetailProps = {
  overview: SessionOverview | null;
  loading: boolean;
};

const props = defineProps<SessionDetailProps>();

const emit = defineEmits<{
  deleted: [];
}>();

// The scroll container ref is provided by SessionWorkspace so we can attach
// passive scroll listeners for infinite-load behaviour.
const scrollContainer = inject<Ref<HTMLElement | null>>(
  "sessionScrollContainer",
  ref(null)
);

// --- local state ---

const timelineTab = ref<"messages" | "events">("messages");
const timelineFilter = ref("");
const deferredTimelineFilter = refDebounced(timelineFilter, 300);

// --- timeline ---

const overviewRef = toRef(props, "overview");
const {
  detailKey,
  events,
  eventsLoading,
  eventsLoadingMore,
  eventError,
  loadMoreEvents,
  loadMoreMessages,
  messageError,
  messages,
  messagesLoading,
  messagesLoadingMore,
  nextEventOffset,
  nextMessageOffset
} = useSessionTimeline(overviewRef);

// --- import / delete flows ---

const {
  targetApp,
  preview,
  importResult,
  previewError,
  importError,
  deleteError,
  previewLoading,
  importLoading,
  deleteLoading,
  deleteDialogOpen,
  importDialogOpen,
  previewReady,
  openImportDialog,
  closeImportDialog,
  closeImportResultDialog,
  handleImport,
  openDeleteDialog,
  closeDeleteDialog,
  handleDelete
} = useSessionDetailActions(overviewRef, detailKey, () => emit("deleted"));

// Reset UI toggles when the overview changes (dialog state resets inside
// useSessionDetailActions).
watch(overviewRef, () => {
  timelineFilter.value = "";
  closeSubagentDialog();
});

// --- derived state ---

const activeDetail = computed(() => props.overview);
const normalizedTimelineFilter = computed(() =>
  normalizeFilterValue(deferredTimelineFilter.value)
);

const agentOptions = computed(() => {
  const detail = activeDetail.value;
  if (!detail) {
    return [];
  }
  // 子代理 tab 只是打开对应过程弹窗的入口,不做时间线筛选;
  // 根会话消息始终展示(Pi 等单 agent 来源过滤后为空,tab 栏整体隐藏)
  return detail.agents.filter((agent) => !agent.isRoot);
});

const filteredMessages = computed(() => messages.value);

const preparedEvents = computed(() => events.value.map(prepareTimelineEvent));

// 文档流时间线：消息 → 渲染单元（文本段/工具行/思考行/子代理入口）
const timelineItems = computed(() => buildTimelineItems(filteredMessages.value));

const visibleItems = computed(() => {
  let list = timelineItems.value;
  const filter = normalizedTimelineFilter.value;
  if (filter.length > 0) {
    list = list.filter((item) => itemSearchText(item).includes(filter));
  }
  return list;
});

const filteredEvents = computed(() => {
  const filter = normalizedTimelineFilter.value;
  if (filter.length === 0) {
    return preparedEvents.value;
  }
  return preparedEvents.value.filter((event) =>
    event.searchText.includes(filter)
  );
});

const visibleMessageLabel = computed(() => {
  if (!activeDetail.value) {
    return "";
  }
  return formatVisibleMessageLabel(
    visibleItems.value.length,
    timelineItems.value.length,
    deferredTimelineFilter.value.trim()
  );
});

const visibleEventLabel = computed(() => {
  if (!activeDetail.value) {
    return "";
  }
  return formatVisibleEventLabel(
    filteredEvents.value.length,
    nextEventOffset.value,
    deferredTimelineFilter.value.trim()
  );
});

const emptyVisibleMessageText = computed(() =>
  emptyMessageText(deferredTimelineFilter.value.trim())
);

const emptyVisibleEventText = computed(() =>
  emptyEventText(deferredTimelineFilter.value.trim())
);

const rootSessionId = computed(
  () => activeDetail.value?.summary.sourceSessionId ?? ""
);

// 子代理过程弹窗：入口行与顶部 tab 共用同一实例。
// 内容按需从后端取该 agent 的完整消息，不依赖时间线分页已加载范围。
type SubagentDialogState = {
  sessionId: string;
  label: string;
  loading: boolean;
  error: string | null;
  items: TimelineItem[];
};

const subagentDialog = ref<SubagentDialogState | null>(null);
const subagentRequestGuard = createRequestGuard();

async function openSubagentDialog(sessionId: string, label: string) {
  const detail = activeDetail.value;
  if (!detail) {
    return;
  }

  const requestId = subagentRequestGuard.next();
  subagentDialog.value = { sessionId, label, loading: true, error: null, items: [] };

  try {
    const agentMessages = await getSessionAgentMessages(
      detail.summary.sourceApp,
      detail.summary.sourceSessionId,
      sessionId,
      detail.summary.transcriptPath
    );

    if (!subagentRequestGuard.isLatest(requestId)) {
      return;
    }

    // 子代理消息里带上标记消息,分组后只剩入口项;取其内部条目作为弹窗内容
    const group = buildTimelineItems(agentMessages).find(
      (item) => item.kind === "subagent-group"
    );
    subagentDialog.value = {
      sessionId,
      label,
      loading: false,
      error: null,
      items: group && group.kind === "subagent-group" ? group.items : []
    };
  } catch (error) {
    if (!subagentRequestGuard.isLatest(requestId)) {
      return;
    }
    subagentDialog.value = {
      sessionId,
      label,
      loading: false,
      error: extractErrorMessage(error, "加载子代理内容失败。"),
      items: []
    };
  }
}

function closeSubagentDialog() {
  subagentRequestGuard.invalidate();
  subagentDialog.value = null;
}

function handleAgentTabClick(agent: SessionAgent) {
  // tab 只是弹窗入口:不改变下方时间线的内容,时间线始终展示全部消息
  void openSubagentDialog(agent.sessionId, agent.label);
}

const timelineFilterPlaceholder = computed(() =>
  timelineTab.value === "messages"
    ? "按消息内容、工具名、块类型筛选"
    : "按事件类型、摘要、载荷筛选"
);

// --- auto-load more when the timeline needs more data ---

watch(
  [
    activeDetail,
    timelineTab,
    messagesLoading,
    messagesLoadingMore,
    nextMessageOffset,
    normalizedTimelineFilter,
    () => visibleItems.value.length
  ],
  () => {
    const detail = activeDetail.value;
    if (
      !detail ||
      timelineTab.value !== "messages" ||
      messagesLoading.value ||
      messagesLoadingMore.value ||
      nextMessageOffset.value === null
    ) {
      return;
    }
    if (visibleItems.value.length > 0) {
      return;
    }
    void loadMoreMessages();
  }
);

watch(
  [
    activeDetail,
    timelineTab,
    eventsLoading,
    eventsLoadingMore,
    nextEventOffset,
    normalizedTimelineFilter,
    () => events.value.length,
    () => filteredEvents.value.length
  ],
  () => {
    const detail = activeDetail.value;
    if (
      !detail ||
      timelineTab.value !== "events" ||
      eventsLoading.value ||
      eventsLoadingMore.value ||
      nextEventOffset.value === null
    ) {
      return;
    }

    const needsInitialEventPage =
      events.value.length === 0 && nextEventOffset.value === 0;
    const needsMoreEventsForFilter =
      filteredEvents.value.length === 0 &&
      normalizedTimelineFilter.value.length > 0;

    if (!needsInitialEventPage && !needsMoreEventsForFilter) {
      return;
    }
    void loadMoreEvents();
  }
);

// --- scroll-driven load more ---

function handleScrollLoadMore() {
  const element = scrollContainer.value;
  if (!element || !isNearBottom(element)) {
    return;
  }

  if (timelineTab.value === "messages") {
    if (
      nextMessageOffset.value !== null &&
      !messagesLoading.value &&
      !messagesLoadingMore.value
    ) {
      void loadMoreMessages();
    }
    return;
  }

  if (
    nextEventOffset.value !== null &&
    !eventsLoading.value &&
    !eventsLoadingMore.value
  ) {
    void loadMoreEvents();
  }
}

watch(
  () => scrollContainer.value,
  (element, _oldElement, onCleanup) => {
    if (!element) {
      return;
    }
    const onScroll = () => handleScrollLoadMore();
    element.addEventListener("scroll", onScroll, { passive: true });
    onCleanup(() => element.removeEventListener("scroll", onScroll));
  },
  { immediate: true }
);
</script>

<template>
  <!-- Skeleton while loading without overview -->
  <div v-if="!overview && loading" class="detail-panel">
    <section class="detail-section skeleton-card">
      <span class="skeleton-line skeleton-title" />
      <span class="skeleton-line" />
      <div class="summary-grid">
        <div v-for="(_, index) in 4" :key="index">
          <span class="skeleton-line skeleton-meta" />
          <span class="skeleton-line" />
        </div>
        <div
          v-for="(_, index) in 2"
          :key="`wide-${index}`"
          class="summary-grid__wide"
        >
          <span class="skeleton-line skeleton-meta" />
          <span class="skeleton-line" />
        </div>
      </div>
    </section>
    <section class="detail-section skeleton-card">
      <span class="skeleton-line skeleton-title" />
      <span class="skeleton-line" />
      <span class="skeleton-line skeleton-short" />
    </section>
    <section class="detail-section skeleton-card">
      <span class="skeleton-line skeleton-title" />
      <span class="skeleton-line" />
      <span class="skeleton-line" />
      <span class="skeleton-line skeleton-short" />
    </section>
  </div>

  <!-- Empty state -->
  <div v-else-if="!overview" class="detail-empty">
    请选择一个会话以查看其转录内容和元数据。
  </div>

  <!-- Detail panel -->
  <div v-else class="detail-panel panel-shell" :aria-busy="loading">
    <section class="detail-section">
      <div class="detail-header">
        <div class="detail-header-info">
          <h2>{{ overview.summary.title }}</h2>
          <p :title="overview.summary.sourceSessionId">
            {{ overview.summary.sourceSessionId }}
          </p>
        </div>
        <div class="detail-header-actions">
          <button
            class="primary-button"
            :disabled="importLoading || deleteLoading"
            type="button"
            @click="openImportDialog"
          >
            导入预览
          </button>
          <button
            v-if="canDeleteSession(overview.summary.sourceApp)"
            class="danger-button"
            :disabled="deleteLoading || importLoading"
            type="button"
            @click="openDeleteDialog"
          >
            {{ deleteLoading ? "删除中..." : "删除会话" }}
          </button>
        </div>
      </div>
      <div v-if="importError" class="error-box">
        <strong>导入失败</strong>
        <pre class="error-text">{{ importError }}</pre>
      </div>
      <div v-if="deleteError" class="error-box">
        <strong>删除失败</strong>
        <pre class="error-text">{{ deleteError }}</pre>
      </div>
      <dl class="summary-grid">
        <div>
          <dt>来源</dt>
          <dd>{{ formatSourceAppName(overview.summary.sourceApp) }}</dd>
        </div>
        <div>
          <dt>分支</dt>
          <dd>{{ overview.summary.gitBranch ?? "未知" }}</dd>
        </div>
        <div>
          <dt>创建时间</dt>
          <dd>{{ formatTimestamp(overview.summary.createdAt) }}</dd>
        </div>
        <div>
          <dt>更新时间</dt>
          <dd>{{ formatTimestamp(overview.summary.updatedAt) }}</dd>
        </div>
        <div class="summary-grid__wide">
          <dt>工作目录</dt>
          <dd>{{ overview.summary.cwd ?? "未知" }}</dd>
        </div>
        <div class="summary-grid__wide">
          <dt>转录文件</dt>
          <dd>{{ overview.summary.transcriptPath }}</dd>
        </div>
      </dl>
    </section>

    <section class="detail-section">
      <div class="timeline-controls">
        <div class="timeline-tab-bar">
          <button
            :class="`timeline-tab${timelineTab === 'messages' ? ' active' : ''}`"
            type="button"
            @click="timelineTab = 'messages'"
          >
            消息
          </button>
          <button
            :class="`timeline-tab${timelineTab === 'events' ? ' active' : ''}`"
            type="button"
            @click="timelineTab = 'events'"
          >
            事件
          </button>
          <input
            v-model="timelineFilter"
            class="timeline-search-input"
            :placeholder="timelineFilterPlaceholder"
            type="search"
          />
        </div>
        <div v-if="agentOptions.length > 0" class="agent-tab-group">
          <button
            v-for="agent in agentOptions"
            :key="agent.sessionId"
            class="timeline-tab"
            type="button"
            @click="handleAgentTabClick(agent)"
          >
            {{ agentDisplayLabel(agent.sessionId, agentOptions) }}
          </button>
        </div>
      </div>

      <MessageTimeline
        v-if="timelineTab === 'messages'"
        :active-detail="overview"
        :empty-text="emptyVisibleMessageText"
        :items="visibleItems"
        :message-error="messageError"
        :messages-loading="messagesLoading"
        :visible-message-label="visibleMessageLabel"
        @open-subagent="
          (sessionId: string, label: string) => openSubagentDialog(sessionId, label)
        "
      />
      <EventTimeline
        v-else
        :agent-options="agentOptions"
        :empty-text="emptyVisibleEventText"
        :event-error="eventError"
        :events="events"
        :events-loading="eventsLoading"
        :filtered-events="filteredEvents"
        :next-event-offset="nextEventOffset"
        :root-session-id="rootSessionId"
        :visible-event-label="visibleEventLabel"
      />
    </section>

    <SubagentGroupDialog
      :open="subagentDialog !== null"
      :label="subagentDialog?.label ?? ''"
      :loading="subagentDialog?.loading ?? false"
      :error="subagentDialog?.error ?? null"
      :items="subagentDialog?.items ?? []"
      @close="closeSubagentDialog"
    />

    <SessionDetailDialogs
      :overview="overview"
      :delete-dialog-open="deleteDialogOpen"
      :delete-loading="deleteLoading"
      :import-dialog-open="importDialogOpen"
      :import-loading="importLoading"
      :import-result="importResult"
      :preview="preview"
      :preview-error="previewError"
      :preview-loading="previewLoading"
      :preview-ready="previewReady"
      :target-app="targetApp"
      @close-delete-dialog="closeDeleteDialog"
      @close-import-dialog="closeImportDialog"
      @close-import-result-dialog="closeImportResultDialog"
      @confirm-delete="handleDelete"
      @confirm-import="handleImport"
      @target-app-change="(app: SourceApp) => (targetApp = app)"
    />

    <div v-if="loading" class="panel-loading-overlay">
      <div class="loading-pill">正在加载会话详情...</div>
    </div>
  </div>
</template>
