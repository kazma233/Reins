import { computed, ref, watch, type Ref } from "vue";
import { getSessionEvents, getSessionMessages } from "../api";
import { extractErrorMessage } from "@shared/lib/errors";
import { createKeyGuard } from "@shared/lib/request-guard";
import type {
  SessionEvent,
  SessionMessage,
  SessionOverview,
  SourceApp
} from "../types";

// 详情时间线单页条数:滚动到底自动续拉,80 条在首屏成本与滚动频率间取衡。
export const DETAIL_PAGE_SIZE = 80;

export function sessionRequestKey(
  sourceApp: SourceApp,
  sourceSessionId: string,
  transcriptPath: string
): string {
  return `${sourceApp}:${sourceSessionId}:${transcriptPath}`;
}

// messages/events 的加载流程完全同构,收敛成一个 loader,避免双份状态机漂移。
export type TimelineLoader<T> = {
  items: Ref<T[]>;
  loading: Ref<boolean>;
  loadingMore: Ref<boolean>;
  error: Ref<string | null>;
  nextOffset: Ref<number | null>;
  loadMore: () => Promise<void>;
  reset: () => void;
};

type TimelinePage<T> = { items: T[]; nextOffset: number | null };

function createTimelineLoader<T>(
  overviewRef: Ref<SessionOverview | null>,
  requestGuard: { capture: () => string | null; isCurrent: (key: string) => boolean },
  fetchPage: (
    overview: SessionOverview,
    offset: number
  ) => Promise<TimelinePage<T>>,
  fallbackError: string
): TimelineLoader<T> {
  const items = ref<T[]>([]) as Ref<T[]>;
  const loading = ref(false);
  const loadingMore = ref(false);
  const error = ref<string | null>(null);
  const nextOffset = ref<number | null>(null);

  async function loadMore(): Promise<void> {
    const currentOverview = overviewRef.value;
    if (
      !currentOverview ||
      loading.value ||
      loadingMore.value ||
      nextOffset.value === null
    ) {
      return;
    }

    const requestKey = requestGuard.capture();
    if (requestKey === null) {
      return;
    }

    // offset 0 且列表为空即首屏加载:整页替换而非追加
    const initialLoad = nextOffset.value === 0 && items.value.length === 0;
    if (initialLoad) {
      loading.value = true;
    } else {
      loadingMore.value = true;
    }
    error.value = null;

    try {
      const page = await fetchPage(currentOverview, nextOffset.value);
      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }
      items.value = initialLoad ? page.items : [...items.value, ...page.items];
      nextOffset.value = page.nextOffset;
    } catch (loadError) {
      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }
      error.value = extractErrorMessage(loadError, fallbackError);
    } finally {
      if (requestGuard.isCurrent(requestKey)) {
        loading.value = false;
        loadingMore.value = false;
      }
    }
  }

  function reset() {
    items.value = [];
    loading.value = false;
    loadingMore.value = false;
    error.value = null;
    nextOffset.value = null;
  }

  return { items, loading, loadingMore, error, nextOffset, loadMore, reset };
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
    return sessionRequestKey(
      current.summary.sourceApp,
      current.summary.sourceSessionId,
      current.summary.transcriptPath
    );
  });

  // 切换会话后，未完成的消息/事件请求都要作废
  const requestGuard = createKeyGuard(() => detailKey.value);

  const messagesLoader = createTimelineLoader<SessionMessage>(
    overview,
    requestGuard,
    async (currentOverview, offset) => {
      const bundle = await getSessionMessages(
        currentOverview.summary.sourceApp,
        currentOverview.summary.sourceSessionId,
        {
          transcriptPath: currentOverview.summary.transcriptPath,
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
    requestGuard,
    async (currentOverview, offset) => {
      const page = await getSessionEvents(
        currentOverview.summary.sourceApp,
        currentOverview.summary.sourceSessionId,
        {
          transcriptPath: currentOverview.summary.transcriptPath,
          offset,
          limit: DETAIL_PAGE_SIZE
        }
      );
      return { items: page.events, nextOffset: page.nextOffset };
    },
    "加载更多事件失败。"
  );

  // overview 变化:两个 loader 全部重置,消息首页立即加载;
  // 事件页懒加载,首页消息结束(成功或失败)后才置 0 允许拉取。
  watch(
    overview,
    currentOverview => {
      messagesLoader.reset();
      eventsLoader.reset();
      if (!currentOverview || !detailKey.value) {
        return;
      }

      const activeDetailKey = detailKey.value;
      messagesLoader.nextOffset.value = 0;
      void messagesLoader.loadMore().then(() => {
        if (requestGuard.isCurrent(activeDetailKey)) {
          eventsLoader.nextOffset.value = 0;
        }
      });
    },
    { immediate: true }
  );

  return { detailKey, messagesLoader, eventsLoader };
}
