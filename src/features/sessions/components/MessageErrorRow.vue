<script setup lang="ts">
import { ref } from "vue";
import type { MessageErrorItem } from "../timeline-group";

// 消息没有正文只有错误：错误文案常显，原始报文按需展开（排查时仍需看
// provider / model / stopReason 等字段）。
type MessageErrorRowProps = {
  item: MessageErrorItem;
};

defineProps<MessageErrorRowProps>();

const expanded = ref(false);
</script>

<template>
  <div class="flow-message-error">
    <button
      :aria-expanded="expanded"
      class="flow-message-error__head"
      type="button"
      @click="expanded = !expanded"
    >
      <span class="flow-row-tag flow-row-tag--danger">错误</span>
      <span class="flow-message-error__text">{{ item.text }}</span>
      <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>
    <pre v-if="expanded" class="flow-tool-detail">{{ item.detailText }}</pre>
  </div>
</template>
