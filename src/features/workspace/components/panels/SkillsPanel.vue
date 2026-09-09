<script setup lang="ts">
import { computed, ref } from "vue";
import { refDebounced } from "@vueuse/core";
import SkillSourceGroup from "../SkillSourceGroup.vue";
import SkillImportDialog from "../dialogs/SkillImportDialog.vue";
import SkillSourceEditDialog from "../dialogs/SkillSourceEditDialog.vue";
import SourceSyncDialog from "../dialogs/SourceSyncDialog.vue";
import RemoveSyncDialog from "../dialogs/RemoveSyncDialog.vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import { formatTargetLabel } from "../../model";
import { buildSkillLinkSourceLabels } from "../../syncAssociations";
import { useWorkspaceState } from "../../composables/useWorkspaceState";
import { useSkillImport } from "../../composables/useSkillImport";
import { useSkillSourceEdit } from "../../composables/useSkillSourceEdit";
import { useSourceSync } from "../../composables/useSourceSync";

// --- workspace state (store-backed, safe to call anywhere) ---

const { configDocument, runningAction } = useWorkspaceState();

const configSkillSources = computed(
  () => configDocument.value?.config?.skillSources ?? [],
);
const sourceLabels = computed(() => buildSkillLinkSourceLabels(configSkillSources.value));

// --- skills domain composables ---

const {
  skillImportDialog,
  openImportDialog,
  closeImportDialog,
  scanGitSkills,
  selectLocalDirectory,
  handleImportSkills,
} = useSkillImport();

const {
  skillSourceEditDialog,
  skillSourceDeleteDialog,
  openSkillSourceEditDialog,
  closeSkillSourceEditDialog,
  handleRefreshSkillSourceEditPreview,
  handleConfirmSkillSourceEdit,
  handleRefreshGitSkillSource,
  openSkillSourceDeleteDialog,
  closeSkillSourceDeleteDialog,
  handleConfirmDeleteSkillSource,
} = useSkillSourceEdit();

const {
  sourceSyncDialog,
  sourceSyncOverwriteDialog,
  removeSyncDialog,
  handleOpenSourceSync,
  handleConfirmSourceSync,
  closeSourceSyncDialog,
  closeSourceSyncOverwriteDialog,
  confirmSourceSyncOverwrite,
  handleOpenSourceRemoveSync,
  closeRemoveSyncDialog,
  handleToggleRemoveSyncTarget,
  handleSetRemoveSyncTargets,
  handleConfirmRemoveSourceSync,
} = useSourceSync();

// --- local filter state ---

const skillNameFilter = ref("");
const debouncedSkillNameFilter = refDebounced(skillNameFilter, 200);
const normalizedSkillNameFilter = computed(() =>
  debouncedSkillNameFilter.value.trim().toLocaleLowerCase(),
);

const visibleSources = computed(() => {
  if (!normalizedSkillNameFilter.value) return configSkillSources.value;
  return configSkillSources.value.filter((source) =>
    source.label.toLocaleLowerCase().includes(normalizedSkillNameFilter.value),
  );
});

const localSources = computed(() =>
  visibleSources.value.filter((source) => source.type === "local"),
);
const remoteSources = computed(() =>
  visibleSources.value.filter((source) => source.type === "git"),
);
</script>

