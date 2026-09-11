<script setup lang="ts">
import { ref } from "vue";
import type { ToolRowItem } from "../timeline-group";

type ToolRowProps = {
  item: ToolRowItem;
};

defineProps<ToolRowProps>();

const expanded = ref(false);

function statusText(status: ToolRowItem["status"]): string | null {
  if (status === "error") {
    return "失败";
  }
  if (status === "no-result") {
    return "无结果";
  }
  return null;
}
</script>

<template>
  <div class="flow-tool">
    <button
      :class="['flow-tool-row', { expanded }]"
      type="button"
      @click="expanded = !expanded"
    >
      <span :class="['flow-tool-dot', item.status]" />
      <span class="flow-tool-summary">{{ item.summary }}</span>
      <span v-if="statusText(item.status)" :class="['flow-tool-status', item.status]">
        {{ statusText(item.status) }}
      </span>
      <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>
    <pre v-if="expanded && item.detailText" class="flow-tool-detail">{{ item.detailText }}</pre>
  </div>
</template>
