<script setup lang="ts">
import { computed } from "vue";
import { joinClasses } from "@shared/lib/join-classes";
import { formatTimestamp } from "@shared/lib/format";
import {
  agentDisplayLabel,
  extractSubagentLabel,
  messageCardClassName,
  type VisibleTimelineMessage
} from "../composables/session-detail-helpers";
import type { SessionAgent } from "../types";
import MessageBlockContent from "./MessageBlockContent.vue";

type MessageCardProps = {
  agentOptions: SessionAgent[];
  isExpanded: boolean;
  message: VisibleTimelineMessage;
  rootSessionId: string;
};

const props = defineProps<MessageCardProps>();

const emit = defineEmits<{
  toggleExpanded: [messageKey: string];
}>();

const messageAgentId = computed(
  () => props.message.message.sessionId ?? props.rootSessionId
);
const isInSubagent = computed(() => messageAgentId.value !== props.rootSessionId);
const agentName = computed(() =>
  agentDisplayLabel(messageAgentId.value, props.agentOptions)
);
const cardClass = computed(() =>
  messageCardClassName(props.message.isSubagentMarker, isInSubagent.value)
);
const subagentLabel = computed(() =>
  props.message.isSubagentMarker
    ? extractSubagentLabel(props.message.message) ?? "Sub-agent"
    : null
);
</script>

<template>
  <article :class="cardClass">
    <div v-if="message.isSubagentMarker" class="subagent-marker-header">
      <div class="subagent-marker-title-row">
        <span class="subagent-badge strong">Sub-agent</span>
        <strong class="subagent-marker-title">{{ subagentLabel }}</strong>
      </div>
    </div>
    <header v-else class="message-card-header">
      <div class="message-card-meta">
        <strong :class="message.message.role === 'user' ? 'message-role-user' : undefined">
          {{ message.message.role === "user" ? "用户" : "助手" }}
        </strong>
        <small>{{ formatTimestamp(message.message.timestamp) }}</small>
        <span v-if="isInSubagent" class="message-source-chip">
          <span class="message-source-chip-label">Agent</span>
          <span class="message-source-chip-value">{{ agentName }}</span>
        </span>
        <div class="message-header-tags">
          <span
            v-for="tag in message.tags"
            :key="tag.key"
            :class="tag.kind === 'tool' ? 'block-tag tool-tag' : 'block-kind'"
          >
            {{ tag.label }}
          </span>
        </div>
      </div>
      <button
        v-if="message.needsExpand"
        class="text-button"
        type="button"
        @click="emit('toggleExpanded', message.key)"
      >
        {{ isExpanded ? "收起" : "展开" }}
      </button>
    </header>
    <div
      v-for="(block, index) in message.blocksToRender"
      :key="`${message.key}-${index}`"
      class="block-card"
    >
      <MessageBlockContent
        v-if="block.contentText"
        :content-text="block.contentText"
        :expanded="isExpanded"
        :kind="block.block.kind"
        :needs-expand="block.needsExpand"
        @expand="emit('toggleExpanded', message.key)"
      />
    </div>
  </article>
</template>
