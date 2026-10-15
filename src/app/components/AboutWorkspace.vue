<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { AGENT_IDS, AGENT_LABELS } from "@shared/lib/agent-labels";
import "./about-workspace.css";

// 关于页是左栏的顶层页(与配置与分发/模型配置/历史会话平级),页面内不放返回:
// 换页由左栏承担,和用量统计那种"从会话列表钻进去的子页"不同。
// 版本只有一个来源:tauri.conf.json(productName/version),不在这里另存常量。
const version = ref<string | null>(null);

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    // 取不到版本(非 Tauri 环境)时留占位:关于页不该因为这个调用失败而打不开。
  }
});

// 工具名取自共享常量表,不在这里另写一份拼写。
const toolNames = AGENT_IDS.map((id) => AGENT_LABELS[id] ?? id).join(" / ");
</script>

<template>
  <div class="about-workspace">
    <header class="about-header">
      <h2 class="about-header__title">关于 Reins</h2>
    </header>

    <div class="about-body">
      <p class="about-body__version">版本 {{ version ?? "未知" }}</p>
      <p class="about-body__desc">
        本地桌面工具：浏览各 AI 编码工具的历史会话、把 Skills 与 MCP 配置分发到多个
        agent target、集中管理聚合提供商的模型配置并一键应用。
      </p>
      <p class="about-body__tools">{{ toolNames }}</p>
    </div>
  </div>
</template>
