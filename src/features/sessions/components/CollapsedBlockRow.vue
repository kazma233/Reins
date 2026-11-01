<script setup lang="ts">
import { computed } from "vue";
import type { CollapsedBlockItem } from "../timeline-group";
import { useConversationCollapse } from "../composables/useConversationCollapse";
import MessageBlockContent from "./MessageBlockContent.vue";

type CollapsedBlockRowProps = {
  item: CollapsedBlockItem;
  // 只看对话开启时收起成一行；开启后仍可单独点开这条思考/杂项块
  conversationOnly: boolean;
};

const props = defineProps<CollapsedBlockRowProps>();

const { expanded, toggle } = useConversationCollapse(() => props.conversationOnly);
const preview = computed(() => {
  const firstLine = props.item.text.split("\n").find((part) => part.trim()) ?? "";
  return firstLine.trim().slice(0, 60);
});
</script>

<template>
  <div class="flow-thinking">
    <button
      :class="['flow-tool-row thinking', { expanded }]"
      type="button"
      @click="toggle"
    >
      <span class="flow-row-tag">{{ item.label }}</span>
      <span class="flow-tool-summary">{{ preview }}</span>
      <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>
    <div v-if="expanded" class="flow-thinking-body">
      <MessageBlockContent
        v-if="item.markdown"
        :content-text="item.text"
        :kind="'text'"
      />
      <pre v-else class="flow-thinking-pre">{{ item.text }}</pre>
    </div>
  </div>
</template>
