<script setup lang="ts">
import { computed } from "vue";
import type { SkillLinkAssociation, SyncTargetOption } from "../types";
import {
  pendingSkillLinkCleanupCount,
  hasCurrentSourceSkillLinks,
  hasUnmanagedSkillLinks,
  skillLinkSourceLabel,
  skillLinkSourcePath,
  skillLinkStateInfo,
} from "../syncAssociations";

type SyncTargetGroup = {
  id: string;
  label: string;
  targets: SyncTargetOption[];
};

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
}>();

function getSelectableTargetIds(targets: SyncTargetOption[]): string[] {
  return targets.filter((t) => t.enabled && !t.linkedTargetId).map((t) => t.id);
}

function isCurrentSourceLink(link: SkillLinkAssociation): boolean {
  return Boolean(props.currentSourceId && link.matchedSourceIds.includes(props.currentSourceId));
}

const groups = computed<SyncTargetGroup[]>(() => {
  const targets = props.targets;
  const globalTargets = targets.filter((target) => !target.id.includes(":"));
  const projectGroups = new Map<string, SyncTargetOption[]>();
  for (const target of targets) {
    const colonPos = target.id.indexOf(":");
    if (colonPos < 0) continue;
    const projectId = target.id.substring(0, colonPos);
    const existing = projectGroups.get(projectId);
    if (existing) existing.push(target);
    else projectGroups.set(projectId, [target]);
  }
  const result: SyncTargetGroup[] = [];
  if (globalTargets.length > 0) {
    result.push({ id: "global", label: "全局目标", targets: globalTargets });
  }
  for (const [projectId, projectTargets] of projectGroups) {
    result.push({ id: `project:${projectId}`, label: `项目 ${projectId}`, targets: projectTargets });
  }
  return result;
});
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
          :class="`manager-sync-target-row${busy || !target.enabled || target.linkedTargetId ? ' manager-sync-target-row--disabled' : ''}`"
        >
          <input
            type="checkbox"
            :checked="selectedTargetIds.has(target.id)"
            :disabled="busy || !target.enabled || Boolean(target.linkedTargetId)"
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
            <span v-if="target.linkedTargetId" class="manager-sync-target-row__link">
              继承自 {{ target.linkedTargetId }}
            </span>
          </span>
          <span :class="`manager-sync-target-row__pill ${target.enabled ? 'pill--on' : 'pill--off'}`">
            {{ target.enabled ? "启用" : "禁用" }}
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
            </div>
          </div>
        </details>
      </div>
    </div>
  </div>
</template>
