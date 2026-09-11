<script setup lang="ts">
import { computed, ref } from "vue";
import type { CollapsedBlockItem } from "../timeline-group";
import MessageBlockContent from "./MessageBlockContent.vue";

type CollapsedBlockRowProps = {
  item: CollapsedBlockItem;
};

const props = defineProps<CollapsedBlockRowProps>();

const expanded = ref(false);
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
      @click="expanded = !expanded"
    >
      <span class="flow-thinking-label">{{ item.label }}</span>
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
