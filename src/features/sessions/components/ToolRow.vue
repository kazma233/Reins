<script setup lang="ts">
import { ref } from "vue";
import type { ToolRowItem } from "../timeline-group";

type ToolRowProps = {
  item: ToolRowItem;
};

defineProps<ToolRowProps>();

const expanded = ref(false);

// tag 文案是"这次调用做了什么"（运行命令 / 已读取…），状态由 tag 颜色表达
// （绿=成功、红=失败、黄=无结果）。
function statusTagClass(status: ToolRowItem["status"]): string {
  if (status === "error") {
    return "flow-row-tag--danger";
  }
  if (status === "no-result") {
    return "flow-row-tag--warning";
  }
  return "flow-row-tag--success";
}
</script>

<template>
  <div class="flow-tool">
    <button
      :class="['flow-tool-row', { expanded }]"
      type="button"
      @click="expanded = !expanded"
    >
      <span :class="['flow-row-tag', statusTagClass(item.status)]">{{ item.label }}</span>
      <span class="flow-tool-summary">{{ item.summary }}</span>
      <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>
    <pre v-if="expanded && item.detailText" class="flow-tool-detail">{{ item.detailText }}</pre>
  </div>
</template>
