<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { getSyncSkillOptions, getSyncTargetOptions } from "../../api";
import { extractErrorMessage } from "@shared/lib/errors";
import DialogShell from "@shared/ui/DialogShell.vue";
import SyncTargetGroups from "../SyncTargetGroups.vue";
import type { SkillSourceConfigView, SyncSkillOption, SyncTargetOption } from "../../types";

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

// Self-managed state: target/skill options are loaded by the dialog itself
// on open, kept locally while open, and reset on close. Parent only forwards
// the open flag, source, and parent-wide loading state (for conflict/overwrite
// confirmation dialogs running in parallel).
const loading = ref(false);
const confirming = ref(false);
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

watch(
  () => [props.open, props.source?.id],
  ([open, _sourceId], _old, onCleanup) => {
    if (!open || !props.source) return;
    let cancelled = false;
    loading.value = true;
    loadError.value = null;

    Promise.all([getSyncTargetOptions(), getSyncSkillOptions(props.source.id)])
      .then(([targetOptions, skillResult]) => {
        if (cancelled) return;
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
        selectedTargetIds.value = new Set();
        loading.value = false;
      })
      .catch((error) => {
        if (cancelled) return;
        targets.value = [];
        skills.value = [];
        sourceRoot.value = "";
        selectedSkillPaths.value = new Set();
        selectedTargetIds.value = new Set();
        loadError.value = extractErrorMessage(error, "读取同步选项失败。");
        loading.value = false;
      });

    onCleanup(() => {
      cancelled = true;
    });
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
const busy = computed(() => loading.value || confirming.value || props.loading);
const canConfirm = computed(
  () => selectedSkillPaths.value.size > 0 && selectedTargetIds.value.size > 0 && !busy.value && !loadError.value,
);

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
      <button class="secondary-button" :disabled="busy" type="button" @click="$emit('close')">
        取消
      </button>
      <button class="primary-button" :disabled="!canConfirm" type="button" @click="handleConfirm">
        {{ confirming ? "同步中..." : "开始同步" }}
      </button>
    </template>

    <p v-if="source" class="manager-sync-source-location">
      {{ sourceRoot || "加载中..." }}
    </p>

    <template v-if="loading">
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
              Skills ({{ selectedSkillPaths.size }}/{{ skills.length }})
            </h3>
            <div class="manager-sync-column__actions">
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
              :busy="confirming"
              :current-source-id="source?.id"
              :source-labels="sourceLabels"
              :source-root="sourceRoot"
              :selected-target-ids="selectedTargetIds"
              :targets="targets"
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
