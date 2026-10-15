<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAppStore } from "./app/stores/app";
import AppModeRail from "./app/components/AppModeRail.vue";
import AboutWorkspace from "./app/components/AboutWorkspace.vue";
import SessionWorkspace from "./features/sessions/SessionWorkspace.vue";
import { captureKeepAliveScroll } from "./features/sessions/composables/useKeepAliveScroll";
import Workspace from "./features/workspace/Workspace.vue";
import ProvidersWorkspace from "./features/providers/ProvidersWorkspace.vue";
import UsageWorkspace from "./features/usage/UsageWorkspace.vue";
import "./app/styles/shell.css";

const appStore = useAppStore();
const { appMode } = storeToRefs(appStore);

// $onAction 的回调在 action 执行前跑:这时会话视图的 DOM 还在文档里,能读到
// 真实滚动位置;等到视图被 KeepAlive 移走就只剩 0 了。
appStore.$onAction(({ name, args }) => {
  if (name === "setAppMode" && args[0] !== "sessions") {
    captureKeepAliveScroll();
  }
});
</script>

<template>
  <div class="app-root-shell">
    <AppModeRail
      :mode="appMode"
      @about="appStore.setAppMode('about')"
      @change="appStore.setAppMode"
    />
    <div class="app-main">
      <!-- 会话工作区用 KeepAlive 保留:去用量统计看曲线再回来,搜索词、排序、
           已加载的页与滚动位置都还在(列表数据本身在 store 里,这里保留的是
           本地视图状态)。其余模式维持原有的卸载重建行为。 -->
      <KeepAlive>
        <SessionWorkspace v-if="appMode === 'sessions'" />
      </KeepAlive>
      <ProvidersWorkspace v-if="appMode === 'providers'" />
      <UsageWorkspace v-else-if="appMode === 'usage'" />
      <Workspace v-else-if="appMode === 'workspace'" />
      <AboutWorkspace v-else-if="appMode === 'about'" />
    </div>
  </div>
</template>
