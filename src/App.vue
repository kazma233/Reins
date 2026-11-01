<script setup lang="ts">
import { ref } from "vue";
import type { AppMode } from "./app/app-mode";
import AppModeRail from "./app/components/AppModeRail.vue";
import AboutWorkspace from "./app/components/AboutWorkspace.vue";
import SessionWorkspace from "./features/sessions/SessionWorkspace.vue";
import { captureKeepAliveScroll } from "./features/sessions/composables/useKeepAliveScroll";
import Workspace from "./features/workspace/Workspace.vue";
import ProvidersWorkspace from "./features/providers/ProvidersWorkspace.vue";
import UsageWorkspace from "./features/usage/UsageWorkspace.vue";
import "./app/styles/shell.css";

const appMode = ref<AppMode>("workspace");

// 抓拍必须排在赋值之前:此时会话视图的 DOM 还在文档里,能读到真实滚动位置;
// 等渲染把视图交给 KeepAlive 移走就只剩 0 了。
function changeAppMode(mode: AppMode) {
  if (mode !== "sessions") {
    captureKeepAliveScroll();
  }
  appMode.value = mode;
}
</script>

<template>
  <div class="app-root-shell">
    <AppModeRail
      :mode="appMode"
      @about="changeAppMode('about')"
      @change="changeAppMode"
    />
    <div class="app-main">
      <!-- 会话工作区用 KeepAlive 保留:去用量统计看曲线再回来,搜索词、排序、
           已加载的页、列表数据与滚动位置都还在。其余模式维持原有的卸载重建行为。 -->
      <KeepAlive>
        <SessionWorkspace
          v-if="appMode === 'sessions'"
          @open-usage="changeAppMode('usage')"
        />
      </KeepAlive>
      <ProvidersWorkspace v-if="appMode === 'providers'" />
      <UsageWorkspace
        v-else-if="appMode === 'usage'"
        @back="changeAppMode('sessions')"
      />
      <Workspace v-else-if="appMode === 'workspace'" />
      <AboutWorkspace v-else-if="appMode === 'about'" />
    </div>
  </div>
</template>
