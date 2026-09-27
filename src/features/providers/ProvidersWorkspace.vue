<script setup lang="ts">
import { onMounted } from "vue";
import { storeToRefs } from "pinia";
import AppToast from "@shared/ui/AppToast.vue";
import { PROVIDERS_TAB_COPY } from "./model";
import { useProvidersStore } from "./stores/providers";
import { useProvidersNotice } from "./composables/useProvidersNotice";
import { useProvidersState } from "./composables/useProvidersState";
import ProvidersPanel from "./components/panels/ProvidersPanel.vue";
import AppsPanel from "./components/panels/AppsPanel.vue";
import "./styles.css";

const store = useProvidersStore();
const { providersTab } = storeToRefs(store);
const { notice, clearNotice } = useProvidersNotice();
const { reloadProvidersState } = useProvidersState();

onMounted(() => {
  void reloadProvidersState();
});
</script>

<template>
  <div class="providers-shell">
    <div class="providers-content">
      <header class="providers-content__header">
        <nav class="providers-nav">
          <button
            v-for="tab in PROVIDERS_TAB_COPY"
            :key="tab.id"
            :class="`providers-nav__button${providersTab === tab.id ? ' is-active' : ''}`"
            type="button"
            @click="store.setProvidersTab(tab.id)"
          >
            {{ tab.label }}
          </button>
        </nav>
      </header>

      <ProvidersPanel v-if="providersTab === 'providers'" />
      <AppsPanel v-else />
    </div>

    <AppToast :notice="notice" @close="clearNotice" />
  </div>
</template>
