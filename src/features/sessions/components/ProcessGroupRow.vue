<script setup lang="ts">
import { computed, ref } from "vue";
import { processGroupSummary, type ProcessGroupItem } from "../timeline-group";
import CollapsedBlockRow from "./CollapsedBlockRow.vue";
import MessageErrorRow from "./MessageErrorRow.vue";
import ToolRow from "./ToolRow.vue";

// 「只看对话」下的过程组行：一行摘要 + 展开后逐条过程，明细不丢也不藏。
// 组内每条过程仍走各自的收起逻辑，可以单独再点开。
type ProcessGroupRowProps = {
  item: ProcessGroupItem;
  conversationOnly: boolean;
};

const props = defineProps<ProcessGroupRowProps>();

const expanded = ref(false);
const summary = computed(() => processGroupSummary(props.item.items));
</script>

<template>
  <div class="flow-process-group">
    <button
      :aria-expanded="expanded"
      :class="['flow-tool-row process-group', { expanded }]"
      type="button"
      @click="expanded = !expanded"
    >
      <span class="flow-row-tag">过程</span>
      <span class="flow-tool-summary">{{ summary }}</span>
      <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>
    <div v-if="expanded" class="flow-process-group__body">
      <template v-for="child in item.items" :key="child.key">
        <ToolRow
          v-if="child.kind === 'tool'"
          :conversation-only="conversationOnly"
          :item="child"
        />
        <CollapsedBlockRow
          v-else-if="child.kind === 'collapsed-block'"
          :conversation-only="conversationOnly"
          :item="child"
        />
        <MessageErrorRow
          v-else-if="child.kind === 'message-error'"
          :conversation-only="conversationOnly"
          :item="child"
        />
      </template>
    </div>
  </div>
</template>
