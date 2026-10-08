<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import AppCard from "@shared/ui/AppCard.vue";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import AppLoadError from "@shared/ui/AppLoadError.vue";
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
  reapplyProvider,
} from "../../model";
import { useProvidersAction } from "../../composables/useProvidersAction";
import { useProvidersState } from "../../composables/useProvidersState";
import ProviderApplyDialog from "../dialogs/ProviderApplyDialog.vue";

const {
  state,
  error,
  appErrors,
  loadingState,
  runningAction,
  retryProvidersState,
  reloadProviderAppState,
} = useProvidersState();
const { runProvidersAction } = useProvidersAction();

const applyDialog = reactive<{ open: boolean; app: ProviderAppId | null; providerId: string }>({
  open: false,
  app: null,
  providerId: "",
});
// 弹窗操作失败的内联错误：与弹窗同生命周期，关闭/重试时清空。
const applyError = ref<string | null>(null);
const removeError = ref<string | null>(null);
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
  applyError.value = null;
  applyDialog.open = true;
}

function closeApplyDialog() {
  applyDialog.open = false;
  applyError.value = null;
}

// 条目行「重新应用」的目标：已应用与配置有偏差的条目都能重开应用弹窗
// （多提供商并存时用它切换默认模型）；平台已删除的偏差条目与外部条目
// 没有平台元数据，返回 null 不出按钮。
function reapplyTarget(entry: ProviderAppEntry): ProviderView | null {
  return reapplyProvider(entry, state.value?.providers ?? []);
}

function confirmApply(input: ApplyProviderInput) {
  applyError.value = null;
  runProvidersAction({
    action: () => applyProviderToApp(input),
    // 应用只改这一个工具的配置，刷新这一张卡片即可。
    reloadApp: input.app,
    onSuccess: () => {
      applyDialog.open = false;
    },
    onError: (message) => {
      applyError.value = message;
    },
  });
}

function requestRemove(app: ProviderAppId, providerId: string) {
  removeError.value = null;
  removeTarget.value = { app, providerId, external: false };
}

function requestRemoveExternal(app: ProviderAppId, entryKey: string) {
  removeError.value = null;
  removeTarget.value = { app, entryKey, external: true };
}

function closeRemoveDialog() {
  removeTarget.value = null;
  removeError.value = null;
}

function confirmRemove() {
  if (!removeTarget.value) {
    return;
  }
  const target = removeTarget.value;
  removeError.value = null;
  runProvidersAction({
    action: () =>
      target.external
        ? removeExternalEntry(target.app, target.entryKey)
        : removeProviderFromApp(target.providerId, target.app),
    // 删除只改这一个工具的配置，刷新这一张卡片即可。
    reloadApp: target.app,
    onSuccess: () => {
      removeTarget.value = null;
    },
    onError: (message) => {
      removeError.value = message;
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
          反读五个工具的全局配置文件；`reins-` 前缀条目按 providers.yaml 归类为「已应用 / 配置有偏差」，其余按外部配置展示，可按条目删除。已应用的条目可重新打开应用弹窗，用于切换默认模型。
        </p>
      </div>
    </div>

    <AppLoadError
      v-if="error"
      :message="error"
      :retrying="loadingState"
      @retry="retryProvidersState"
    />

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

        <div v-if="appErrors[appState.app]" class="providers-app__refresh-error">
          <p class="providers-entry__note providers-entry__note--error">
            刷新失败：{{ appErrors[appState.app] }}
          </p>
          <button
            class="providers-mini-button"
            type="button"
            @click="reloadProviderAppState(appState.app)"
          >
            重试
          </button>
        </div>

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
                v-if="reapplyTarget(entry)"
                class="providers-mini-button"
                :disabled="runningAction"
                type="button"
                @click="openApply(appState.app, reapplyTarget(entry)!.id)"
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
      :error="applyError"
      @close="closeApplyDialog"
      @confirm="confirmApply"
    />

    <ConfirmDialog
      :open="removeTarget !== null"
      title-id="provider-remove-from-app-title"
      :title="removeConfirmText.title"
      :description="removeConfirmText.description"
      confirm-label="删除"
      :loading="runningAction"
      @close="closeRemoveDialog"
      @confirm="confirmRemove"
    >
      <AppFieldError :message="removeError" />
    </ConfirmDialog>
  </section>
</template>
