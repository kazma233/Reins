<script setup lang="ts">
import { computed } from "vue";
import type { SkillLinkAssociation, SyncTargetOption } from "../types";
import {
  canRemoveSkillLink,
  pendingSkillLinkCleanupCount,
  hasCurrentSourceSkillLinks,
  hasUnmanagedSkillLinks,
  isSelectableSyncTarget,
  skillDirIsAlias,
  skillDirOwnerLabel,
  skillLinkSourceLabel,
  skillLinkSourcePath,
  skillLinkStateInfo,
} from "../syncAssociations";
import { groupSyncTargets } from "../syncTargetGroups";

type SyncTargetGroupsProps = {
  busy: boolean;
  currentSourceId?: string;
  sourceLabels?: Record<string, string>;
  sourceRoot?: string;
  selectedTargetIds: Set<string>;
  targets: SyncTargetOption[];
};

const props = defineProps<SyncTargetGroupsProps>();

defineEmits<{
  setTargets: [targetIds: string[], selected: boolean];
  toggleTarget: [id: string];
  removeLink: [targetId: string, destinationPath: string];
}>();

function getSelectableTargetIds(targets: SyncTargetOption[]): string[] {
  return targets.filter(isSelectableSyncTarget).map((t) => t.id);
}

function isCurrentSourceLink(link: SkillLinkAssociation): boolean {
  return Boolean(props.currentSourceId && link.matchedSourceIds.includes(props.currentSourceId));
}

const groups = computed(() => groupSyncTargets(props.targets));
</script>

<template>
  <div class="manager-sync-target-groups">
    <div v-for="group in groups" :key="group.id" class="manager-sync-target-group">
      <div class="manager-sync-target-group__header">
        <span>{{ group.label }}</span>
        <span class="manager-sync-target-group__count">
          {{ getSelectableTargetIds(group.targets).filter((id) => selectedTargetIds.has(id)).length }}/{{ getSelectableTargetIds(group.targets).length }}
        </span>
        <div class="manager-sync-target-group__actions">
          <button
            class="secondary-button manager-sync-target-group__action"
            :disabled="busy || getSelectableTargetIds(group.targets).length === 0 || getSelectableTargetIds(group.targets).every((id) => selectedTargetIds.has(id))"
            type="button"
            @click="$emit('setTargets', getSelectableTargetIds(group.targets), true)"
          >
            全选
          </button>
          <button
            class="secondary-button manager-sync-target-group__action"
            :disabled="busy || getSelectableTargetIds(group.targets).every((id) => !selectedTargetIds.has(id))"
            type="button"
            @click="$emit('setTargets', getSelectableTargetIds(group.targets), false)"
          >
            取消
          </button>
        </div>
      </div>
      <div
        v-for="target in group.targets"
        :key="target.id"
        class="manager-sync-target-entry"
      >
        <label
          :class="`manager-sync-target-row${busy || skillDirIsAlias(target) ? ' manager-sync-target-row--disabled' : ''}`"
        >
          <input
            type="checkbox"
            :checked="selectedTargetIds.has(target.id)"
            :disabled="busy || skillDirIsAlias(target)"
            @change="$emit('toggleTarget', target.id)"
          />
          <span class="manager-sync-target-row__body">
            <span class="manager-sync-target-row__label-line">
              <span class="manager-sync-target-row__label">{{ target.label }}</span>
              <span class="manager-sync-target-row__tags">
                <span
                  v-if="hasUnmanagedSkillLinks(target.links)"
                  class="manager-sync-target-row__tag manager-sync-target-row__tag--unmanaged"
                >
                  非受管控
                </span>
                <span
                  v-if="hasCurrentSourceSkillLinks(target.links, currentSourceId)"
                  class="manager-sync-target-row__tag manager-sync-target-row__tag--linked"
                >
                  关联
                </span>
              </span>
            </span>
            <span class="manager-sync-target-row__path">{{ target.skillDir }}</span>
            <span v-if="skillDirOwnerLabel(target)" class="manager-sync-target-row__link">
              {{ skillDirOwnerLabel(target) }}
            </span>
            <!-- 链接指向的原始目的地：让用户知道这个别名到底挂在哪儿。 -->
            <span v-if="target.skillDirLinkTarget" class="manager-sync-target-row__link">
              {{ target.skillDirLinkTarget }}
            </span>
          </span>
        </label>

        <details v-if="target.links.length > 0" class="manager-sync-target-links">
          <summary>
            已关联 {{ target.links.length }} 项
            <span v-if="pendingSkillLinkCleanupCount(target.links) > 0" class="manager-sync-target-links__cleanup">
              待清理 {{ pendingSkillLinkCleanupCount(target.links) }} 项
            </span>
          </summary>
          <div class="manager-sync-target-links__list">
            <div
              v-for="link in target.links"
              :key="link.destinationPath"
              :class="[
                'manager-sync-target-link',
                { 'manager-sync-target-link--current-source': isCurrentSourceLink(link) },
              ]"
            >
              <span class="manager-sync-target-link__main">
                <span class="manager-sync-target-link__name">{{ link.skillName }}</span>
                <span
                  v-if="link.matchedSourceIds.length > 0"
                  class="manager-sync-target-link__source"
                >{{ skillLinkSourceLabel(link, currentSourceId, sourceLabels) }}</span>
                <span class="manager-sync-target-link__path" :title="link.sourcePath">
                  {{ skillLinkSourcePath(link.sourcePath, sourceRoot) }}
                </span>
              </span>
              <span :class="`manager-sync-target-link__state manager-sync-target-link__state--${skillLinkStateInfo(link.state).tone}`">
                {{ skillLinkStateInfo(link.state).label }}
              </span>
              <button
                v-if="canRemoveSkillLink(link)"
                :aria-label="`移除 ${link.skillName} 的软链接`"
                :disabled="busy"
                class="manager-sync-target-link__remove"
                type="button"
                @click="$emit('removeLink', target.id, link.destinationPath)"
              >
                <svg aria-hidden="true" focusable="false" viewBox="0 0 24 24">
                  <path
                    d="M6.7 5.3 12 10.6l5.3-5.3 1.4 1.4-5.3 5.3 5.3 5.3-1.4 1.4-5.3-5.3-5.3 5.3-1.4-1.4 5.3-5.3-5.3-5.3z"
                  />
                </svg>
              </button>
            </div>
          </div>
        </details>
      </div>
    </div>
  </div>
</template>
