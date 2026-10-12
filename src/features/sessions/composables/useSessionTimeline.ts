import { computed, ref, watch, type ComputedRef, type Ref } from "vue";
import { getSessionEvents, getSessionMessages } from "../api";
import { extractErrorMessage } from "@shared/lib/errors";
import { createKeyGuard } from "@shared/lib/request-guard";
import { createPagedList, type PagedList } from "./createPagedList";
import { sessionIdentityKey } from "../stores/session";
import type {
  SessionEvent,
  SessionMessage,
  SessionOverview
} from "../types";

// 详情时间线单页条数:滚动到底自动续拉,80 条在首屏成本与滚动频率间取衡。
export const DETAIL_PAGE_SIZE = 80;

// messages/events 的加载流程完全同构,收敛成一个 loader,避免双份状态机漂移。
// 状态机本体在 createPagedList,这里只补时间线的数据落点(items ref)与
// 单一错误位。
export type TimelineLoader<T> = {
  items: Ref<T[]>;
  loading: Ref<boolean>;
  loadingMore: Ref<boolean>;
  error: Ref<string | null>;
  nextOffset: Ref<number | null>;
  canLoadMore: ComputedRef<boolean>;
  loadMore: () => Promise<void>;
  prime: () => void;
  reset: () => void;
};

function createTimelineLoader<T>(
  overviewRef: Ref<SessionOverview | null>,
  requestKey: () => string | null,
  fetchPage: (overview: SessionOverview, offset: number) => Promise<{ items: T[]; nextOffset: number | null }>,
  fallbackError: string
): TimelineLoader<T> {
  const items = ref<T[]>([]) as Ref<T[]>;
  const error = ref<string | null>(null);

  const list: PagedList = createPagedList<T>({
    requestKey,
    fetchPage: async (offset) => {
      const overview = overviewRef.value;
      // requestKey 非 null 即意味着 overview 存在;二者在同一同步段读取。
      if (!overview) {
        throw new Error("时间线加载缺少会话详情。");
      }
      const loaded = await fetchPage(overview, offset);
      // 时间线的「还有更多」就是 nextOffset 未尽。
      return {
        items: loaded.items,
        nextOffset: loaded.nextOffset,
        hasMore: loaded.nextOffset !== null
      };
    },
    apply: (result, mode) => {
      items.value =
        mode === "replace" ? result.items : [...items.value, ...result.items];
    },
    onError: (loadError) => {
      error.value = extractErrorMessage(loadError, fallbackError);
    },
    isEmpty: () => items.value.length === 0
  });

  async function loadMore(): Promise<void> {
    // 与收编前一致:真正开始加载时才清错误,gate 未放行不动它。
    if (list.canLoadMore.value) {
      error.value = null;
    }
    await list.loadMore();
  }

  function reset(): void {
    list.reset();
    items.value = [];
    error.value = null;
  }

  return {
    items,
    loading: list.loading,
    loadingMore: list.loadingMore,
    error,
    nextOffset: list.nextOffset,
    canLoadMore: list.canLoadMore,
    loadMore,
    prime: list.prime,
    reset
  };
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

  // 切换会话后，未完成的消息/记录请求都要作废
  const requestGuard = createKeyGuard(() => detailKey.value);

  const messagesLoader = createTimelineLoader<SessionMessage>(
    overview,
    () => detailKey.value,
    async (currentOverview, offset) => {
      const bundle = await getSessionMessages(
        currentOverview.summary.sourceApp,
        currentOverview.summary.sourceSessionId,
        {
          offset,
          limit: DETAIL_PAGE_SIZE
        }
      );
      return { items: bundle.messages, nextOffset: bundle.nextOffset };
    },
    "加载会话时间线失败。"
  );

  const eventsLoader = createTimelineLoader<SessionEvent>(
    overview,
    () => detailKey.value,
    async (currentOverview, offset) => {
      const page = await getSessionEvents(
        currentOverview.summary.sourceApp,
        currentOverview.summary.sourceSessionId,
        {
          offset,
          limit: DETAIL_PAGE_SIZE
        }
      );
      return { items: page.events, nextOffset: page.nextOffset };
    },
    "加载更多记录失败。"
  );

  // overview 变化:两个 loader 全部重置,消息首页立即加载;
  // raw 页懒加载,首页消息结束(成功或失败)后才置就绪允许拉取。
  watch(
    overview,
    (currentOverview) => {
      messagesLoader.reset();
      eventsLoader.reset();
      if (!currentOverview || !detailKey.value) {
        return;
      }

      const activeDetailKey = detailKey.value;
      messagesLoader.prime();
      void messagesLoader.loadMore().then(() => {
        if (requestGuard.isCurrent(activeDetailKey)) {
          eventsLoader.prime();
        }
      });
    },
    { immediate: true }
  );

  return { detailKey, messagesLoader, eventsLoader };
}
