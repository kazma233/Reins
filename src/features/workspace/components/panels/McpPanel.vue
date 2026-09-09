<script setup lang="ts">
import { computed } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import McpCard from "../McpCard.vue";
import McpCreateDialog from "../dialogs/McpCreateDialog.vue";
import ProjectAgentPickerDialog from "../dialogs/ProjectAgentPickerDialog.vue";
import { useMcpMutations } from "../../composables/useMcpMutations";
import { useProjectAgentPicker } from "../../composables/useProjectAgentPicker";
import { useWorkspaceState } from "../../composables/useWorkspaceState";
import { formatTargetLabel } from "../../model";
import type { McpInspection } from "../../types";

const { configDocument, inspection, runningAction } = useWorkspaceState();

const {
  mcpCreateDialog,
  mcpDeleteDialog,
  mcpApplyPreviewDialog,
  mcpSyncConfirmDialog,
  mcpTargetRemoveDialog,
  openMcpCreateDialog,
  openMcpEditDialog,
  openMcpDeleteDialog,
  closeMcpCreateDialog,
  closeMcpSyncConfirmDialog,
  handleCreateWorkspaceMcp,
  handleConfirmSyncMcpConfig,
  closeMcpDeleteDialog,
  handleConfirmDeleteMcp,
  closeMcpTargetRemoveDialog,
  confirmMcpTargetRemove,
  handleToggleMcpTarget,
  closeMcpApplyPreviewDialog,
  handleConfirmApplyMcpPreview,
} = useMcpMutations();

const {
  projectAgentPickerDialog,
  enabledProjectEntries,
  projectAgentsByProjectId,
  pickerInstalledAgentIds,
  openProjectAgentPickerForMcp,
  closeProjectAgentPickerDialog,
  setProjectAgentPickerSelectedAgent,
  handleConfirmProjectAgentPicker,
} = useProjectAgentPicker();

const configMcps = computed(() => configDocument.value?.config?.mcps ?? []);
const targetIds = computed(
  () => configDocument.value?.config?.targets.map((target) => target.id) ?? [],
);

const inspectionMap = computed<Map<string, McpInspection>>(
  () => new Map((inspection.value?.mcps ?? []).map((mcp) => [mcp.name, mcp])),
);

// ProjectAgentPickerDialog requires non-null projectId; fall back to a safe
// placeholder when the picker is closed so prop types stay valid.
const pickerAgents = computed(() =>
  projectAgentPickerDialog.projectId
    ? (projectAgentsByProjectId.value.get(projectAgentPickerDialog.projectId) ?? [])
    : [],
);
const pickerProjectId = computed(() => projectAgentPickerDialog.projectId ?? "");
</script>

