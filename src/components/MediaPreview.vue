<script setup lang="ts">
import {computed,ref,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {requestAvatar,avatarKey,cachedAvatar} from '../media/avatarCache';
import type {Message} from '../types';
const props=defineProps<{member:Message;profileId:string;generation:number}>();
const emit=defineEmits<{browse:[]}>();
const busy=ref(false);
const key=computed(()=>avatarKey(props.profileId,props.generation,props.member.thumbnail_file_id??undefined));
const url=computed(()=>cachedAvatar(key.value)||props.member.minithumbnail||'');
async function load(){if(!props.member.thumbnail_file_id||busy.value)return;busy.value=true;try{
 await requestAvatar(props.profileId,props.generation,props.member.thumbnail_file_id);
}finally{busy.value=false;}}
watch(()=>props.member.thumbnail_file_id,()=>void load(),{immediate:true});
const kind=computed(()=>props.member.content_kind);
</script>
<template><div class="media-preview">
 <button type="button" class="media-preview-open" aria-label="打开媒体浏览器" @click="emit('browse')">
  <img v-if="url?.startsWith('data:image/')" :src="url" :alt="kind+'预览'" loading="lazy"/>
  <video v-else-if="url?.startsWith('data:video/mp4')" :src="url" muted playsinline preload="metadata"/>
  <span v-else class="media-placeholder">{{kind==='photo'?'图片':kind==='video'?'视频':kind}}</span>
  <span class="media-open-hint">查看大图 / 媒体</span>
 </button>
</div></template>