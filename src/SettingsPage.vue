<script setup lang="ts">
import { provide } from 'vue';
import {useWorkspace,workspaceKey} from './useWorkspace';
import ApplicationSettings from './components/ApplicationSettings.vue';
import AccountSettings from './components/AccountSettings.vue';
import AppearanceSettings from './components/AppearanceSettings.vue';
import ConfirmDialog from './components/ConfirmDialog.vue';
import {useSettingsNavigation} from './useSettingsNavigation';
const workspace=useWorkspace(true);provide(workspaceKey,workspace);
const {tab,navigate}=useSettingsNavigation(workspace);
</script>
<template>
<main class="settings-page">
  <nav class="settings-tabs segmented-tabs" :style="{'--segment-index':tab==='application'?0:tab==='account'?1:2}" aria-label="设置子页面">
    <button :aria-pressed="tab==='application'" :disabled="workspace.busy.value||workspace.resetBusy.value||!workspace.settingsTargetReady.value" @click="navigate({tab:'application'})">应用设置</button>
    <button :aria-pressed="tab==='account'" :disabled="workspace.busy.value||workspace.resetBusy.value||!workspace.settingsTargetReady.value" @click="navigate({tab:'account'})">账号设置</button>
    <button :aria-pressed="tab==='appearance'" :disabled="workspace.busy.value||workspace.resetBusy.value||!workspace.settingsTargetReady.value" @click="navigate({tab:'appearance'})">外观设置</button>
  </nav>
  <!-- Keep every settings view mounted, so unsaved inputs and scroll positions survive switches. -->
  <div class="settings-panels">
    <Transition name="selection-fade"><ApplicationSettings v-show="tab==='application'"/></Transition>
    <Transition name="selection-fade"><AccountSettings v-show="tab==='account'"/></Transition>
    <Transition name="selection-fade"><AppearanceSettings v-show="tab==='appearance'"/></Transition>
  </div>
  <ConfirmDialog/>
</main>
</template>
