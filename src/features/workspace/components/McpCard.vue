<script setup lang="ts">
import { computed } from "vue";
import { formatTimestamp } from "@shared/lib/format";
import AppCard from "@shared/ui/AppCard.vue";
import AppResultBadge from "@shared/ui/AppResultBadge.vue";
import McpTargetButton from "./McpTargetButton.vue";
import ProjectMcpTargetButton from "./ProjectMcpTargetButton.vue";
import type {
  AgentTargetId,
  McpConfigView,
  McpInspection,
  McpTargetInspection,
} from "../types";

type ProjectTargetEntry = {
  id: string;
  agents: { id: AgentTargetId }[];
};

type McpCardProps = {
  mcp: McpConfigView;
  inspection: McpInspection | null;
  projects: ProjectTargetEntry[];
  targetIds: AgentTargetId[];
  loading?: boolean;
  // 项目 agent 选择器的部分失败：弹窗已关，失败目标只能留在卡片上。
  warning?: string | null;
  // 卡片级「同步到目标」的结果，同样在触发它的卡片上展示。
  syncResult?: { text: string; failed: boolean } | null;
};

const props = withDefaults(defineProps<McpCardProps>(), {
  loading: false,
  warning: null,
  syncResult: null,
});

defineEmits<{
  editMcp: [mcp: McpConfigView];
  requestDeleteMcp: [serverNames: string[]];
  toggleMcpTarget: [serverName: string, targetId: AgentTargetId];
  openProjectAgentPicker: [serverName: string, projectId: string];
  syncMcp: [mcp: McpConfigView];
}>();

function formatMcpSummary(mcp: McpConfigView): string {
  if (mcp.transport === "stdio") {
    const commandLine = [mcp.command, ...mcp.args].filter(Boolean).join(" ").trim();
    return commandLine || "stdio";
  }
  return mcp.url || (mcp.transport === "sse" ? "sse" : "http");
}

function buildMcpTargetItemById(
  targets: McpTargetInspection[],
): Map<AgentTargetId, McpTargetInspection> {
  return new Map(targets.map((target) => [target.targetId, target]));
}

const createdAtLabel = computed(() => formatTimestamp(props.mcp.createdAt));
const summary = computed(() => formatMcpSummary(props.mcp));
const targetItemById = computed(() =>
  buildMcpTargetItemById(props.inspection?.targets ?? []),
);
// 操作区只放一条结果：优先最近一次「同步到目标」，其次是项目同步的部分失败。
const hint = computed<{ text: string; failed: boolean } | null>(() => {
  if (props.syncResult) return props.syncResult;
  return props.warning ? { text: props.warning, failed: true } : null;
});
// 同步只对已安装的 mcp 有意义：没有安装目标时按钮禁用。
const installedTargetCount = computed(
  () => (props.inspection?.targets ?? []).filter((target) => target.state === "present").length,
);
</script>

<template>
  <AppCard>
    <template #header>
      <span class="manager-skill-name">{{ mcp.name }}</span>
      <span v-if="mcp.createdAt" class="manager-skill-updated">
        新增于 {{ createdAtLabel }}
      </span>
    </template>
    <template #headerMeta>
      <span v-if="mcp.homepage" class="manager-header-path"><strong>主页</strong>{{ mcp.homepage }}</span>
    </template>

    <p class="manager-skill-description">{{ summary }}</p>
    <!-- 全局 target 与项目分组各占一行：项目按钮显示的是项目名，与全局的
         agent 名混排时难以区分 -->
    <div v-if="targetIds.length > 0" class="manager-target-buttons-group">
      <span class="manager-target-buttons-group__label">全局</span>
      <div class="manager-target-buttons">
        <McpTargetButton
          v-for="targetId in targetIds"
          :key="targetId"
          :server-name="mcp.name"
          :target-id="targetId"
          :target-item="targetItemById.get(targetId) ?? null"
          :transport="mcp.transport"
          :loading="loading"
          @toggle="(serverName: string, tid: AgentTargetId) => $emit('toggleMcpTarget', serverName, tid)"
        />
      </div>
    </div>
    <div v-if="projects.length > 0" class="manager-target-buttons-group">
      <span class="manager-target-buttons-group__label">项目</span>
      <div class="manager-target-buttons">
        <ProjectMcpTargetButton
          v-for="entry in projects"
          :key="entry.id"
          :server-name="mcp.name"
          :project-entry="entry"
          :inspection="inspection"
          :loading="loading"
          @click="(serverName: string, projectId: string) => $emit('openProjectAgentPicker', serverName, projectId)"
        />
      </div>
    </div>

    <template #actions>
      <!-- 结果只占一个图标：完整文案在 hover 气泡里，不参与操作行宽度 -->
      <AppResultBadge
        v-if="hint"
        :message="hint.text"
        :tone="hint.failed ? 'danger' : 'success'"
      />
      <button
        class="secondary-button"
        :disabled="loading || installedTargetCount === 0"
        :title="
          installedTargetCount > 0
            ? `把当前配置重新写入已安装的 ${installedTargetCount} 个目标`
            : '该 mcp 还没有安装到任何目标'
        "
        type="button"
        @click="$emit('syncMcp', mcp)"
      >
        同步到目标{{ installedTargetCount > 0 ? ` (${installedTargetCount})` : "" }}
      </button>
      <button
        class="secondary-button"
        :disabled="loading"
        type="button"
        @click="$emit('editMcp', mcp)"
      >
        修改
      </button>
      <button
        class="danger-button"
        :disabled="loading"
        type="button"
        @click="$emit('requestDeleteMcp', [mcp.name])"
      >
        删除
      </button>
    </template>
  </AppCard>
</template>