<template>
  <Teleport defer to="#workspace-panel-actions">
    <input
      id="manager-skill-name-filter"
      v-model="skillNameFilter"
      class="manager-skills-toolbar__search-input"
      placeholder="按来源搜索..."
      type="text"
    />
    <button
      class="primary-button"
      :disabled="runningAction"
      type="button"
      @click="openImportDialog"
    >
      新增来源
    </button>
  </Teleport>

  <section class="manager-stack manager-stack--stretch">
    <article class="manager-panel manager-panel--fill">
      <div class="manager-panel-section manager-panel-section--fill">
        <div class="manager-scroll-region">
          <template v-if="visibleSources.length">
            <div class="manager-skill-source-groups">
              <SkillSourceGroup
                heading="本地 Skills"
                heading-id="local-skills-heading"
                :loading="runningAction"
                :sources="localSources"
                @edit="openSkillSourceEditDialog"
                @delete="openSkillSourceDeleteDialog"
                @sync="handleOpenSourceSync"
                @remove-sync="handleOpenSourceRemoveSync"
                @refresh="handleRefreshGitSkillSource"
              />
              <SkillSourceGroup
                heading="远端 Skills"
                heading-id="remote-skills-heading"
                :loading="runningAction"
                :sources="remoteSources"
                @edit="openSkillSourceEditDialog"
                @delete="openSkillSourceDeleteDialog"
                @sync="handleOpenSourceSync"
                @remove-sync="handleOpenSourceRemoveSync"
                @refresh="handleRefreshGitSkillSource"
              />
            </div>
          </template>
          <div v-else class="empty-state">当前没有匹配的来源。</div>
        </div>
      </div>
    </article>

    <!-- --- skill import --- -->

    <SkillImportDialog
      :dialog-state="skillImportDialog"
      :loading="runningAction"
      @close="closeImportDialog"
      @confirm="handleImportSkills"
      @discover-git-skills="scanGitSkills"
      @pick-local-skill-directory="selectLocalDirectory"
    />

    <!-- --- skill source edit / delete --- -->

    <SkillSourceEditDialog
      :state="skillSourceEditDialog"
      :loading="runningAction"
      @close="closeSkillSourceEditDialog"
      @confirm="handleConfirmSkillSourceEdit"
      @refresh-preview="handleRefreshSkillSourceEditPreview"
    />

    <ConfirmDialog
      :open="skillSourceDeleteDialog.open"
      dialog-class-name="manager-import-dialog"
      eyebrow="Source"
      :title="`删除来源: ${skillSourceDeleteDialog.source?.label ?? ''}`"
      title-id="skill-source-delete-dialog-title"
      confirm-button-class-name="danger-button"
      :confirm-label="runningAction ? '删除中...' : '确认删除'"
      :loading="runningAction"
      @close="closeSkillSourceDeleteDialog"
      @confirm="handleConfirmDeleteSkillSource"
    >
      <p class="manager-skill-description">
        删除后会移除 target 中指向该来源的软链接，但不会删除来源目录中的原始 skills。
      </p>
    </ConfirmDialog>

    <!-- --- source sync --- -->

    <SourceSyncDialog
      :open="sourceSyncDialog.open"
      :source="sourceSyncDialog.source"
      :source-labels="sourceLabels"
      :loading="sourceSyncDialog.loading"
      @close="closeSourceSyncDialog"
      @confirm="handleConfirmSourceSync"
    />

    <ConfirmDialog
      :open="sourceSyncOverwriteDialog.open"
      dialog-class-name="manager-import-dialog"
      eyebrow="Sync"
      title="覆盖已有软链接"
      title-id="source-sync-overwrite-dialog-title"
      cancel-label="取消"
      confirm-button-class-name="danger-button"
      :confirm-label="sourceSyncOverwriteDialog.loading ? '覆盖中...' : '覆盖并同步'"
      :loading="sourceSyncOverwriteDialog.loading"
      @close="closeSourceSyncOverwriteDialog"
      @confirm="confirmSourceSyncOverwrite"
    >
      <p class="manager-skill-description">
        以下目标已存在软链接，覆盖会先移除旧链接再写入新链接。
      </p>
      <div class="manager-delete-confirm-list">
        <div
          v-for="(conflict, index) in sourceSyncOverwriteDialog.conflicts"
          :key="`${conflict.skillName}-${conflict.targetId}-${index}`"
          class="manager-import-skill-row manager-import-skill-row--readonly"
        >
          <div class="manager-skill-headline">
            <div class="manager-skill-headline__meta">
              <span class="manager-skill-name">{{ conflict.skillName }}</span>
              <span class="manager-skill-updated">
                {{ formatTargetLabel(conflict.targetId) }}
              </span>
            </div>
            <span class="pill danger-pill">{{ conflict.existingKind }}</span>
          </div>
          <p class="manager-skill-description">{{ conflict.detail }}</p>
        </div>
      </div>
    </ConfirmDialog>

    <RemoveSyncDialog
      :open="removeSyncDialog.open"
      :loading="removeSyncDialog.loading || runningAction"
      :source-id="removeSyncDialog.source?.id ?? ''"
      :source-labels="sourceLabels"
      :targets-loading="removeSyncDialog.targetsLoading"
      :source-label="removeSyncDialog.source?.label ?? ''"
      :targets="removeSyncDialog.targets"
      :selected-target-ids="removeSyncDialog.selectedTargetIds"
      @close="closeRemoveSyncDialog"
      @confirm="handleConfirmRemoveSourceSync"
      @set-targets="handleSetRemoveSyncTargets"
      @toggle-target="handleToggleRemoveSyncTarget"
    />
  </section>
</template>
