<script setup lang="ts">
import { computed, inject, ref, toRef, watch, type Ref } from "vue";
import { refDebounced } from "@vueuse/core";
import { useSessionDetailActions } from "../composables/useSessionDetailActions";
import { useSessionTimeline } from "../composables/useSessionTimeline";
import {
  agentDisplayLabel,
  emptyEventText,
  emptyMessageText,
  eventMatchesAgent,
  fallbackAgent,
  formatVisibleEventLabel,
  formatVisibleMessageLabel,
  isNearBottom,
  messageMatchesAgent,
  normalizeFilterValue,
  prepareTimelineEvent,
  prepareTimelineMessage,
  toVisibleTimelineMessage
} from "../composables/session-detail-helpers";
import { formatTimestamp } from "@shared/lib/format";
import AppCheckbox from "@shared/ui/AppCheckbox.vue";
import { formatSourceAppName } from "../source-app";
import SessionDetailDialogs from "./SessionDetailDialogs.vue";
import MessageTimeline from "./MessageTimeline.vue";
import EventTimeline from "./EventTimeline.vue";
import type { SessionOverview, SourceApp } from "../types";
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

const expandedMessageIds = ref<Record<string, boolean>>({});
const conversationOnly = ref(true);
const timelineTab = ref<"messages" | "events">("messages");
const selectedAgentId = ref<string | "all">("all");
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
  expandedMessageIds.value = {};
  timelineFilter.value = "";
  selectedAgentId.value = "all";
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
  if (detail.agents.length > 0) {
    return detail.agents;
  }
  return [fallbackAgent(detail)];
});

const preparedMessages = computed(() =>
  messages.value.map(prepareTimelineMessage)
);
const preparedEvents = computed(() => events.value.map(prepareTimelineEvent));

const filteredMessages = computed(() => {
  const detail = activeDetail.value;
  if (!detail) {
    return [];
  }
  return preparedMessages.value.filter(({ message }) =>
    messageMatchesAgent(message, selectedAgentId.value, detail.summary.sourceSessionId)
  );
});

const timelineMessages = computed(() =>
  filteredMessages.value.flatMap((message) => {
    const visible = toVisibleTimelineMessage(message, conversationOnly.value);
    return visible ? [visible] : [];
  })
);

const visibleMessages = computed(() => {
  const filter = normalizedTimelineFilter.value;
  if (filter.length === 0) {
    return timelineMessages.value;
  }
  return timelineMessages.value.filter((message) =>
    message.searchText.includes(filter)
  );
});

const agentFilteredEvents = computed(() => {
  const detail = activeDetail.value;
  if (!detail) {
    return [];
  }
  return preparedEvents.value.filter(({ event }) =>
    eventMatchesAgent(event, selectedAgentId.value, detail.summary.sourceSessionId)
  );
});

const filteredEvents = computed(() => {
  const filter = normalizedTimelineFilter.value;
  if (filter.length === 0) {
    return agentFilteredEvents.value;
  }
  return agentFilteredEvents.value.filter((event) =>
    event.searchText.includes(filter)
  );
});

const selectedAgentLabel = computed(() => {
  const detail = activeDetail.value;
  if (!detail) {
    return "";
  }
  if (selectedAgentId.value === "all") {
    return "全部 Agent";
  }
  return agentDisplayLabel(selectedAgentId.value, agentOptions.value);
});

const visibleMessageLabel = computed(() => {
  if (!activeDetail.value) {
    return "";
  }
  return formatVisibleMessageLabel(
    selectedAgentLabel.value,
    conversationOnly.value,
    visibleMessages.value.length,
    timelineMessages.value.length,
    deferredTimelineFilter.value.trim()
  );
});

const visibleEventLabel = computed(() => {
  if (!activeDetail.value) {
    return "";
  }
  return formatVisibleEventLabel(
    selectedAgentLabel.value,
    filteredEvents.value.length,
    agentFilteredEvents.value.length,
    nextEventOffset.value,
    deferredTimelineFilter.value.trim()
  );
});

const emptyVisibleMessageText = computed(() =>
  emptyMessageText(
    selectedAgentLabel.value,
    conversationOnly.value,
    deferredTimelineFilter.value.trim(),
    filteredMessages.value.length
  )
);

const emptyVisibleEventText = computed(() =>
  emptyEventText(selectedAgentLabel.value, deferredTimelineFilter.value.trim())
);

const rootSessionId = computed(
  () => activeDetail.value?.summary.sourceSessionId ?? ""
);

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
    selectedAgentId,
    () => visibleMessages.value.length
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
    if (visibleMessages.value.length > 0) {
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
    selectedAgentId,
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
      (selectedAgentId.value !== "all" || normalizedTimelineFilter.value.length > 0);

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

// --- message expand toggle ---

function toggleMessageExpanded(messageKey: string) {
  expandedMessageIds.value = {
    ...expandedMessageIds.value,
    [messageKey]: !expandedMessageIds.value[messageKey]
  };
}
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
          <AppCheckbox
            v-if="timelineTab === 'messages'"
            v-model="conversationOnly"
            class-name="timeline-toggle-control"
          >
            仅看对话
          </AppCheckbox>
        </div>
        <div v-if="agentOptions.length > 1" class="agent-tab-group">
          <button
            v-for="agent in agentOptions"
            :key="agent.sessionId"
            :class="`timeline-tab${selectedAgentId === agent.sessionId ? ' active' : ''}`"
            type="button"
            @click="selectedAgentId = agent.sessionId"
          >
            {{ agentDisplayLabel(agent.sessionId, agentOptions) }}
          </button>
          <button
            :class="`timeline-tab${selectedAgentId === 'all' ? ' active' : ''}`"
            type="button"
            @click="selectedAgentId = 'all'"
          >
            全部
          </button>
        </div>
      </div>

      <MessageTimeline
        v-if="timelineTab === 'messages'"
        :active-detail="overview"
        :agent-options="agentOptions"
        :empty-text="emptyVisibleMessageText"
        :expanded-message-ids="expandedMessageIds"
        :message-error="messageError"
        :messages="messages"
        :messages-loading="messagesLoading"
        :next-message-offset="nextMessageOffset"
        :root-session-id="rootSessionId"
        :visible-message-label="visibleMessageLabel"
        :visible-messages="visibleMessages"
        @toggle-expanded="toggleMessageExpanded"
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
