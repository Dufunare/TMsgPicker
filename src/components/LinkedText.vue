<script setup lang="ts">
import {computed} from 'vue';
import {isTauri} from '@tauri-apps/api/core';
import {openUrl} from '@tauri-apps/plugin-opener';
import {linkedTextParts, safeExternalUrl} from '../text/linkedText';
import type {TextLink} from '../types';

const props=defineProps<{text:string;links?:TextLink[]}>();
const parts=computed(()=>linkedTextParts(props.text,props.links));

async function open(url:string) {
  const safe=safeExternalUrl(url);
  if (!safe) return;
  try { await openUrl(safe); }
  catch { /* Leave the message usable if the OS cannot open a link. */ }
}
function follow(event:MouseEvent,url:string) {
  // Browser preview uses the ordinary safe anchor; desktop opens externally.
  if (!isTauri()) return;
  event.preventDefault();
  void open(url);
}
</script>
<template><span class="linked-text"><template v-for="(part,i) in parts" :key="i"><a v-if="part.url" :href="part.url" rel="noreferrer noopener" target="_blank" @click="follow($event,part.url)">{{part.text}}</a><template v-else>{{part.text}}</template></template></span></template>
