<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { joinClasses } from "@shared/lib/join-classes";
import { formatTimestamp } from "@shared/lib/format";
import { useInfiniteScroll } from "@shared/lib/infinite-scroll";
import { useLoadElapsed } from "@shared/lib/use-load-elapsed";
import { useKeepAliveScroll } from "../composables/useKeepAliveScroll";
import { sessionIdentityKey } from "../session-identity";
import { formatSessionUsage } from "../token-usage";
import { formatSourceAppName } from "../source-app";
import { formatSessionModels, sessionModelsBadge } from "../session-models";
import type { SessionSummary } from "../types";
import "./session-list.css";

type SessionListProps = {
  sourceKey: string;
  sessions: SessionSummary[];
  totalCount: number;
  selectedSessionKey: string | null;
  loading: boolean;
  loadingMore: boolean;
  loadMoreError?: string | null;
  hasMore: boolean;
  query: string;
  queryKey: string;
  // 状态机的最近成功请求 key;null 表示还没有成功加载过。
  resultKey: string | null;
  reverse: boolean;
  // 跨来源合并视图才需要标出每条会话属于哪个工具;单一来源下是冗余信息。
  showAgent?: boolean;
};

const props = withDefaults(defineProps<SessionListProps>(), {
  showAgent: false
});

// 存储状态角标:各来源共用同一套文案。归档=来源自己把会话移出了默认列表
// (Codex 的 archived_sessions/、DSH 的归档集、ZCode/OpenCode 的归档时间),
// 压缩=转录在磁盘上不是该来源的明文常态(Codex 的 .jsonl.zst 冷会话)。
const ARCHIVED_LABEL = "已归档";
const ARCHIVED_HINT = "该会话已被工具自身归档，默认不出现在它的会话列表里。";
const COMPRESSED_LABEL = "已压缩";
const COMPRESSED_HINT = "转录以压缩形式存储的冷会话，打开时会先解压。";

// 模型角标：主模型 + 其余数量，完整清单放 title；没有模型信息的会话不占位。
function modelsBadge(session: SessionSummary): string | null {
  return sessionModelsBadge(session.models);
}

function modelsHint(session: SessionSummary): string {
  return `模型：${formatSessionModels(session.models)}`;
}

const emit = defineEmits<{
  loadMore: [];
  select: [sessionKey: string];
}>();

const listRef = ref<HTMLDivElement | null>(null);

// 加载中状态条的实时耗时(首扫大库时让用户看到等了多久)。
const loadElapsed = useLoadElapsed(computed(() => props.loading));

// 模式切换(如去用量统计)会缓存这个视图,列表滚动位置跟着一起留;
// 进详情会整体销毁重建这个 v-if 子树,所以挂载时也要回放最近一次抓拍。
useKeepAliveScroll("sessions-list", () => listRef.value, { restoreOnMount: true });

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

// 滚动加载 + 视口补拉:四条件门与距离判定统一在 infinite-scroll 助手,
// 收编前这份门在 watch / mounted / 滚动回调里逐字重复三次。
useInfiniteScroll({
  getElement: () => listRef.value,
  canLoadMore: () =>
    !props.loading &&
    !props.loadingMore &&
    props.hasMore &&
    // 错误未清除前不再自动加载，否则失败会立刻触发下一轮请求。
    !props.loadMoreError,
  onLoadMore: () => emit("loadMore"),
  autoFillDeps: [
    () => props.loading,
    () => props.loadingMore,
    () => props.hasMore,
    () => props.loadMoreError,
    () => props.sessions.length,
    () => props.queryKey,
    () => props.reverse,
    () => props.sourceKey
  ]
});

</script>

<template>
  <div class="panel-shell session-list-shell">
    <div v-if="loading" class="session-list-refresh-status">
      {{ loadingLabel }}
      <span class="session-list-refresh-elapsed">已用时 {{ loadElapsed.text }}</span>
    </div>
    <div
      ref="listRef"
      :aria-busy="loading || loadingMore"
      class="session-list"
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
          :key="sessionIdentityKey(session.sourceApp, session.sourceSessionId)"
          :class="
            joinClasses(
              'session-card',
              sessionIdentityKey(session.sourceApp, session.sourceSessionId) ===
                selectedSessionKey &&
                'active'
            )
          "
          type="button"
          @click="
            emit(
              'select',
              sessionIdentityKey(session.sourceApp, session.sourceSessionId)
            )
          "
        >
          <span class="session-card-title" :title="session.title">{{
            session.title
          }}</span>
          <!-- 附加信息在标题下一行铺开:时间 / 模型 / 状态角标 / 使用量 -->
          <div class="session-meta-row">
            <small class="session-meta-time">{{
              formatTimestamp(session.updatedAt)
            }}</small>
            <small v-if="showAgent" class="session-meta-agent">{{
              formatSourceAppName(session.sourceApp)
            }}</small>
            <span
              v-if="modelsBadge(session)"
              :title="modelsHint(session)"
              class="session-meta-model"
              >{{ modelsBadge(session) }}</span
            >
            <span
              v-if="session.archived"
              :title="ARCHIVED_HINT"
              class="session-state-chip session-state-chip--archived"
              >{{ ARCHIVED_LABEL }}</span
            >
            <span
              v-if="session.compressed"
              :title="COMPRESSED_HINT"
              class="session-state-chip"
              >{{ COMPRESSED_LABEL }}</span
            >
            <small v-if="session.tokenUsage" class="session-token-usage">
              <span>{{ formatSessionUsage(session.tokenUsage) }}</span>
            </small>
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
      <div
        v-else-if="loadMoreError"
        class="session-list-footer session-list-error"
        role="alert"
      >
        <span>{{ loadMoreError }}</span>
        <button
          class="session-list-error-retry"
          type="button"
          @click="emit('loadMore')"
        >
          重试
        </button>
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
