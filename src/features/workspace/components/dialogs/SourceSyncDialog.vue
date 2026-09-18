<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  getSyncSkillOptions,
  getSyncTargetOptions,
  refreshGitSkillSource,
  removeSourceSync,
  removeTargetSkillLink,
} from "../../api";
import { extractErrorMessage } from "@shared/lib/errors";
import AppTooltip from "@shared/ui/AppTooltip.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import SyncTargetGroups from "../SyncTargetGroups.vue";
import { useWorkspaceAction } from "../../composables/useWorkspaceAction";
import type {
  SkillLinkAssociation,
  SkillSourceConfigView,
  SyncSkillOption,
  SyncTargetOption,
} from "../../types";

type SourceSyncSnapshot = {
  sourceRoot: string;
  skills: SyncSkillOption[];
};

type SourceSyncDialogProps = {
  open: boolean;
  source: SkillSourceConfigView | null;
  sourceLabels: Record<string, string>;
  loading: boolean;
};

const props = defineProps<SourceSyncDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [skillPaths: string[], targetIds: string[], snapshot: SourceSyncSnapshot];
}>();

const { runWorkspaceAction } = useWorkspaceAction();

// Self-managed state: target/skill options are loaded by the dialog itself
// on open, kept locally while open, and reset on close. Parent only forwards
// the open flag, source, and parent-wide loading state (for conflict/overwrite
// confirmation dialogs running in parallel).
const loading = ref(false);
const confirming = ref(false);
// 移除操作改由弹窗自己发起，父级只保留弹窗开关状态；removingDestination 记录
// 正在移除的条目，让移除期间的操作入口一起禁用。
const removingSync = ref(false);
const removingDestination = ref<string | null>(null);
const refreshingSource = ref(false);
const targets = ref<SyncTargetOption[]>([]);
const skills = ref<SyncSkillOption[]>([]);
const selectedSkillPaths = ref<Set<string>>(new Set());
const selectedTargetIds = ref<Set<string>>(new Set());
const skillSearch = ref("");
const showUnmatched = ref(false);
const sourceRoot = ref("");
const loadError = ref<string | null>(null);

function getSelectableTargetIds(list: SyncTargetOption[]): string[] {
  return list.filter((t) => t.enabled && !t.linkedTargetId).map((t) => t.id);
}

// 递增令牌作废旧请求：关闭弹窗或切换来源后，在途响应不得再写回状态。
let loadToken = 0;

async function refreshOptions({ keepTargetSelection = false }: { keepTargetSelection?: boolean } = {}) {
  const source = props.source;
  if (!source) return;
  const token = ++loadToken;

  loading.value = true;
  loadError.value = null;

  try {
    const [targetOptions, skillResult] = await Promise.all([
      getSyncTargetOptions(),
      getSyncSkillOptions(source.id),
    ]);
    if (token !== loadToken) return;
    targets.value = targetOptions;
    skills.value = skillResult.skills;
    sourceRoot.value = skillResult.sourceRoot;
    skillSearch.value = "";
    showUnmatched.value = false;
    // Default-check skills that match the source's include patterns so
    // the user doesn't have to re-tick them every sync.
    selectedSkillPaths.value = new Set(
      skillResult.skills.filter((s) => s.matched !== false).map((s) => s.relativePath),
    );
    if (!keepTargetSelection) selectedTargetIds.value = new Set();
  } catch (error) {
    if (token !== loadToken) return;
    targets.value = [];
    skills.value = [];
    sourceRoot.value = "";
    selectedSkillPaths.value = new Set();
    selectedTargetIds.value = new Set();
    loadError.value = extractErrorMessage(error, "读取同步选项失败。");
  } finally {
    if (token === loadToken) loading.value = false;
  }
}

watch(
  () => [props.open, props.source?.id],
  ([open]) => {
    if (!open || !props.source) {
      loadToken += 1;
      return;
    }
    void refreshOptions();
  },
);

