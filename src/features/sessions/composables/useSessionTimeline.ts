import { computed, ref, watch, type ComputedRef, type Ref } from "vue";
import { getSessionEvents, getSessionMessages } from "../api";
import { extractErrorMessage } from "@shared/lib/errors";
import { createKeyGuard } from "@shared/lib/request-guard";
import { sessionIdentityKey } from "../session-identity";
import type {
  SessionEvent,
  SessionMessage,
  SessionOverview
} from "../types";

// 详情正文全量加载:时间线分组器的调用-结果配对、子代理入口收拢都依赖
// 完整消息流,分页会把跨页配对变成脏数据,这里一次取回不再有加载更多。
export type TimelineLoader<T> = {
  items: Ref<T[]>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
  load: () => Promise<void>;
  reset: () => void;
};

function createTimelineLoader<T>(
  requestKey: () => string | null,
  fetchAll: (requestId: string) => Promise<T[]>,
  fallbackError: string
): TimelineLoader<T> {
  const items = ref<T[]>([]) as Ref<T[]>;
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load(): Promise<void> {
    const requestId = requestKey();
    if (requestId === null || loading.value) {
      return;
    }
    loading.value = true;
    error.value = null;
    try {
      const loaded = await fetchAll(requestId);
      // 加载期间身份切换即丢弃:旧会话的数据不能落进新会话的落点。
      if (requestKey() === requestId) {
        items.value = loaded;
      }
    } catch (loadError) {
      if (requestKey() === requestId) {
        error.value = extractErrorMessage(loadError, fallbackError);
      }
    } finally {
      if (requestKey() === requestId) {
        loading.value = false;
      }
    }
  }

  function reset(): void {
    items.value = [];
    loading.value = false;
    error.value = null;
  }

  return { items, loading, error, load, reset };
}

export type SessionTimelineState = {
  detailKey: Ref<string | null>;
  messagesLoader: TimelineLoader<SessionMessage>;
  eventsLoader: TimelineLoader<SessionEvent>;
};

export function useSessionTimeline(
  overview: Ref<SessionOverview | null>
): SessionTimelineState {
  const detailKey = computed<string | null>(() => {
    const current = overview.value;
    if (!current) {
      return null;
    }
    return sessionIdentityKey(
      current.summary.sourceApp,
      current.summary.sourceSessionId
    );
  });

  const detail = () => {
    const current = overview.value;
    return current && detailKey.value !== null
      ? { sourceApp: current.summary.sourceApp, sessionId: current.summary.sourceSessionId }
      : null;
  };

  const messagesLoader = createTimelineLoader<SessionMessage>(
    () => detailKey.value,
    async () => {
      const current = detail();
      if (!current) {
        throw new Error("时间线加载缺少会话详情。");
      }
      return getSessionMessages(current.sourceApp, current.sessionId);
    },
    "加载会话时间线失败。"
  );

  const eventsLoader = createTimelineLoader<SessionEvent>(
    () => detailKey.value,
    async () => {
      const current = detail();
      if (!current) {
        throw new Error("记录加载缺少会话详情。");
      }
      return getSessionEvents(current.sourceApp, current.sessionId);
    },
    "加载会话记录失败。"
  );

  // 切换会话:两个 loader 全部重置并立即全量加载。
  watch(
    overview,
    () => {
      messagesLoader.reset();
      eventsLoader.reset();
      if (detailKey.value === null) {
        return;
      }
      void messagesLoader.load();
      void eventsLoader.load();
    },
    { immediate: true }
  );

  return { detailKey, messagesLoader, eventsLoader };
}