<template>
  <Teleport defer to="#workspace-panel-actions">
    <button
      class="secondary-button"
      :disabled="runningAction"
      type="button"
      @click="openMcpCreateDialog"
    >
      新增
    </button>
  </Teleport>

  <section class="manager-stack manager-stack--stretch">
    <article class="manager-panel manager-panel--fill">
      <div class="manager-panel-section manager-panel-section--fill">
        <template v-if="configMcps.length">
          <div class="manager-stack manager-scroll-region">
            <McpCard
              v-for="mcp in configMcps"
              :key="mcp.name"
              :mcp="mcp"
              :inspection="inspectionMap.get(mcp.name) ?? null"
              :loading="runningAction"
              :projects="enabledProjectEntries"
              :target-ids="targetIds"
              @edit-mcp="openMcpEditDialog"
              @request-delete-mcp="openMcpDeleteDialog"
              @toggle-mcp-target="handleToggleMcpTarget"
              @open-project-agent-picker="openProjectAgentPickerForMcp"
            />
          </div>
        </template>
        <div v-else class="empty-state">当前还没有 mcp。</div>
      </div>
    </article>
  </section>

  <!-- --- mcp create / delete / sync confirm / apply preview / target remove --- -->

  <McpCreateDialog
    :form="mcpCreateDialog.form"
    :open="mcpCreateDialog.open"
    :loading="mcpCreateDialog.loading || runningAction"
    @close="closeMcpCreateDialog"
    @confirm="handleCreateWorkspaceMcp"
  />

  <ConfirmDialog
    :open="mcpDeleteDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="MCP"
    :title="`删除 mcp: ${mcpDeleteDialog.serverNames.join(', ')}`"
    title-id="mcp-delete-dialog-title"
    confirm-button-class-name="danger-button"
    :confirm-label="runningAction ? '删除中...' : '确认删除'"
    :loading="runningAction"
    description="删除后该 mcp 的配置会从工作区移除，已分发的目标配置不会自动回滚。"
    @close="closeMcpDeleteDialog"
    @confirm="handleConfirmDeleteMcp"
  />

  <ConfirmDialog
    :open="mcpSyncConfirmDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="MCP"
    :title="`重命名并同步: ${mcpSyncConfirmDialog.originalName} → ${mcpSyncConfirmDialog.nextName}`"
    title-id="mcp-sync-confirm-dialog-title"
    cancel-label="取消"
    confirm-button-class-name="primary-button"
    :confirm-label="mcpSyncConfirmDialog.loading ? '同步中...' : `同步 ${mcpSyncConfirmDialog.targetIds.length} 个目标`"
    :loading="mcpSyncConfirmDialog.loading"
    :description="`该 mcp 已安装到 ${mcpSyncConfirmDialog.targetIds.length} 个目标，重命名后需要同步更新这些目标的配置。`"
    @close="closeMcpSyncConfirmDialog"
    @confirm="handleConfirmSyncMcpConfig"
  />

  <ConfirmDialog
    :open="mcpApplyPreviewDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="MCP"
    :title="`预览: ${mcpApplyPreviewDialog.serverName} → ${mcpApplyPreviewDialog.targetId ? formatTargetLabel(mcpApplyPreviewDialog.targetId) : ''}`"
    title-id="mcp-apply-preview-dialog-title"
    cancel-label="取消"
    confirm-button-class-name="primary-button"
    :confirm-label="mcpApplyPreviewDialog.submitting ? '写入中...' : '确认写入'"
    :loading="mcpApplyPreviewDialog.loading || mcpApplyPreviewDialog.submitting"
    @close="closeMcpApplyPreviewDialog"
    @confirm="handleConfirmApplyMcpPreview"
  >
    <template v-if="mcpApplyPreviewDialog.preview">
      <p class="manager-skill-description">
        将写入 {{ mcpApplyPreviewDialog.preview.configPath }}（{{ mcpApplyPreviewDialog.preview.format }}）
      </p>
      <pre class="manager-pre">{{ mcpApplyPreviewDialog.preview.content }}</pre>
    </template>
  </ConfirmDialog>

  <ConfirmDialog
    :open="mcpTargetRemoveDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="MCP"
    :title="`移除: ${mcpTargetRemoveDialog.serverName} 从 ${mcpTargetRemoveDialog.targetId ? formatTargetLabel(mcpTargetRemoveDialog.targetId) : ''}`"
    title-id="mcp-target-remove-dialog-title"
    cancel-label="取消"
    confirm-button-class-name="danger-button"
    :confirm-label="mcpTargetRemoveDialog.loading ? '移除中...' : '确认移除'"
    :loading="mcpTargetRemoveDialog.loading"
    description="会从目标配置文件中移除该 mcp 节点。"
    @close="closeMcpTargetRemoveDialog"
    @confirm="confirmMcpTargetRemove"
  />

  <!-- --- project agent picker --- -->

  <ProjectAgentPickerDialog
    :open="projectAgentPickerDialog.open"
    :loading="projectAgentPickerDialog.loading || runningAction"
    :agents="pickerAgents"
    :context-name="projectAgentPickerDialog.contextName"
    :project-id="pickerProjectId"
    :installed-agent-ids="pickerInstalledAgentIds"
    :selected-agent-id="projectAgentPickerDialog.selectedAgentId"
    @close="closeProjectAgentPickerDialog"
    @confirm="handleConfirmProjectAgentPicker"
    @selected-agent-change="setProjectAgentPickerSelectedAgent"
  />
</template>
