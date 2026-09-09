<script setup lang="ts">
import { computed } from "vue";
import { formatTimestamp } from "@shared/lib/format";
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
};

const props = withDefaults(defineProps<McpCardProps>(), {
  loading: false,
});

defineEmits<{
  editMcp: [mcp: McpConfigView];
  requestDeleteMcp: [serverNames: string[]];
  toggleMcpTarget: [serverName: string, targetId: AgentTargetId];
  openProjectAgentPicker: [serverName: string, projectId: string];
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
</script>

<template>
  <div class="manager-skill-row">
    <div class="manager-skill-row__header">
      <div class="manager-skill-main">
        <div class="manager-skill-headline">
          <div class="manager-skill-headline__meta">
            <span class="manager-skill-name">{{ mcp.name }}</span>
            <span v-if="mcp.createdAt" class="manager-skill-updated">
              新增于 {{ createdAtLabel }}
            </span>
          </div>
          <div class="manager-card-actions">
            <button
              class="secondary-button manager-skill-delete-button"
              :disabled="loading"
              type="button"
              @click="$emit('editMcp', mcp)"
            >
              修改
            </button>
            <button
              class="danger-button manager-skill-delete-button"
              :disabled="loading"
              type="button"
              @click="$emit('requestDeleteMcp', [mcp.name])"
            >
              删除
            </button>
          </div>
        </div>
        <p class="manager-skill-description">{{ summary }}</p>
        <p v-if="mcp.homepage" class="manager-skill-description">主页 · {{ mcp.homepage }}</p>
        <div class="manager-target-buttons">
          <McpTargetButton
            v-for="targetId in targetIds"
            :key="targetId"
            :server-name="mcp.name"
            :target-id="targetId"
            :target-item="targetItemById.get(targetId) ?? null"
            :loading="loading"
            @toggle="(serverName: string, tid: AgentTargetId) => $emit('toggleMcpTarget', serverName, tid)"
          />
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
    </div>
  </div>
</template>
