<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAppStore } from "./app/stores/app";
import AppModeRail from "./app/components/AppModeRail.vue";
import SessionWorkspace from "./features/sessions/SessionWorkspace.vue";
import Workspace from "./features/workspace/Workspace.vue";
import "./app/styles/shell.css";

const appStore = useAppStore();
const { appMode } = storeToRefs(appStore);
</script>

<template>
  <div class="app-root-shell">
    <AppModeRail :mode="appMode" @change="appStore.setAppMode" />
    <div class="app-main">
      <SessionWorkspace v-if="appMode === 'sessions'" />
      <Workspace v-else />
    </div>
  </div>
</template>
