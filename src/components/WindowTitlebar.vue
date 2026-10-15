<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { createWindowControls } from '../window/windowControls';

const props = defineProps<{ view: string }>();
const controls = createWindowControls();
const maximized = ref(false);
const failed = ref('');
let stopResize: (() => void) | undefined;
const canMaximize = props.view === 'main';
const title = props.view === 'login' ? '添加 Telegram 账号' : props.view === 'settings' || props.view === 'account' ? 'TMsgPicker · 设置' : 'TMsgPicker';

async function refresh() {
  if (canMaximize) maximized.value = await controls.isMaximized();
}
async function action(fn: () => Promise<void>) {
  try { failed.value = ''; await fn(); }
  catch (error) { failed.value = String(error); }
}
function doubleClick(event: MouseEvent) {
  if (!canMaximize || (event.target as HTMLElement).closest('button')) return;
  void action(async () => { await controls.toggleMaximize(); await refresh(); });
}
onMounted(async () => {
  try {
    await refresh();
    if (canMaximize) stopResize = await controls.onResized(() => { void refresh().catch(() => {}); });
  } catch (error) { failed.value = String(error); }
});
onUnmounted(() => stopResize?.());
</script>
<template>
<header class="window-titlebar" data-tauri-drag-region @dblclick="doubleClick">
  <div class="window-titlebar-identity" data-tauri-drag-region>
    <span class="window-titlebar-mark" aria-hidden="true">✦</span>
    <span class="window-titlebar-label" data-tauri-drag-region>{{ title }}</span>
  </div>
  <span v-if="failed" class="window-titlebar-error" role="alert" :title="failed">窗口操作失败</span>
  <div class="window-titlebar-actions" role="group" aria-label="窗口操作">
    <button type="button" aria-label="最小化" title="最小化" @click="action(() => controls.minimize())"><span class="chrome-icon chrome-minimize" aria-hidden="true"/></button>
    <button v-if="canMaximize" type="button" :aria-label="maximized ? '还原' : '最大化'" :title="maximized ? '还原' : '最大化'" @click="action(async () => { await controls.toggleMaximize(); await refresh(); })"><span :class="maximized ? 'chrome-icon chrome-restore' : 'chrome-icon chrome-maximize'" aria-hidden="true"/></button>
    <button type="button" class="window-titlebar-close" aria-label="关闭窗口" title="关闭窗口" @click="action(() => controls.close())"><span class="chrome-icon chrome-close" aria-hidden="true"/></button>
  </div>
</header>
</template>
