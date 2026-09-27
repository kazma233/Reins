<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import AppCard from "@shared/ui/AppCard.vue";
import { applyProviderToApp, removeExternalEntry, removeProviderFromApp } from "../../api";
import type {
  ApplyProviderInput,
  ProviderAppEntry,
  ProviderAppId,
  ProviderView,
} from "../../generated";
import {
  APP_LABELS,
  ENTRY_STATUS_LABELS,
  applyCandidates,
  protocolCompatible,
  protocolLabel,
  reasoningEffortMappingText,
} from "../../model";
import { useProvidersAction } from "../../composables/useProvidersAction";
import { useProvidersState } from "../../composables/useProvidersState";
import ProviderApplyDialog from "../dialogs/ProviderApplyDialog.vue";

const { state, loadingState, runningAction } = useProvidersState();
const { runProvidersAction } = useProvidersAction();

const applyDialog = reactive<{ open: boolean; app: ProviderAppId | null; providerId: string }>({
  open: false,
  app: null,
  providerId: "",
});
const removeTarget = ref<
  | { app: ProviderAppId; providerId: string; external: false }
  | { app: ProviderAppId; entryKey: string; external: true }
  | null
>(null);

// 外部条目是用户手工写入工具配置的内容，删除后无法从 Reins 恢复，文案需强警告。
const removeConfirmText = computed(() => {
  if (!removeTarget.value) {
    return { title: "", description: "" };
  }
  const appLabel = APP_LABELS[removeTarget.value.app];
  return removeTarget.value.external
    ? {
        title: "删除外部条目",
        description: `将从 ${appLabel} 的配置文件中直接删除外部条目「${removeTarget.value.entryKey}」。这是工具配置中手工添加的内容，删除后无法从 Reins 恢复。`,
      }
    : {
        title: "移除应用",
        description: `将从 ${appLabel} 的配置文件中移除 reins-${removeTarget.value.providerId} 相关条目（默认模型仍引用时随移除清空）。`,
      };
});

function applyTarget(): { provider: ProviderView; app: ProviderAppId } | null {
  const provider = state.value?.providers.find((item) => item.id === applyDialog.providerId);
  if (!provider || applyDialog.app === null) {
    return null;
  }
  return { provider, app: applyDialog.app };
}

function openApply(app: ProviderAppId, providerId: string) {
  applyDialog.app = app;
  applyDialog.providerId = providerId;
  applyDialog.open = true;
}

// 漂移条目的重新应用目标：平台仍存在才可重新应用；平台已删除的漂移
// 条目没有平台元数据，返回 null 不出按钮。
function reapplyProvider(entry: ProviderAppEntry): ProviderView | null {
  if (entry.status !== "drifted" || !entry.providerId) {
    return null;
  }
  return (
    state.value?.providers.find((provider) => provider.id === entry.providerId) ?? null
  );
}

function confirmApply(input: ApplyProviderInput) {
  runProvidersAction({
    action: () => applyProviderToApp(input),
    success: (result) => ({ message: result.detail || "已应用。" }),
    after: () => {
      applyDialog.open = false;
    },
  });
}

function requestRemove(app: ProviderAppId, providerId: string) {
  removeTarget.value = { app, providerId, external: false };
}

function requestRemoveExternal(app: ProviderAppId, entryKey: string) {
  removeTarget.value = { app, entryKey, external: true };
}

function confirmRemove() {
  if (!removeTarget.value) {
    return;
  }
  const target = removeTarget.value;
  runProvidersAction({
    action: () =>
      target.external
        ? removeExternalEntry(target.app, target.entryKey)
        : removeProviderFromApp(target.providerId, target.app),
    success: (result) => ({ message: result.detail || "已删除。" }),
    after: () => {
      removeTarget.value = null;
    },
  });
}
</script>

