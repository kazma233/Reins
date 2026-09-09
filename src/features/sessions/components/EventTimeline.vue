<script setup lang="ts">
import { joinClasses } from "@shared/lib/join-classes";
import { formatTimestamp } from "@shared/lib/format";
import {
  agentLabel,
  shouldShowEventAgentChip,
  type PreparedTimelineEvent
} from "../composables/session-detail-helpers";
import type { SessionAgent, SessionEvent } from "../types";

type EventTimelineProps = {
  emptyText: string;
  eventError: string | null;
  events: SessionEvent[];
  eventsLoading: boolean;
  filteredEvents: PreparedTimelineEvent[];
  nextEventOffset: number | null;
  visibleEventLabel: string;
  agentOptions: SessionAgent[];
  rootSessionId: string;
};

defineProps<EventTimelineProps>();
</script>

<template>
  <p v-if="eventError" class="error-text">{{ eventError }}</p>
  <div v-if="eventsLoading && events.length === 0" class="timeline">
    <article
      v-for="(_, index) in 3"
      :key="index"
      class="timeline-card compact skeleton-card"
    >
      <span class="skeleton-line skeleton-title" />
      <span class="skeleton-line" />
    </article>
  </div>
  <p v-else-if="filteredEvents.length === 0" class="muted-text">
    {{ emptyText }}
  </p>
  <div v-else class="timeline">
    <article
      v-for="event in filteredEvents"
      :key="event.key"
      :class="
        joinClasses(
          'timeline-card',
          'compact',
          'event-card',
          event.isSubagentLifecycle && 'subagent-event-card'
        )
      "
    >
      <header class="event-card-header">
        <span class="event-kind">{{ event.event.kind }}</span>
        <small>{{ formatTimestamp(event.event.timestamp) }}</small>
        <div
          v-if="
            shouldShowEventAgentChip(event.agentSessionId, rootSessionId)
          "
          class="message-source-chip"
        >
          <span class="message-source-chip-label">Agent</span>
          <span class="message-source-chip-value">
            {{ event.agentSessionId ? agentLabel(event.agentSessionId, agentOptions) : "" }}
          </span>
        </div>
      </header>
      <p class="event-summary">{{ event.event.summary }}</p>
      <pre v-if="event.payloadText" class="event-payload">{{ event.payloadText }}</pre>
    </article>
  </div>
  <div
    v-if="filteredEvents.length > 0 || nextEventOffset !== null"
    class="detail-pagination"
  >
    <span class="muted-text">{{ visibleEventLabel }}</span>
  </div>
</template>
