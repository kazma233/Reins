<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { joinClasses } from "@shared/lib/join-classes";
import { formatTimestamp } from "@shared/lib/format";
import type { SessionSummary } from "../types";
import "./session-list.css";

type SessionListProps = {
  sourceKey: string;
  sessions: SessionSummary[];
  totalCount: number;
  selectedSessionKey: string | null;
  loading: boolean;
  loadingMore: boolean;
  hasMore: boolean;
  query: string;
  queryKey: string;
  resultKey: string;
  reverse: boolean;
};

const props = defineProps<SessionListProps>();

const emit = defineEmits<{
  loadMore: [];
  queryChange: [query: string];
  reverseChange: [reverse: boolean];
  select: [sessionKey: string];
}>();

const listRef = ref<HTMLDivElement | null>(null);

const hasSessions = computed(() => props.sessions.length > 0);
const showSkeletonRows = computed(() => props.loading && !hasSessions.value);
const showEmptyState = computed(() => !props.loading && !hasSessions.value);
const loadingLabel = computed(() =>
  props.query.trim().length > 0 ? "正在筛选会话..." : "正在加载会话..."
);

// Reset scroll position whenever a new result set arrives.
watch(
  () => props.resultKey,
  () => {
    nextTick(() => {
      if (listRef.value) {
        listRef.value.scrollTop = 0;
      }
    });
  }
);

// Auto-load more when the list is shorter than the viewport.
watch(
  () => [
    props.hasMore,
    props.loading,
    props.loadingMore,
    props.queryKey,
    props.reverse,
    props.sessions.length,
    props.sourceKey
  ],
  () => {
    const listElement = listRef.value;
    if (!listElement || props.loading || props.loadingMore || !props.hasMore) {
      return;
    }
    if (listElement.scrollHeight <= listElement.clientHeight + 32) {
      emit("loadMore");
    }
  }
);

onMounted(() => {
  // Trigger the initial auto-load check after mount.
  const listElement = listRef.value;
  if (!listElement || props.loading || props.loadingMore || !props.hasMore) {
    return;
  }
  if (listElement.scrollHeight <= listElement.clientHeight + 32) {
    emit("loadMore");
  }
});

function handleScroll(event: Event) {
  if (props.loading || props.loadingMore || !props.hasMore) {
    return;
  }
  const listElement = event.currentTarget as HTMLDivElement;
  const remainingDistance =
    listElement.scrollHeight - listElement.scrollTop - listElement.clientHeight;
  if (remainingDistance < 240) {
    emit("loadMore");
  }
}

function handleQueryInput(event: Event) {
  emit("queryChange", (event.target as HTMLInputElement).value);
}

function handleReverseClick() {
  emit("reverseChange", !props.reverse);
}
</script>

<template>
  <div class="panel-shell session-list-shell">
    <label class="session-filter-field">
      <span class="session-filter-label">筛选会话</span>
      <div class="session-filter-controls">
        <input
          :placeholder="'按标题或 Session ID 模糊搜索'"
          type="search"
          :value="query"
          @input="handleQueryInput"
        />
        <button
          :aria-label="reverse ? '切换为当前排序' : '切换为反向排序'"
          :class="joinClasses('session-sort-button', reverse && 'active')"
          :title="
            reverse
              ? '当前为反向排序，点击恢复默认排序'
              : '当前为默认排序，点击切换为反向排序'
          "
          type="button"
          @click="handleReverseClick"
        >
          <svg
            aria-hidden="true"
            class="session-sort-icon"
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
    </label>
    <div v-if="loading" class="session-list-refresh-status">
      {{ loadingLabel }}
    </div>
    <div
      ref="listRef"
      :aria-busy="loading || loadingMore"
      class="session-list"
      @scroll="handleScroll"
    >
      <template v-if="showSkeletonRows">
        <div
          v-for="(_, index) in 6"
          :key="`skeleton-${index}`"
          class="session-card skeleton-card"
        >
          <span class="skeleton-line skeleton-title" />
          <div class="session-meta-row">
            <span class="skeleton-line skeleton-meta" />
          </div>
        </div>
      </template>
      <template v-else>
        <button
          v-for="session in sessions"
          :key="session.transcriptPath"
          :class="
            joinClasses(
              'session-card',
              session.transcriptPath === selectedSessionKey && 'active'
            )
          "
          type="button"
          @click="emit('select', session.transcriptPath)"
        >
          <span class="session-card-title">{{ session.title }}</span>
          <div class="session-meta-row">
            <small>{{ formatTimestamp(session.updatedAt) }}</small>
          </div>
        </button>
      </template>
      <div v-if="showEmptyState" class="empty-state session-list-empty">
        {{ query.trim().length > 0 ? "没有匹配的会话。" : "该来源下暂无会话。" }}
      </div>
      <div v-if="loadingMore" class="session-list-footer">
        <small>已加载 {{ sessions.length }} / {{ totalCount }}</small>
        <span class="session-list-hint">正在加载更多...</span>
      </div>
      <div v-else-if="hasMore" class="session-list-footer">
        <small>已加载 {{ sessions.length }} / {{ totalCount }}</small>
        <span class="session-list-hint">向下滚动以加载更多</span>
      </div>
      <div v-else-if="totalCount > 0" class="session-list-footer complete">
        <small>共加载 {{ sessions.length }} 条会话</small>
      </div>
    </div>
  </div>
</template>
