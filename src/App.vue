<script setup lang="ts">
import { provide } from 'vue';
import { useWorkspace, workspaceKey } from './useWorkspace';
import WorkspaceLayout from './components/WorkspaceLayout.vue';
import ConfirmDialog from './components/ConfirmDialog.vue';
import AccountPanel from './components/AccountPanel.vue';
import ChatPanel from './components/ChatPanel.vue';
import QueryPanel from './components/QueryPanel.vue';
import MessageResultsPanel from './components/MessageResultsPanel.vue';
const w=useWorkspace();provide(workspaceKey,w);
</script>
<template><WorkspaceLayout :left="w.leftRatio.value" :right="w.rightRatio.value" @resize="w.resizeLayout" @reset="w.resetLayout"><template #left><AccountPanel/><ChatPanel v-if="w.snapshot.value?.profiles.length"/></template><template #middle><MessageResultsPanel/></template><template #right><QueryPanel/></template></WorkspaceLayout><ConfirmDialog/></template>
