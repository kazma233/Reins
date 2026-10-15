<script setup lang="ts">
import type { MessageErrorItem } from "../timeline-group";
import { useConversationCollapse } from "../composables/useConversationCollapse";

// 消息没有正文只有错误：错误文案常显，原始报文按需展开（排查时仍需看
// provider / model / stopReason 等字段）。
type MessageErrorRowProps = {
  item: MessageErrorItem;
  // 只看对话开启时收起成一行；开启后仍可单独点开这条错误报文
  conversationOnly: boolean;
};

const props = defineProps<MessageErrorRowProps>();

const { expanded, toggle } = useConversationCollapse(() => props.conversationOnly);
</script>

<template>
  <div class="flow-message-error">
    <button
      :aria-expanded="expanded"
      class="flow-message-error__head"
      type="button"
      @click="toggle"
    >
      <span class="flow-row-tag flow-row-tag--danger">错误</span>
      <span class="flow-message-error__text">{{ item.text }}</span>
      <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>
    <pre v-if="expanded" class="flow-tool-detail">{{ item.detailText }}</pre>
  </div>
</template>
