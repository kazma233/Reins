<script setup lang="ts">
import { computed } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import AppLoadError from "@shared/ui/AppLoadError.vue";
import McpCard from "../McpCard.vue";
import McpCreateDialog from "../dialogs/McpCreateDialog.vue";
import ProjectAgentPickerDialog from "../dialogs/ProjectAgentPickerDialog.vue";
import { useMcpMutations } from "../../composables/useMcpMutations";
import { useProjectAgentPicker } from "../../composables/useProjectAgentPicker";
import { useWorkspaceState } from "../../composables/useWorkspaceState";
import { useWorkspaceStore } from "../../stores/workspace";
import { formatTargetLabel, groupTargetIds } from "../../model";
import type { AgentTargetId, McpInspection } from "../../types";

const store = useWorkspaceStore();
const { configDocument, inspection, runningAction, loadError, retryWorkspaceState } =
  useWorkspaceState();

const {
  mcpCreateDialog,
  mcpDeleteDialog,
  mcpApplyPreviewDialog,
  mcpSyncConfirmDialog,
  mcpTargetRemoveDialog,
  mcpCardSyncDialog,
  mcpCardSyncNotice,
  openMcpCreateDialog,
  openMcpEditDialog,
  openMcpDeleteDialog,
  closeMcpCreateDialog,
  closeMcpSyncConfirmDialog,
  clearMcpFormError,
  handleCreateWorkspaceMcp,
  handleConfirmSyncMcpConfig,
  closeMcpDeleteDialog,
  handleConfirmDeleteMcp,
  closeMcpTargetRemoveDialog,
  confirmMcpTargetRemove,
  handleToggleMcpTarget,
  closeMcpApplyPreviewDialog,
  handleConfirmApplyMcpPreview,
  openMcpCardSyncDialog,
  closeMcpCardSyncDialog,
  confirmMcpCardSync,
} = useMcpMutations();

