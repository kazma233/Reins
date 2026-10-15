<script setup lang="ts">
import type { ToolRowItem } from "../timeline-group";
import { useConversationCollapse } from "../composables/useConversationCollapse";

type ToolRowProps = {
  item: ToolRowItem;
  // 只看对话开启时收起成一行；开启后仍可单独点开这条过程
  conversationOnly: boolean;
};

const props = defineProps<ToolRowProps>();

const { expanded, toggle } = useConversationCollapse(() => props.conversationOnly);

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
      @click="toggle"
    >
      <span :class="['flow-row-tag', statusTagClass(item.status)]">{{ item.label }}</span>
      <span class="flow-tool-summary">{{ item.summary }}</span>
      <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>
    <pre v-if="expanded && item.detailText" class="flow-tool-detail">{{ item.detailText }}</pre>
  </div>
</template>
