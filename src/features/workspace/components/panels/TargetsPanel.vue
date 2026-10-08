<script setup lang="ts">
import { computed } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import AppCard from "@shared/ui/AppCard.vue";
import AppLoadError from "@shared/ui/AppLoadError.vue";
import AppResultBadge from "@shared/ui/AppResultBadge.vue";
import ProjectCreateDialog from "../dialogs/ProjectCreateDialog.vue";
import TargetCreateDialog from "../dialogs/TargetCreateDialog.vue";
import { useProjectMutations } from "../../composables/useProjectMutations";
import { useTargetMutations } from "../../composables/useTargetMutations";
import { useWorkspaceState } from "../../composables/useWorkspaceState";
import { useWorkspaceStore } from "../../stores/workspace";
import { formatTargetLabel } from "../../model";

const store = useWorkspaceStore();
const { configDocument, loadingInspection, runningAction, loadError, retryWorkspaceState } =
  useWorkspaceState();

const {
  targetCreateDialog,
  targetDeleteDialog,
  pendingToggleTargetIds,
  openTargetCreateDialog,
  openTargetEditDialog,
  closeTargetCreateDialog,
  clearTargetFormError,
  handleApplyBuiltinTargetPreset,
  toggleTargetEnabled,
  openTargetDeleteDialog,
  closeTargetDeleteDialog,
  handlePickTargetSkillDirectory,
  handlePickTargetMcpConfigFile,
  handleSubmitTarget,
  handleConfirmDeleteTarget,
} = useTargetMutations();

const {
  projectCreateDialog,
  projectDeleteDialog,
  openProjectCreateDialog,
  openProjectEditDialog,
  closeProjectCreateDialog,
  clearProjectFormError,
  handlePickProjectPath,
  handleSubmitProject,
  openProjectDeleteDialog,
  closeProjectDeleteDialog,
  handleConfirmDeleteProject,
} = useProjectMutations();

const configTargets = computed(() => configDocument.value?.config?.targets ?? []);
const configProjects = computed(() => configDocument.value?.config?.projects ?? []);

// 启停失败的结果跟卡片按钮放在一起：结果按 target id 常驻，reload 重建卡片后仍在。
function toggleError(targetId: string): string | null {
  return store.actionResults[`target-toggle:${targetId}`]?.message ?? null;
}

// 启停不进整页忙碌态，进行中的卡片自己锁定（文案不变，按钮宽度不会跟着跳）。
function isTogglingTarget(targetId: string): boolean {
  return pendingToggleTargetIds.value.has(targetId);
}
</script>

