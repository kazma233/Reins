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

export const DETAIL_PAGE_SIZE = 40;

export function sessionRequestKey(
  sourceApp: SourceApp,
  sourceSessionId: string,
  transcriptPath: string
): string {
  return `${sourceApp}:${sourceSessionId}:${transcriptPath}`;
}

export type SessionTimelineState = {
  detailKey: Ref<string | null>;
  events: Ref<SessionEvent[]>;
  eventsLoading: Ref<boolean>;
  eventsLoadingMore: Ref<boolean>;
  eventError: Ref<string | null>;
  loadMoreEvents: () => Promise<void>;
  loadMoreMessages: () => Promise<void>;
  messageError: Ref<string | null>;
  messages: Ref<SessionMessage[]>;
  messagesLoading: Ref<boolean>;
  messagesLoadingMore: Ref<boolean>;
  nextEventOffset: Ref<number | null>;
  nextMessageOffset: Ref<number | null>;
};

export function useSessionTimeline(
  overview: Ref<SessionOverview | null>
): SessionTimelineState {
  const messages = ref<SessionMessage[]>([]);
  const events = ref<SessionEvent[]>([]);
  const messagesLoading = ref(false);
  const eventsLoading = ref(false);
  const messagesLoadingMore = ref(false);
  const eventsLoadingMore = ref(false);
  const messageError = ref<string | null>(null);
  const eventError = ref<string | null>(null);
  const nextMessageOffset = ref<number | null>(null);
  const nextEventOffset = ref<number | null>(null);

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

  // Race guard for async loaders: a response is only applied while the
  // detail key is unchanged since the request was issued.
  const requestGuard = createKeyGuard(() => detailKey.value);

  async function loadMoreMessages() {
    const currentOverview = overview.value;
    if (
      !currentOverview ||
      messagesLoading.value ||
      messagesLoadingMore.value ||
      nextMessageOffset.value === null
    ) {
      return;
    }

    const requestKey = requestGuard.capture();
    if (!requestKey) {
      return;
    }

    messagesLoadingMore.value = true;
    messageError.value = null;

    try {
      const bundle = await getSessionMessages(
        currentOverview.summary.sourceApp,
        currentOverview.summary.sourceSessionId,
        {
          transcriptPath: currentOverview.summary.transcriptPath,
          offset: nextMessageOffset.value,
          limit: DETAIL_PAGE_SIZE
        }
      );

      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }

      messages.value = [...messages.value, ...bundle.messages];
      nextMessageOffset.value = bundle.nextOffset;
    } catch (error) {
      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }
      messageError.value = extractErrorMessage(error, "加载更多消息失败。");
    } finally {
      if (requestGuard.isCurrent(requestKey)) {
        messagesLoadingMore.value = false;
      }
    }
  }

  async function loadMoreEvents() {
    const currentOverview = overview.value;
    if (
      !currentOverview ||
      eventsLoading.value ||
      eventsLoadingMore.value ||
      nextEventOffset.value === null
    ) {
      return;
    }

    const requestKey = requestGuard.capture();
    if (!requestKey) {
      return;
    }

    const initialLoad = events.value.length === 0 && nextEventOffset.value === 0;

    if (initialLoad) {
      eventsLoading.value = true;
    } else {
      eventsLoadingMore.value = true;
    }

    eventError.value = null;

    try {
      const page = await getSessionEvents(
        currentOverview.summary.sourceApp,
        currentOverview.summary.sourceSessionId,
        {
          transcriptPath: currentOverview.summary.transcriptPath,
          offset: nextEventOffset.value,
          limit: DETAIL_PAGE_SIZE
        }
      );

      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }

      events.value = [...events.value, ...page.events];
      nextEventOffset.value = page.nextOffset;
    } catch (error) {
      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }
      eventError.value = extractErrorMessage(error, "加载更多事件失败。");
    } finally {
      if (requestGuard.isCurrent(requestKey)) {
        eventsLoading.value = false;
        eventsLoadingMore.value = false;
      }
    }
  }

  // Reset and load the initial message batch whenever the overview changes.
  // Events are loaded lazily via loadMoreEvents when the events tab is opened.
  watch(
    overview,
    (currentOverview, _oldOverview, onCleanup) => {
      const activeDetailKey = detailKey.value;
      if (!currentOverview || !activeDetailKey) {
        messages.value = [];
        events.value = [];
        messageError.value = null;
        eventError.value = null;
        messagesLoading.value = false;
        eventsLoading.value = false;
        messagesLoadingMore.value = false;
        eventsLoadingMore.value = false;
        nextMessageOffset.value = null;
        nextEventOffset.value = null;
        return;
      }

      let cancelled = false;

      // Snapshot the overview so the nested async function retains the
      // narrowed (non-null) type — TS can't carry narrowing into closures.
      const activeOverview: SessionOverview = currentOverview;

      messages.value = [];
      events.value = [];
      messageError.value = null;
      eventError.value = null;
      messagesLoading.value = true;
      eventsLoading.value = false;
      messagesLoadingMore.value = false;
      eventsLoadingMore.value = false;
      nextMessageOffset.value = null;
      nextEventOffset.value = null;

      async function loadInitialMessages() {
        try {
          const bundle = await getSessionMessages(
            activeOverview.summary.sourceApp,
            activeOverview.summary.sourceSessionId,
            {
              transcriptPath: activeOverview.summary.transcriptPath,
              offset: 0,
              limit: DETAIL_PAGE_SIZE
            }
          );

          if (cancelled || !requestGuard.isCurrent(activeDetailKey)) {
            return;
          }

          messages.value = bundle.messages;
          nextMessageOffset.value = bundle.nextOffset;
          nextEventOffset.value = 0;
        } catch (error) {
          if (cancelled || !requestGuard.isCurrent(activeDetailKey)) {
            return;
          }
          const message = extractErrorMessage(error, "加载会话时间线失败。");
          messageError.value = message;
          eventError.value = message;
        } finally {
          if (!cancelled && requestGuard.isCurrent(activeDetailKey)) {
            messagesLoading.value = false;
          }
        }
      }

      void loadInitialMessages();

      onCleanup(() => {
        cancelled = true;
      });
    },
    { immediate: true }
  );

  return {
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
  };
}
