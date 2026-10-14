<script setup lang="ts">
import { provide } from 'vue';
import {useWorkspace,workspaceKey} from './useWorkspace';
import ApplicationSettings from './components/ApplicationSettings.vue';
import AccountSettings from './components/AccountSettings.vue';
import ConfirmDialog from './components/ConfirmDialog.vue';
const accountView=new URLSearchParams(window.location.search).get('view')==='account';
const workspace=useWorkspace(true);provide(workspaceKey,workspace);
</script>
<template><AccountSettings v-if="accountView"/><ApplicationSettings v-else/><ConfirmDialog/></template>
