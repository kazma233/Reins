<script setup lang="ts">
import type { SessionOverview } from "../types";
import type { TimelineItem } from "../timeline-group";
import MessageBlockContent from "./MessageBlockContent.vue";
import CollapsedBlockRow from "./CollapsedBlockRow.vue";
import ToolRow from "./ToolRow.vue";
import SubagentEntryRow from "./SubagentEntryRow.vue";
import SubagentGroupRow from "./SubagentGroupRow.vue";

// 文档流时间线：助手/工具内容靠左，用户输入右侧气泡；
// 工具与思考默认折叠成一行，子代理只暴露入口。
type MessageTimelineProps = {
  activeDetail: SessionOverview;
  emptyText: string;
  items: TimelineItem[];
  messageError: string | null;
  messagesLoading: boolean;
  visibleMessageLabel: string;
};

defineProps<MessageTimelineProps>();

defineEmits<{ openSubagent: [sessionId: string, label: string] }>();
</script>

<template>
  <p v-if="messageError" class="error-text">{{ messageError }}</p>
  <div v-if="messagesLoading && items.length === 0" class="flow">
    <article
      v-for="(_, index) in 3"
      :key="index"
      class="skeleton-card"
    >
      <span class="skeleton-line skeleton-title" />
      <span class="skeleton-line" />
      <span class="skeleton-line skeleton-short" />
    </article>
  </div>
  <p v-else-if="items.length === 0" class="muted-text">{{ emptyText }}</p>
  <div v-else class="flow">
    <template v-for="item in items" :key="item.key">
      <template v-if="item.kind === 'text'">
        <div v-if="item.role === 'user'" class="flow-user">
          <MessageBlockContent
            v-for="(block, index) in item.blocks"
            :key="`${item.key}-${index}`"
            :content-text="block.text ?? ''"
            :kind="block.kind"
          />
        </div>
        <div v-else class="flow-text">
          <MessageBlockContent
            v-for="(block, index) in item.blocks"
            :key="`${item.key}-${index}`"
            :content-text="block.text ?? ''"
            :kind="block.kind"
          />
        </div>
      </template>
      <ToolRow v-else-if="item.kind === 'tool'" :item="item" />
      <CollapsedBlockRow v-else-if="item.kind === 'collapsed-block'" :item="item" />
      <SubagentEntryRow v-else-if="item.kind === 'subagent'" :item="item" />
      <SubagentGroupRow
        v-else-if="item.kind === 'subagent-group'"
        :item="item"
        @open="
          (sessionId: string, label: string) => $emit('openSubagent', sessionId, label)
        "
      />
    </template>
  </div>
  <div class="detail-pagination">
    <span class="muted-text">{{ visibleMessageLabel }}</span>
  </div>
</template>