<template>
  <section class="manager-stack manager-stack--stretch">
    <Teleport defer to="#workspace-panel-actions">
      <button
        class="secondary-button"
        :disabled="runningAction"
        type="button"
        @click="openTargetCreateDialog"
      >
        新增全局 Target
      </button>
      <button
        class="secondary-button"
        :disabled="runningAction"
        type="button"
        @click="openProjectCreateDialog"
      >
        新增项目 Target
      </button>
    </Teleport>
    <article class="manager-panel manager-panel--fill">
      <!-- 读取失败时错误条压在内容之上，重试入口就在出错的地方 -->
      <AppLoadError
        v-if="loadError"
        :message="loadError"
        :retrying="loadingInspection"
        @retry="retryWorkspaceState"
      />
      <div v-if="loadingInspection" class="loading-pill">正在检查状态...</div>

      <div class="manager-panel-section manager-panel-section--fill">
        <div class="manager-scroll-region">
          <div class="manager-target-list">
            <div class="manager-target-group">
              <div class="manager-target-group__header">
                <span>全局 targets · {{ configTargets.length }} 项</span>
              </div>
              <template v-if="configTargets.length">
                <AppCard
                  v-for="target in configTargets"
                  :key="target.id"
                >
                  <template #header>
                    <span class="manager-target-row__name-text">{{ formatTargetLabel(target.id) }}</span>
                    <!-- 启用是常态不标注，只有停用才提示；状态变化由「停用/启用」按钮承载 -->
                    <span v-if="!target.enabled" class="manager-target-row__pill pill--off">已停用</span>
                  </template>
                  <template #ext>
                    <div class="manager-target-row__actions">
                      <AppResultBadge
                        v-if="toggleError(target.id)"
                        placement="bottom"
                        :message="toggleError(target.id) ?? ''"
                        tone="danger"
                      />
                      <button
                        class="secondary-button"
                        :disabled="runningAction || isTogglingTarget(target.id)"
                        type="button"
                        @click="openTargetEditDialog(target)"
                      >
                        编辑
                      </button>
                      <button
                        class="secondary-button"
                        :disabled="runningAction || isTogglingTarget(target.id)"
                        type="button"
                        @click="toggleTargetEnabled(target)"
                      >
                        {{ target.enabled ? "停用" : "启用" }}
                      </button>
                      <button
                        class="danger-button"
                        :disabled="runningAction || isTogglingTarget(target.id)"
                        type="button"
                        @click="openTargetDeleteDialog(target.id)"
                      >
                        删除
                      </button>
                    </div>
                  </template>

                  <div class="manager-target-row__meta manager-target-row__meta--paths">
                    <span><strong>skill</strong><span>{{ target.skillDir }}</span></span>
                    <span v-if="target.configPath"><strong>mcp</strong><span>{{ target.configPath }} ({{ target.mcpConfigPrefix }})</span></span>
                  </div>
                </AppCard>
              </template>
              <div v-else class="empty-state">当前没有全局 target。</div>
            </div>

            <div class="manager-target-group">
              <div class="manager-target-group__header">
                <span>项目 targets · {{ configProjects.length }} 项</span>
              </div>
              <template v-if="configProjects.length">
                <AppCard
                  v-for="project in configProjects"
                  :key="project.id"
                >
                  <template #header>
                    <span class="manager-target-row__name-text">{{ project.id }}</span>
                  </template>
                  <template #headerMeta>
                    <span class="manager-header-path"><strong>路径</strong>{{ project.path }}</span>
                  </template>
                  <template #ext>
                    <div class="manager-target-row__actions">
                      <button
                        class="secondary-button"
                        :disabled="runningAction"
                        type="button"
                        @click="openProjectEditDialog(project)"
                      >
                        编辑
                      </button>
                      <button
                        class="danger-button"
                        :disabled="runningAction"
                        type="button"
                        @click="openProjectDeleteDialog(project.id)"
                      >
                        删除
                      </button>
                    </div>
                  </template>

                  <div class="manager-project-target-paths">
                    <div
                      v-for="target in project.agents"
                      :key="target.id"
                      class="manager-project-target-path"
                    >
                      <div class="manager-project-target-path__headline">
                        <span class="manager-project-target-path__name">
                          {{ formatTargetLabel(target.id) }}
                        </span>
                        <!-- 启用是常态不标注，与外层卡片的状态处理一致 -->
                        <span v-if="!target.enabled" class="manager-target-row__pill pill--off">已停用</span>
                      </div>
                      <div class="manager-project-target-path__items">
                        <span><strong>skill</strong><span>{{ target.skillDir }}</span></span>
                        <span v-if="target.configPath"><strong>mcp</strong><span>{{ target.configPath }} ({{ target.mcpConfigPrefix }})</span></span>
                      </div>
                    </div>
                  </div>
                </AppCard>
              </template>
              <div v-else class="empty-state">当前还没有项目 targets。</div>
            </div>
          </div>
        </div>
      </div>
    </article>
  </section>

  <!-- --- target create / delete --- -->

  <TargetCreateDialog
    :form="targetCreateDialog.form"
    :open="targetCreateDialog.open"
    :loading="targetCreateDialog.loading || runningAction"
    :error="targetCreateDialog.error"
    @close="closeTargetCreateDialog"
    @confirm="handleSubmitTarget"
    @clear-field-error="clearTargetFormError"
    @apply-builtin-preset="handleApplyBuiltinTargetPreset"
    @pick-mcp-config-file="handlePickTargetMcpConfigFile"
    @pick-skill-directory="handlePickTargetSkillDirectory"
  />

  <ConfirmDialog
    :open="targetDeleteDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="Target"
    :title="`删除 target: ${targetDeleteDialog.targetId ?? ''}`"
    title-id="target-delete-dialog-title"
    confirm-button-class-name="danger-button"
    :confirm-label="runningAction ? '删除中...' : '确认删除'"
    :loading="runningAction"
    :error="targetDeleteDialog.error"
    description="删除后该 target 的 skills 目录与 mcp 配置路径不会被清理，但不再参与后续同步。"
    @close="closeTargetDeleteDialog"
    @confirm="handleConfirmDeleteTarget"
  />

  <!-- --- project create / delete --- -->

  <ProjectCreateDialog
    :form="projectCreateDialog.form"
    :open="projectCreateDialog.open"
    :loading="projectCreateDialog.loading || runningAction"
    :error="projectCreateDialog.error"
    @close="closeProjectCreateDialog"
    @confirm="handleSubmitProject"
    @clear-field-error="clearProjectFormError"
    @pick-project-path="handlePickProjectPath"
  />

  <ConfirmDialog
    :open="projectDeleteDialog.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="Project"
    :title="`删除项目: ${projectDeleteDialog.projectId ?? ''}`"
    title-id="project-delete-dialog-title"
    confirm-button-class-name="danger-button"
    :confirm-label="runningAction ? '删除中...' : '确认删除'"
    :loading="runningAction"
    :error="projectDeleteDialog.error"
    description="删除后该项目的 agent 配置会从工作区移除，已分发的软链接和配置不会自动回滚。"
    @close="closeProjectDeleteDialog"
    @confirm="handleConfirmDeleteProject"
  />
</template>
