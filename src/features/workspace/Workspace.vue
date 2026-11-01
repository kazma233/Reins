<script setup lang="ts">
import { onMounted, ref } from "vue";
import { WORKSPACE_TAB_COPY } from "./model";
import type { WorkspaceTab } from "./types";
import { useWorkspaceState } from "./composables/useWorkspaceState";
import TargetsPanel from "./components/panels/TargetsPanel.vue";
import SkillsPanel from "./components/panels/SkillsPanel.vue";
import McpPanel from "./components/panels/McpPanel.vue";
import "./styles.css";

// tab 偏好跟着应用走:离开配置与分发再回来,停留的分页保持上次选择。
const workspaceTab = ref<WorkspaceTab>("targets");

// 数据状态由根组件持有一次;面板经 props 消费,写操作刷新同一份状态。
const workspace = useWorkspaceState();

onMounted(() => {
  void workspace.reloadWorkspaceState();
});
</script>

<template>
  <div class="manager-shell">
    <div class="manager-content">
      <header class="manager-content__header">
        <nav class="manager-nav">
          <button
            v-for="tab in WORKSPACE_TAB_COPY"
            :key="tab.id"
            :class="`manager-nav__button${workspaceTab === tab.id ? ' is-active' : ''}`"
            type="button"
            @click="workspaceTab = tab.id"
          >
            {{ tab.label }}
          </button>
        </nav>

        <div id="workspace-panel-actions" class="manager-content__actions"></div>
      </header>

      <TargetsPanel
        v-if="workspaceTab === 'targets'"
        :config-document="workspace.configDocument.value"
        :inspection="workspace.inspection.value"
        :loading-inspection="workspace.loadingInspection.value"
        :running-action="workspace.runningAction.value"
        :load-error="workspace.loadError.value"
        :workspace="workspace"
      />
      <SkillsPanel
        v-else-if="workspaceTab === 'skills'"
        :config-document="workspace.configDocument.value"
        :running-action="workspace.runningAction.value"
        :load-error="workspace.loadError.value"
        :workspace="workspace"
      />
      <McpPanel
        v-else-if="workspaceTab === 'mcp'"
        :config-document="workspace.configDocument.value"
        :inspection="workspace.inspection.value"
        :running-action="workspace.runningAction.value"
        :load-error="workspace.loadError.value"
        :workspace="workspace"
      />
    </div>
  </div>
</template>
