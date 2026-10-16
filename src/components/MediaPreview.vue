<script setup lang="ts">
import {ref,computed,watch,onUnmounted} from 'vue';
import UiIcon from './UiIcon.vue';
import {requestMedia} from '../media/mediaRequests.ts';
import type {Message} from '../types';

const props=defineProps<{member:Message;profileId:string;generation:number}>();
const hovered=ref(false);
let hoverTimer:ReturnType<typeof setTimeout>|undefined;
function enter(){clearTimeout(hoverTimer);hoverTimer=setTimeout(()=>hovered.value=true,420);}
function leave(){clearTimeout(hoverTimer);hovered.value=false;}
const compactFormatter=new Intl.NumberFormat('zh-CN',{notation:'compact',maximumFractionDigits:1});
const formatted=(value:number|null)=>value===null?'—':compactFormatter.format(value);

const src=ref(''),loading=ref(false),loadingFull=ref(false),fullLoaded=ref(false),error=ref('');
let revision=0;
let controller:AbortController|undefined;
const photo=computed(()=>props.member.content_kind==='photo');
const thumbnailId=computed(()=>props.member.thumbnail_file_id);
const fallback=computed(()=>{
  const image=props.member.minithumbnail??'';
  return /^data:image\/(jpeg|png|webp|gif);base64,/.test(image)?image:'';
});
async function load(){
  controller?.abort();
  const abort=new AbortController();
  controller=abort;
  const token=++revision,id=thumbnailId.value;
  src.value=fallback.value;error.value='';loadingFull.value=false;fullLoaded.value=false;
  if(!id){loading.value=false;return;}
  loading.value=true;
  try{
    // Never fetch the full-resolution photo automatically when an Album opens.
    // The small TDLib thumbnail, and then its minithumbnail fallback, are
    // sufficient for a grid; full files require explicit user interaction.
    const uri=await requestMedia(props.profileId,props.generation,id,false,
      {signal:abort.signal,priority:'thumbnail'});
    if(token!==revision||abort.signal.aborted)return;
    if(uri?.startsWith('data:image/')||(!photo.value&&uri?.startsWith('data:video/mp4')))src.value=uri;
    else if(!src.value)error.value='预览不可用';
  }catch(e){if(token===revision&&!abort.signal.aborted&&!src.value)error.value=String(e);}
  finally{if(token===revision)loading.value=false;}
}
async function loadFull(){
  const id=props.member.full_file_id;
  if(!photo.value||!id||loadingFull.value||fullLoaded.value)return;
  controller?.abort();
  const abort=new AbortController();
  controller=abort;
  const token=++revision;
  loadingFull.value=true;loading.value=false;error.value='';
  try{
    const uri=await requestMedia(props.profileId,props.generation,id,true,
      {signal:abort.signal,priority:'full'});
    if(token!==revision||abort.signal.aborted)return;
    if(uri?.startsWith('data:image/')){src.value=uri;fullLoaded.value=true;}
    else error.value='原图不可用';
  }catch(e){if(token===revision&&!abort.signal.aborted)error.value=String(e);}
  finally{if(token===revision)loadingFull.value=false;}
}
watch([thumbnailId,()=>props.member.full_file_id,fallback,()=>props.profileId,()=>props.generation],
  ()=>void load(),{immediate:true});
onUnmounted(()=>{++revision;controller?.abort();clearTimeout(hoverTimer);});
</script>
<template><div class="media-preview" :class="{'media-preview-photo':photo}" @mouseenter="enter" @mouseleave="leave">
<div v-if="photo" class="media-preview-open">
  <img v-if="src" :src="src" alt="图片预览" loading="lazy" decoding="async"/>
  <span v-else>{{loading?'加载中…':error||'图片'}}</span>
</div>
<div v-else class="media-video-cover">
  <img v-if="src.startsWith('data:image/')" :src="src" alt="视频封面" loading="lazy" decoding="async"/>
  <video v-else-if="src.startsWith('data:video/mp4')" :src="src" muted playsinline preload="metadata" @loadedmetadata="($event.target as HTMLVideoElement).currentTime=0.05"/>
  <span v-else>{{loading?'加载封面…':error||'视频'}}</span><span class="video-kind-tag">视频</span>
</div>
<button v-if="photo&&member.full_file_id" class="media-full-control" type="button"
  :disabled="loadingFull||fullLoaded" :title="error||'按需加载原图（可能较大）'" @click.stop="loadFull">
  {{loadingFull?'加载原图…':fullLoaded?'原图已加载':'查看原图'}}
</button>
<div class="member-metric-overlay" :class="{'is-visible':hovered}" aria-label="当前媒体成员的浏览与转发">
 <span><UiIcon name="eye"/>{{formatted(member.metrics.views)}}</span>
 <span><UiIcon name="forward"/>{{formatted(member.metrics.forwards)}}</span>
</div></div></template>