<template>
  <section class="providers-panel">
    <div class="providers-section-header">
      <div>
        <h3 class="providers-section-title">Agent</h3>
        <p class="providers-section-hint">
          反读五个工具的全局配置文件；`reins-` 前缀条目按 providers.yaml 归类为「已应用 / 配置有偏差」，其余按外部配置展示，可按条目删除。
        </p>
      </div>
    </div>

    <div v-if="loadingState" class="providers-loading">读取中…</div>

    <div v-else-if="state" class="providers-app-list">
      <AppCard v-for="appState in state.apps" :key="appState.app">
        <template #header>
          <h4 class="providers-app__title">{{ APP_LABELS[appState.app] }}</h4>
          <span class="providers-badge providers-badge--muted">
            {{ appState.additive ? "多提供商并存" : "单活动提供商" }}
          </span>
        </template>
        <template #headerMeta>
          <div class="providers-app__meta">
            <span
              v-for="path in appState.configPaths"
              :key="path"
              class="providers-app__path"
              :title="path"
            ><strong>配置</strong>{{ path }}</span>
            <span
              v-if="appState.supportedProtocols.length > 0"
            ><strong>协议</strong>{{
              appState.supportedProtocols
                .map((protocol) => protocolLabel(protocol))
                .join(" · ")
            }}</span>
          </div>
        </template>

        <p v-if="appState.loadError" class="providers-entry__note">
          读取失败：{{ appState.loadError }}
        </p>

        <ul v-if="appState.entries.length > 0" class="providers-entry-list">
          <li
            v-for="entry in appState.entries"
            :key="`${appState.app}-${entry.key}`"
            class="providers-entry-row"
          >
            <div class="providers-entry-row__head">
              <span class="providers-entry-row__key">{{ entry.key }}</span>
              <span class="providers-badge" :class="`providers-badge--${entry.status}`">
                {{ ENTRY_STATUS_LABELS[entry.status] }}
              </span>
              <span v-if="entry.defaultModelId" class="providers-badge providers-badge--muted">
                默认 {{ entry.defaultModelId }}
              </span>
              <span class="providers-entry-row__spacer" />
              <button
                v-if="reapplyProvider(entry)"
                class="providers-mini-button"
                :disabled="runningAction"
                type="button"
                @click="openApply(appState.app, reapplyProvider(entry)!.id)"
              >
                重新应用
              </button>
              <button
                v-if="entry.providerId && entry.status !== 'external'"
                class="providers-mini-button providers-mini-button--danger"
                :disabled="runningAction"
                type="button"
                @click="requestRemove(appState.app, entry.providerId)"
              >
                移除
              </button>
              <button
                v-if="entry.status === 'external' && appState.app !== 'claude'"
                class="providers-mini-button providers-mini-button--danger"
                :disabled="runningAction"
                type="button"
                @click="requestRemoveExternal(appState.app, entry.key)"
              >
                删除
              </button>
            </div>
            <div class="providers-entry-row__sub">
              <span v-if="entry.baseUrl" class="providers-entry-row__url">{{ entry.baseUrl }}</span>
              <span v-if="entry.protocol">协议：{{ protocolLabel(entry.protocol) }}</span>
              <span v-if="entry.modelIds.length > 0">模型：{{ entry.modelIds.join("、") }}</span>
              <span v-if="reasoningEffortMappingText(appState)">
                思考等级：{{ reasoningEffortMappingText(appState) }}
              </span>
            </div>
            <div v-for="note in entry.notes" :key="note" class="providers-entry-row__note">
              {{ note }}
            </div>
          </li>
        </ul>

        <p v-else-if="!appState.loadError" class="providers-empty">
          {{ appState.configExists ? "没有识别到聚合提供商条目。" : "未检测到配置文件。" }}
        </p>

        <!-- 应用按钮放卡片最底部：提供商多时 flex-wrap 自动换行；候选为空（全部已应用）时整条隐藏 -->
        <template v-if="applyCandidates(appState, state.providers).length > 0" #footer>
          <span class="providers-app__actions-label">应用提供商</span>
          <button
            v-for="provider in applyCandidates(appState, state.providers)"
            :key="`${appState.app}-apply-${provider.id}`"
            class="secondary-button providers-app__chip"
            :disabled="runningAction || !protocolCompatible(appState, provider)"
            type="button"
            :title="protocolCompatible(appState, provider) ? '' : '协议不兼容'"
            @click="openApply(appState.app, provider.id)"
          >
            {{ provider.label }}
          </button>
        </template>
      </AppCard>
    </div>

    <ProviderApplyDialog
      v-if="applyDialog.app !== null && applyTarget()"
      :open="applyDialog.open"
      :loading="runningAction"
      :app="applyDialog.app"
      :provider="applyTarget()!.provider"
      :state="state"
      @close="applyDialog.open = false"
      @confirm="confirmApply"
    />

    <ConfirmDialog
      :open="removeTarget !== null"
      title-id="provider-remove-from-app-title"
      :title="removeConfirmText.title"
      :description="removeConfirmText.description"
      confirm-label="删除"
      :loading="runningAction"
      @close="removeTarget = null"
      @confirm="confirmRemove"
    />
  </section>
</template>