function handleToggleSkill(path: string) {
  const next = new Set(selectedSkillPaths.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  selectedSkillPaths.value = next;
}

function handleToggleTarget(id: string) {
  const next = new Set(selectedTargetIds.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  selectedTargetIds.value = next;
}

function handleSetTargets(targetIds: string[], selected: boolean) {
  const next = new Set(selectedTargetIds.value);
  for (const id of targetIds) {
    if (selected) next.add(id);
    else next.delete(id);
  }
  selectedTargetIds.value = next;
}

const query = computed(() => skillSearch.value.trim().toLowerCase());
const matchedSkills = computed(() => skills.value.filter((s) => s.matched !== false));
const unmatchedSkills = computed(() => skills.value.filter((s) => s.matched === false));
const visibleMatched = computed(() =>
  matchedSkills.value.filter((s) => {
    if (!query.value) return true;
    return (
      s.name.toLowerCase().includes(query.value) ||
      s.relativePath.toLowerCase().includes(query.value)
    );
  }),
);
const visibleUnmatched = computed(() =>
  unmatchedSkills.value.filter((s) => {
    if (!query.value) return true;
    return (
      s.name.toLowerCase().includes(query.value) ||
      s.relativePath.toLowerCase().includes(query.value)
    );
  }),
);
const visibleSkills = computed(() => [...visibleMatched.value, ...visibleUnmatched.value]);

const selectableTargetIds = computed(() => getSelectableTargetIds(targets.value));
const allSkillsSelected = computed(
  () => visibleSkills.value.length > 0 && visibleSkills.value.every((s) => selectedSkillPaths.value.has(s.relativePath)),
);
const allTargetsSelected = computed(
  () => selectableTargetIds.value.length > 0 && selectedTargetIds.value.size === selectableTargetIds.value.length,
);
const busy = computed(
  () =>
    loading.value ||
    confirming.value ||
    removingSync.value ||
    removingDestination.value !== null ||
    refreshingSource.value ||
    props.loading,
);
const canConfirm = computed(
  () => selectedSkillPaths.value.size > 0 && selectedTargetIds.value.size > 0 && !busy.value && !loadError.value,
);
const canRemoveSync = computed(() => selectedTargetIds.value.size > 0 && !busy.value);
const isGitSource = computed(() => props.source?.type === "git");
const canRefreshSource = computed(() => isGitSource.value && !busy.value);

function handleSelectAllSkills() {
  selectedSkillPaths.value = new Set(visibleSkills.value.map((s) => s.relativePath));
}

function handleDeselectAllSkills() {
  selectedSkillPaths.value = new Set();
}

function handleSelectAllTargets() {
  selectedTargetIds.value = new Set(selectableTargetIds.value);
}

function handleDeselectAllTargets() {
  selectedTargetIds.value = new Set();
}

function handleConfirm() {
  confirming.value = true;
  Promise.resolve(
    emit(
      "confirm",
      Array.from(selectedSkillPaths.value),
      Array.from(selectedTargetIds.value),
      {
        sourceRoot: sourceRoot.value,
        skills: skills.value.filter((skill) =>
          selectedSkillPaths.value.has(skill.relativePath),
        ),
      },
    ),
  ).finally(() => {
    confirming.value = false;
  });
}

// 后端按「链接目标落在来源目录下」判定归属，前端用 matchedSourceIds 做同一判定，
// 直接改内存里的目标列表，避免重新扫描（弹窗与各面板会闪一下加载态）。
function dropLinks(targetIds: Set<string>, shouldDrop: (link: SkillLinkAssociation) => boolean) {
  targets.value = targets.value.map((target) =>
    targetIds.has(target.id)
      ? { ...target, links: target.links.filter((link) => !shouldDrop(link)) }
      : target,
  );
}

// 移除勾选目标上属于当前来源的软链接。弹窗保持打开，列表就地更新，方便接着
// 同步或逐条清理。
async function handleRemoveSync() {
  const source = props.source;
  if (!source || selectedTargetIds.value.size === 0) return;
  const removedTargetIds = new Set(selectedTargetIds.value);

  removingSync.value = true;
  await runWorkspaceAction({
    action: () => removeSourceSync(source.id, Array.from(removedTargetIds)),
    success: (result) =>
      result.removed.length > 0
        ? { message: `已移除 ${result.removed.length} 个软链接。` }
        : { message: "没有需要移除的软链接。", tone: "info" },
    error: `移除 ${source.label} 同步失败。`,
    skipReload: true,
    after: () =>
      dropLinks(removedTargetIds, (link) => link.matchedSourceIds.includes(source.id)),
  });
  removingSync.value = false;
}

async function handleRemoveLink(targetId: string, destinationPath: string) {
  removingDestination.value = destinationPath;
  await runWorkspaceAction({
    action: () => removeTargetSkillLink(targetId, destinationPath),
    success: (item) => ({ message: `已移除 ${item.skillName} 的软链接。` }),
    error: "移除软链接失败。",
    skipReload: true,
    after: () =>
      dropLinks(new Set([targetId]), (link) => link.destinationPath === destinationPath),
  });
  removingDestination.value = null;
}

// 强制拉取忽略 24 小时自动更新间隔。拉完重扫来源，让左列直接反映远端最新
// 内容；目标勾选与来源无关，予以保留。
async function handleRefreshSource() {
  const source = props.source;
  if (!source || source.type !== "git") return;

  refreshingSource.value = true;
  await runWorkspaceAction({
    action: () => refreshGitSkillSource(source.id),
    success: `已从远端拉取 ${source.label}。`,
    error: `拉取 ${source.label} 失败。`,
    skipReload: true,
    after: () => {
      void refreshOptions({ keepTargetSelection: true });
    },
  });
  refreshingSource.value = false;
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="manager-import-dialog manager-sync-dialog"
    eyebrow="Sync"
    :title="`同步来源: ${source?.label ?? ''}`"
    title-id="source-sync-dialog-title"
    :close-disabled="busy"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        class="danger-button manager-sync-dialog__remove"
        :disabled="!canRemoveSync"
        type="button"
        @click="handleRemoveSync"
      >
        {{ removingSync ? "移除中..." : `移除同步 (${selectedTargetIds.size})` }}
      </button>
      <button class="primary-button" :disabled="!canConfirm" type="button" @click="handleConfirm">
        {{ confirming ? "同步中..." : "开始同步" }}
      </button>
    </template>

    <!-- 已有数据时保留列表（交互由 busy 锁住），重新扫描完成后再整体替换，
         避免拉取来源后整块弹窗闪成加载态 -->
    <template v-if="loading && skills.length === 0">
      <div class="empty-state">正在加载同步选项...</div>
    </template>
    <template v-else-if="loadError">
      <div class="empty-state error-text">{{ loadError }}</div>
    </template>
    <template v-else>
      <div class="manager-sync-columns">
        <div class="manager-sync-column">
          <div class="manager-sync-column__header">
            <h3 class="manager-sync-column__title">
              <AppTooltip :tip="`来源路径：${sourceRoot || '加载中...'}`">
                <span class="manager-sync-column__info" aria-hidden="true">
                  <svg focusable="false" viewBox="0 0 24 24">
                    <path
                      d="M11 17h2v-6h-2v6zm1-15C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zM11 9h2V7h-2v2z"
                    />
                  </svg>
                </span>
                Skills ({{ selectedSkillPaths.size }}/{{ skills.length }})
              </AppTooltip>
            </h3>
            <div class="manager-sync-column__actions">
              <button
                v-if="isGitSource"
                class="secondary-button manager-sync-column__action"
                :disabled="!canRefreshSource"
                title="忽略 24 小时自动更新间隔"
                type="button"
                @click="handleRefreshSource"
              >
                {{ refreshingSource ? "拉取中..." : "强制拉取" }}
              </button>
              <button
                class="secondary-button manager-sync-column__action"
                :disabled="allSkillsSelected"
                type="button"
                @click="handleSelectAllSkills"
              >
                全选
              </button>
              <button
                class="secondary-button manager-sync-column__action"
                :disabled="selectedSkillPaths.size === 0"
                type="button"
                @click="handleDeselectAllSkills"
              >
                取消全选
              </button>
            </div>
          </div>
          <input
            v-model="skillSearch"
            class="manager-search-input"
            placeholder="搜索 skills 名字或路径"
            type="text"
          />
          <template v-if="skills.length">
            <div class="manager-import-skill-list">
              <template v-if="visibleMatched.length === 0 && visibleUnmatched.length === 0">
                <div class="empty-state">没有匹配的 skills。</div>
              </template>
              <template v-else>
                <div
                  v-for="skill in visibleMatched"
                  :key="skill.relativePath"
                  class="manager-import-skill-row"
                >
                  <label class="manager-sync-skill-row">
                    <input
                      type="checkbox"
                      :checked="selectedSkillPaths.has(skill.relativePath)"
                      :disabled="confirming"
                      @change="handleToggleSkill(skill.relativePath)"
                    />
                    <span class="manager-skill-name">{{ skill.name }}</span>
                  </label>
                </div>
                <details
                  v-if="visibleUnmatched.length > 0"
                  class="manager-sync-unmatched"
                  :open="showUnmatched"
                  @toggle="showUnmatched = ($event.currentTarget as HTMLDetailsElement).open"
                >
                  <summary>未匹配 {{ unmatchedSkills.length }} 项</summary>
                  <div class="manager-sync-unmatched__list">
                    <div
                      v-for="skill in visibleUnmatched"
                      :key="skill.relativePath"
                      class="manager-import-skill-row"
                    >
                      <label class="manager-sync-skill-row">
                        <input
                          type="checkbox"
                          :checked="selectedSkillPaths.has(skill.relativePath)"
                          :disabled="confirming"
                          @change="handleToggleSkill(skill.relativePath)"
                        />
                        <span class="manager-skill-name">{{ skill.name }}</span>
                      </label>
                    </div>
                  </div>
                </details>
              </template>
            </div>
          </template>
          <div v-else class="empty-state">没有可用的 skills。</div>
        </div>

        <div class="manager-sync-column">
          <div class="manager-sync-column__header">
            <h3 class="manager-sync-column__title">
              Targets ({{ selectedTargetIds.size }}/{{ selectableTargetIds.length }})
            </h3>
            <div class="manager-sync-column__actions">
              <button
                class="secondary-button manager-sync-column__action"
                :disabled="allTargetsSelected"
                type="button"
                @click="handleSelectAllTargets"
              >
              全选
              </button>
              <button
                class="secondary-button manager-sync-column__action"
                :disabled="selectedTargetIds.size === 0"
                type="button"
                @click="handleDeselectAllTargets"
              >
              取消全选
              </button>
            </div>
          </div>
          <template v-if="targets.length">
            <SyncTargetGroups
              :busy="busy"
              :current-source-id="source?.id"
              :source-labels="sourceLabels"
              :source-root="sourceRoot"
              :selected-target-ids="selectedTargetIds"
              :targets="targets"
              @remove-link="handleRemoveLink"
              @set-targets="handleSetTargets"
              @toggle-target="handleToggleTarget"
            />
          </template>
          <div v-else class="empty-state">没有可用的 targets。</div>
        </div>
      </div>
    </template>
  </DialogShell>
</template>
