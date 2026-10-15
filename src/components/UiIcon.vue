<script setup lang="ts">
defineProps<{name:'plus'|'chevron'|'more'|'settings'|'collapse'|'expand'|'refresh'|'search'|'link'|'eye'|'forward'}>();
const paths={plus:'M12 5v14M5 12h14',chevron:'m6 9 6 6 6-6',more:'M5 12h.01M12 12h.01M19 12h.01',settings:'M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1Z M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0',collapse:'m7 14 5-5 5 5',expand:'m7 10 5 5 5-5',refresh:'M20 7v5h-5M4 17v-5h5M6 7a7 7 0 0 1 12-1l2 6M4 12l2 6a7 7 0 0 0 12-1',search:'M15.5 15.5 21 21M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',link:'m10 14 4-4M8 16l-1 1a4 4 0 0 1-6-6l4-4a4 4 0 0 1 6 0M16 8l1-1a4 4 0 0 1 6 6l-4 4a4 4 0 0 1-6 0',eye:'M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12ZM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6',forward:'M14 5l7 7-7 7v-4c-6 0-9 2-11 5 0-8 4-12 11-12V5Z'};
</script>
<template><svg viewBox="0 0 24 24" class="ui-icon" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path :d="paths[name]"/></svg></template>
<style>.ui-icon{width:16px;height:16px;flex-shrink:0}</style>
