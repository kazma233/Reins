<script setup lang="ts">
import { onMounted } from "vue";
import { storeToRefs } from "pinia";
import AppToast from "@shared/ui/AppToast.vue";
import { WORKSPACE_TAB_COPY } from "./model";
import { useWorkspaceStore } from "./stores/workspace";
import { useWorkspaceNotice } from "./composables/useWorkspaceNotice";
import { useWorkspaceState } from "./composables/useWorkspaceState";
import TargetsPanel from "./components/panels/TargetsPanel.vue";
import SkillsPanel from "./components/panels/SkillsPanel.vue";
import McpPanel from "./components/panels/McpPanel.vue";
import "./styles.css";

// --- store + notice (singletons) ---

const store = useWorkspaceStore();
const { workspaceTab } = storeToRefs(store);
const { notice, clearNotice } = useWorkspaceNotice();

// --- workspace state (single bootstrap, fired from onMounted below) ---

const { reloadWorkspaceState } = useWorkspaceState();

onMounted(() => {
  void reloadWorkspaceState();
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
            @click="store.setWorkspaceTab(tab.id)"
          >
            {{ tab.label }}
          </button>
        </nav>

        <div id="workspace-panel-actions" class="manager-content__actions"></div>
      </header>

      <TargetsPanel v-if="workspaceTab === 'targets'" />
      <SkillsPanel v-else-if="workspaceTab === 'skills'" />
      <McpPanel v-else-if="workspaceTab === 'mcp'" />
    </div>

    <AppToast :notice="notice" @close="clearNotice" />
  </div>
</template>
