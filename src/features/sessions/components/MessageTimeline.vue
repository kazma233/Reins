<script setup lang="ts">
import type {
  SessionAgent,
  SessionMessage,
  SessionOverview
} from "../types";
import type { VisibleTimelineMessage } from "../composables/session-detail-helpers";
import MessageCard from "./MessageCard.vue";

type MessageTimelineProps = {
  activeDetail: SessionOverview;
  agentOptions: SessionAgent[];
  emptyText: string;
  expandedMessageIds: Record<string, boolean>;
  messageError: string | null;
  messages: SessionMessage[];
  messagesLoading: boolean;
  nextMessageOffset: number | null;
  rootSessionId: string;
  visibleMessageLabel: string;
  visibleMessages: VisibleTimelineMessage[];
};

defineProps<MessageTimelineProps>();

defineEmits<{
  toggleExpanded: [messageKey: string];
}>();
</script>

<template>
  <p v-if="messageError" class="error-text">{{ messageError }}</p>
  <div v-if="messagesLoading && messages.length === 0" class="timeline">
    <article
      v-for="(_, index) in 3"
      :key="index"
      class="timeline-card skeleton-card"
    >
      <span class="skeleton-line skeleton-title" />
      <span class="skeleton-line" />
      <span class="skeleton-line skeleton-short" />
    </article>
  </div>
  <p v-else-if="visibleMessages.length === 0" class="muted-text">{{ emptyText }}</p>
  <div v-else class="timeline">
    <MessageCard
      v-for="message in visibleMessages"
      :key="message.key"
      :agent-options="agentOptions"
      :is-expanded="Boolean(expandedMessageIds[message.key])"
      :message="message"
      :root-session-id="rootSessionId"
      @toggle-expanded="$emit('toggleExpanded', $event)"
    />
  </div>
  <div
    v-if="
      activeDetail.messageCount !== null ||
      visibleMessages.length > 0 ||
      nextMessageOffset !== null
    "
    class="detail-pagination"
  >
    <span class="muted-text">{{ visibleMessageLabel }}</span>
  </div>
</template>
