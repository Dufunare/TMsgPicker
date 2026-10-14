<script setup lang="ts">
import { provide } from 'vue';
import {useWorkspace,workspaceKey} from './useWorkspace';
import AccountSettings from './components/AccountSettings.vue';
import ConfirmDialog from './components/ConfirmDialog.vue';
const workspace=useWorkspace(true);provide(workspaceKey,workspace);
</script>
<template><AccountSettings/><ConfirmDialog/></template>
