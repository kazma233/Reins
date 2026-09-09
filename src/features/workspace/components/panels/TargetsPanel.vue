<script setup lang="ts">
import { computed } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import ProjectCreateDialog from "../dialogs/ProjectCreateDialog.vue";
import TargetCreateDialog from "../dialogs/TargetCreateDialog.vue";
import { useProjectMutations } from "../../composables/useProjectMutations";
import { useTargetMutations } from "../../composables/useTargetMutations";
import { useWorkspaceState } from "../../composables/useWorkspaceState";
import { formatTargetLabel } from "../../model";

const { configDocument, loadingInspection, runningAction } = useWorkspaceState();

const {
  targetCreateDialog,
  targetDeleteDialog,
  openTargetCreateDialog,
  openTargetEditDialog,
  closeTargetCreateDialog,
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
  handlePickProjectPath,
  handleSubmitProject,
  openProjectDeleteDialog,
  closeProjectDeleteDialog,
  handleConfirmDeleteProject,
} = useProjectMutations();

const configTargets = computed(() => configDocument.value?.config?.targets ?? []);
const configProjects = computed(() => configDocument.value?.config?.projects ?? []);

const projectsWithEnabledAgents = computed(() =>
  configProjects.value.map((project) => ({
    project,
    enabledAgents: project.agents.filter((agent) => agent.enabled),
  })),
);
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
      <div v-if="loadingInspection" class="loading-pill">正在检查状态...</div>

      <div class="manager-panel-section manager-panel-section--fill">
        <div class="manager-scroll-region">
          <div class="manager-target-list">
            <div class="manager-target-group">
              <div class="manager-target-group__header">
                <span>全局 targets · {{ configTargets.length }} 项</span>
              </div>
              <template v-if="configTargets.length">
                <div
                  v-for="target in configTargets"
                  :key="target.id"
                  class="manager-target-row"
                >
                  <span :class="`manager-target-row__icon ${target.enabled ? 'icon--on' : 'icon--off'}`">
                    {{ target.enabled ? "✓" : "✗" }}
                  </span>
                  <div class="manager-target-row__body">
                    <div class="manager-target-row__headline">
                      <span class="manager-target-row__name">{{ formatTargetLabel(target.id) }}</span>
                      <span :class="`manager-target-row__pill ${target.enabled ? 'pill--on' : 'pill--off'}`">
                        {{ target.enabled ? "启用" : "禁用" }}
                      </span>
                    </div>
                    <div class="manager-target-row__meta manager-target-row__meta--paths">
                      <span>skill: {{ target.skillDir }}</span>
                      <span v-if="target.configPath">
                        mcp: {{ target.configPath }} ({{ target.mcpConfigPrefix }})
                      </span>
                    </div>
                  </div>
                  <div class="manager-target-row__actions">
                    <button
                      class="secondary-button"
                      :disabled="runningAction"
                      type="button"
                      @click="openTargetEditDialog(target)"
                    >
                      编辑
                    </button>
                    <button
                      :class="target.enabled ? 'warning-button' : 'secondary-button'"
                      :disabled="runningAction"
                      type="button"
                      @click="toggleTargetEnabled(target)"
                    >
                      {{ target.enabled ? "停用" : "启用" }}
                    </button>
                    <button
                      class="danger-button"
                      :disabled="runningAction"
                      type="button"
                      @click="openTargetDeleteDialog(target.id)"
                    >
                      删除
                    </button>
                  </div>
                </div>
              </template>
              <div v-else class="empty-state">当前没有全局 target。</div>
            </div>

            <div class="manager-target-group">
              <div class="manager-target-group__header">
                <span>项目 targets · {{ configProjects.length }} 项</span>
              </div>
              <template v-if="configProjects.length">
                <div
                  v-for="{ project, enabledAgents } in projectsWithEnabledAgents"
                  :key="project.id"
                  class="manager-target-row manager-target-row--project"
                >
                  <span class="manager-target-row__icon icon--project">P</span>
                  <div class="manager-target-row__body">
                    <div class="manager-target-row__headline manager-target-row__headline--project">
                      <span class="manager-target-row__name">{{ project.id }}</span>
                      <span class="manager-target-row__pill pill--on">
                        {{ enabledAgents.length }} agents
                      </span>
                      <div class="manager-target-row__actions manager-target-row__actions--project">
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
                    </div>
                    <div class="manager-target-row__meta">
                      <span>{{ project.path }}</span>
                    </div>
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
                          <span :class="`manager-target-row__pill ${target.enabled ? 'pill--on' : 'pill--off'}`">
                            {{ target.enabled ? "启用" : "禁用" }}
                          </span>
                        </div>
                        <div class="manager-project-target-path__items">
                          <span>skill: {{ target.skillDir }}</span>
                          <span v-if="target.configPath">
                            mcp: {{ target.configPath }} ({{ target.mcpConfigPrefix }})
                          </span>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
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
    @close="closeTargetCreateDialog"
    @confirm="handleSubmitTarget"
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
    description="删除后该 target 的 skills 目录与 mcp 配置路径不会被清理，但不再参与后续同步。"
    @close="closeTargetDeleteDialog"
    @confirm="handleConfirmDeleteTarget"
  />

  <!-- --- project create / delete --- -->

  <ProjectCreateDialog
    :form="projectCreateDialog.form"
    :open="projectCreateDialog.open"
    :loading="projectCreateDialog.loading || runningAction"
    @close="closeProjectCreateDialog"
    @confirm="handleSubmitProject"
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
    description="删除后该项目的 agent 配置会从工作区移除，已分发的软链接和配置不会自动回滚。"
    @close="closeProjectDeleteDialog"
    @confirm="handleConfirmDeleteProject"
  />
</template>
