<script setup lang="ts">
import {computed,ref,watch,onUnmounted} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import type {Message} from '../types';
const props=defineProps<{items:Message[];index:number;profileId:string;generation:number}>();
const emit=defineEmits<{close:[]}>();
const current=ref(props.index),data=ref(''),loading=ref(false),zoom=ref(1),error=ref('');
let revision=0;
const item=computed(()=>props.items[current.value]);
function move(offset:number){current.value=(current.value+offset+props.items.length)%props.items.length;zoom.value=1;}
async function fetchMedia(){
 const member=item.value, token=++revision;data.value='';error.value='';
 if(!member?.full_file_id&&!member?.thumbnail_file_id){error.value='该媒体没有可下载的文件';return;}
 loading.value=true;
 try{
 const result=await invoke<string|null>('photo_thumbnail',{profileId:props.profileId,fileId:member.full_file_id??member.thumbnail_file_id,full:true});
 if(token!==revision)return;
 if(result && /^(data:image\/(jpeg|png|webp|gif)|data:video\/mp4);base64,/.test(result))data.value=result;
 else {error.value='文件尚未下载完成，或超过 12 MiB 安全上限；可在 Telegram 查看原文';}
 }catch(e){if(token===revision)error.value=String(e);}finally{if(token===revision)loading.value=false;}
}
watch([current,()=>props.generation],()=>void fetchMedia(),{immediate:true});
function keys(e:KeyboardEvent){if(e.key==='Escape')emit('close');if(e.key==='ArrowRight')move(1);if(e.key==='ArrowLeft')move(-1);}
onUnmounted(()=>revision++);
</script>
<template><div class="media-lightbox" role="dialog" aria-modal="true" aria-label="媒体浏览" tabindex="0" @keydown="keys" @click.self="emit('close')">
 <header><span>{{current+1}} / {{items.length}}</span><div class="buttons"><button type="button" @click="zoom=Math.max(.5,zoom-.25)">－</button><button type="button" @click="zoom=Math.min(3,zoom+.25)">＋</button><button type="button" @click="emit('close')">关闭 ×</button></div></header>
 <div class="media-lightbox-body">
  <button type="button" :disabled="items.length<2" aria-label="上一个" @click="move(-1)">‹</button>
  <div class="media-lightbox-content"><p v-if="loading">正在加载媒体…</p><p v-else-if="error">{{error}}</p>
   <img v-else-if="data.startsWith('data:image/')" :src="data" :style="{transform:'scale('+zoom+')'}" alt="媒体大图"/>
   <video v-else-if="data.startsWith('data:video/mp4')" :src="data" controls playsinline preload="metadata"/>
  </div>
  <button type="button" :disabled="items.length<2" aria-label="下一个" @click="move(1)">›</button>
 </div></div></template>