const {
  projectAgentPickerDialog,
  enabledProjectEntries,
  projectAgentsByProjectId,
  pickerInstalledAgentIds,
  pickerPendingDiff,
  openProjectAgentPickerForMcp,
  closeProjectAgentPickerDialog,
  toggleProjectAgentPickerAgent,
  openProjectAgentPickerConfirm,
  closeProjectAgentPickerConfirm,
  handleApplyProjectAgentPicker,
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

// 确认弹窗里与按钮一致只显示 agent 名，省掉项目前缀
function pickerAgentLabel(targetId: AgentTargetId) {
  const colonPos = targetId.indexOf(":");
  return colonPos >= 0 ? targetId.substring(colonPos + 1) : targetId;
}

// 同步确认里的目标按项目分组：项目 target 各归自己的 project，
// 全局 target 单独一组，与卡片上的分组口径一致。
const syncDialogTargetGroups = computed(() => groupTargetIds(mcpCardSyncDialog.targetIds));
const syncDialogGlobalTargets = computed(
  () => syncDialogTargetGroups.value.find((group) => group.key === "global")?.targetIds ?? [],
);
const syncDialogProjectTargets = computed(() =>
  syncDialogTargetGroups.value.filter((group) => group.key !== "global"),
);

const pickerApplyDanger = computed(
  () =>
    pickerPendingDiff.value.toAdd.length === 0 &&
    pickerPendingDiff.value.toRemove.length > 0,
);

// 项目 agent 选择器的部分失败按 mcp 名字挂在卡片上，弹窗关闭后仍可见。
function pickerWarning(serverName: string): string | null {
  return store.projectPickerWarnings[serverName] ?? null;
}

// 卡片同步结果同理：弹窗关闭后结果要留在触发它的卡片上。
function cardSyncResult(serverName: string): { text: string; failed: boolean } | null {
  const notice = mcpCardSyncNotice.value;
  if (!notice || notice.serverName !== serverName) return null;
  return { text: notice.text, failed: notice.failed };
}
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
        <AppLoadError
          v-if="loadError"
          :message="loadError"
          :retrying="runningAction"
          @retry="retryWorkspaceState"
        />
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
              :warning="pickerWarning(mcp.name)"
              :sync-result="cardSyncResult(mcp.name)"
              @edit-mcp="openMcpEditDialog"
              @request-delete-mcp="openMcpDeleteDialog"
              @toggle-mcp-target="handleToggleMcpTarget"
              @open-project-agent-picker="openProjectAgentPickerForMcp"
              @sync-mcp="openMcpCardSyncDialog"
            />
          </div>
        </template>
        <div v-else class="empty-state">当前还没有 mcp。</div>
      </div>
    </article>
  </section>

  <!-- --- mcp create / delete / sync confirm / apply preview / target remove --- -->

  <McpCreateDialog
    :error="mcpCreateDialog.form.errors.form ?? null"
    :form="mcpCreateDialog.form"
    :loading="mcpCreateDialog.loading || runningAction"
    :open="mcpCreateDialog.open"
    @clear-field-error="clearMcpFormError"
    @close="closeMcpCreateDialog"
    @confirm="handleCreateWorkspaceMcp"
  />

  <ConfirmDialog
    :open="mcpDeleteDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="mcp"
    :title="`删除 mcp: ${mcpDeleteDialog.serverNames.join(', ')}`"
    title-id="mcp-delete-dialog-title"
    confirm-button-class-name="danger-button"
    :confirm-label="runningAction ? '删除中...' : '确认删除'"
    :loading="runningAction"
    :error="mcpDeleteDialog.error"
    description="删除后该 mcp 的配置会从工作区移除，已分发的目标配置不会自动回滚。"
    @close="closeMcpDeleteDialog"
    @confirm="handleConfirmDeleteMcp"
  />

  <ConfirmDialog
    :open="mcpSyncConfirmDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="mcp"
    :title="`重命名并同步: ${mcpSyncConfirmDialog.originalName} → ${mcpSyncConfirmDialog.nextName}`"
    title-id="mcp-sync-confirm-dialog-title"
    cancel-label="取消"
    confirm-button-class-name="primary-button"
    :confirm-label="mcpSyncConfirmDialog.loading ? '同步中...' : `同步 ${mcpSyncConfirmDialog.targetIds.length} 个目标`"
    :loading="mcpSyncConfirmDialog.loading"
    :error="mcpSyncConfirmDialog.error"
    :description="`该 mcp 已安装到 ${mcpSyncConfirmDialog.targetIds.length} 个目标，重命名后需要同步更新这些目标的配置。`"
    @close="closeMcpSyncConfirmDialog"
    @confirm="handleConfirmSyncMcpConfig"
  />

  <ConfirmDialog
    :open="mcpApplyPreviewDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="mcp"
    :title="`预览: ${mcpApplyPreviewDialog.serverName} → ${mcpApplyPreviewDialog.targetId ? formatTargetLabel(mcpApplyPreviewDialog.targetId) : ''}`"
    title-id="mcp-apply-preview-dialog-title"
    cancel-label="取消"
    confirm-button-class-name="primary-button"
    :confirm-label="mcpApplyPreviewDialog.submitting ? '写入中...' : '确认写入'"
    :loading="mcpApplyPreviewDialog.loading || mcpApplyPreviewDialog.submitting"
    :error="mcpApplyPreviewDialog.error"
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
    eyebrow="mcp"
    :title="`移除: ${mcpTargetRemoveDialog.serverName} 从 ${mcpTargetRemoveDialog.targetId ? formatTargetLabel(mcpTargetRemoveDialog.targetId) : ''}`"
    title-id="mcp-target-remove-dialog-title"
    cancel-label="取消"
    confirm-button-class-name="danger-button"
    :confirm-label="mcpTargetRemoveDialog.loading ? '移除中...' : '确认移除'"
    :loading="mcpTargetRemoveDialog.loading"
    :error="mcpTargetRemoveDialog.error"
    description="会从目标配置文件中移除该 mcp 节点。"
    @close="closeMcpTargetRemoveDialog"
    @confirm="confirmMcpTargetRemove"
  />

  <ConfirmDialog
    :open="mcpCardSyncDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="mcp"
    :title="`同步 ${mcpCardSyncDialog.serverName} 到 ${mcpCardSyncDialog.targetIds.length} 个目标`"
    title-id="mcp-card-sync-dialog-title"
    cancel-label="取消"
    confirm-button-class-name="primary-button"
    :confirm-label="mcpCardSyncDialog.loading ? '同步中...' : '开始同步'"
    :loading="mcpCardSyncDialog.loading"
    :error="mcpCardSyncDialog.error"
    description="把当前配置写入这些已安装该 mcp 的目标配置文件；名称不变，只覆盖连接参数。"
    @close="closeMcpCardSyncDialog"
    @confirm="confirmMcpCardSync"
  >
    <div class="manager-sync-dialog__target-groups">
      <!-- 全局单独一段，项目各自一段 （带「项目」小标题区分），
           与卡片上的「全局 / 项目」分组口径一致 -->
      <div class="manager-sync-dialog__target-section">
        <span class="manager-sync-dialog__target-section-label">全局</span>
        <div class="manager-sync-dialog__target-section-body">
          <p v-if="syncDialogGlobalTargets.length > 0" class="manager-sync-dialog__target-list">
            {{ syncDialogGlobalTargets.map((id) => formatTargetLabel(id)).join("、") }}
          </p>
          <p v-else class="manager-sync-dialog__target-empty">未安装到全局 target</p>
        </div>
      </div>

      <div v-if="syncDialogProjectTargets.length > 0" class="manager-sync-dialog__target-section">
        <span class="manager-sync-dialog__target-section-label">项目</span>
        <div class="manager-sync-dialog__target-section-body">
          <div
            v-for="group in syncDialogProjectTargets"
            :key="group.key"
            class="manager-sync-dialog__target-row"
          >
            <span class="manager-sync-dialog__target-row-label">{{ group.label }}：</span>
            <span class="manager-sync-dialog__target-row-list">
              {{ group.targetIds.map((id) => pickerAgentLabel(id)).join("、") }}
            </span>
          </div>
        </div>
      </div>
    </div>
  </ConfirmDialog>

  <!-- --- project agent picker --- -->

  <ProjectAgentPickerDialog
    :open="projectAgentPickerDialog.open"
    :loading="projectAgentPickerDialog.loading || runningAction"
    :agents="pickerAgents"
    :context-name="projectAgentPickerDialog.contextName"
    :project-id="pickerProjectId"
    :installed-agent-ids="pickerInstalledAgentIds"
    :desired-agent-ids="projectAgentPickerDialog.desiredAgentIds"
    :pending-diff="pickerPendingDiff"
    :transport="projectAgentPickerDialog.transport"
    @close="closeProjectAgentPickerDialog"
    @toggle-agent="toggleProjectAgentPickerAgent"
    @apply="openProjectAgentPickerConfirm"
  />

  <ConfirmDialog
    :open="projectAgentPickerDialog.confirmOpen"
    dialog-class-name="manager-import-dialog"
    eyebrow="mcp · project"
    :title="`应用 MCP ${projectAgentPickerDialog.contextName} 到 ${pickerProjectId}`"
    title-id="project-agent-apply-dialog-title"
    cancel-label="取消"
    :confirm-button-class-name="pickerApplyDanger ? 'danger-button' : 'primary-button'"
    :confirm-label="projectAgentPickerDialog.loading ? '应用中...' : '确认应用'"
    :loading="projectAgentPickerDialog.loading || runningAction"
    description="确认后会将变更写入各 agent 目录下的 MCP 配置文件。"
    @close="closeProjectAgentPickerConfirm"
    @confirm="handleApplyProjectAgentPicker"
  >
    <div class="manager-stack">
      <div
        v-if="pickerPendingDiff.toAdd.length > 0"
        class="manager-target-buttons-group"
      >
        <span class="manager-target-buttons-group__label">新增</span>
        <div class="manager-target-buttons">
          <span
            v-for="id in pickerPendingDiff.toAdd"
            :key="id"
            class="pill success-pill"
          >{{ pickerAgentLabel(id) }}</span>
        </div>
      </div>
      <div
        v-if="pickerPendingDiff.toRemove.length > 0"
        class="manager-target-buttons-group"
      >
        <span class="manager-target-buttons-group__label">移除</span>
        <div class="manager-target-buttons">
          <span
            v-for="id in pickerPendingDiff.toRemove"
            :key="id"
            class="pill danger-pill"
          >{{ pickerAgentLabel(id) }}</span>
        </div>
      </div>
    </div>
  </ConfirmDialog>
</template>
