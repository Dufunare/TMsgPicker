<script setup lang="ts">
import { isTauri } from '@tauri-apps/api/core';
import type { Component } from 'vue';
import WindowTitlebar from './WindowTitlebar.vue';

defineProps<{ page: Component; view: string }>();
const nativeWindow = isTauri();
</script>
<template>
  <div class="window-shell">
    <WindowTitlebar v-if="nativeWindow" :view="view"/>
    <div class="window-body"><component :is="page"/></div>
  </div>
</template>
