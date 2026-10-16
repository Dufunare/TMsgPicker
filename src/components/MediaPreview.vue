<script setup lang="ts">
import {ref,computed,watch,onUnmounted} from 'vue';
import UiIcon from './UiIcon.vue';
import {cachedMedia,requestMedia} from '../media/mediaRequests.ts';
import type {Message} from '../types';

const props=defineProps<{member:Message;profileId:string;generation:number}>();
const hovered=ref(false);
let hoverTimer:ReturnType<typeof setTimeout>|undefined;
function enter(){clearTimeout(hoverTimer);hoverTimer=setTimeout(()=>hovered.value=true,420);}
function leave(){clearTimeout(hoverTimer);hovered.value=false;}
const compactFormatter=new Intl.NumberFormat('zh-CN',{notation:'compact',maximumFractionDigits:1});
const formatted=(value:number|null)=>value===null?'—':compactFormatter.format(value);

const src=ref(''),loadingPreview=ref(false),loadingFull=ref(false),fullLoaded=ref(false),error=ref('');
let revision=0;
let previewController:AbortController|undefined;
let fullController:AbortController|undefined;
const photo=computed(()=>props.member.content_kind==='photo');
const thumbnailId=computed(()=>props.member.thumbnail_file_id);
const originalId=computed(()=>photo.value?props.member.full_file_id:null);
const fallback=computed(()=>{
  const image=props.member.minithumbnail??'';
  return /^data:image\/(jpeg|png|webp|gif);base64,/.test(image)?image:'';
});

function load(){
  previewController?.abort();
  fullController?.abort();
  const token=++revision;
  const profile=props.profileId,generation=props.generation;
  const thumb=thumbnailId.value,original=originalId.value,isPhoto=photo.value;
  const restored=original?cachedMedia(profile,generation,original,true):null;

  // A reopened Album can restore an original synchronously from the bounded
  // session cache. Cache eviction merely means automatically loading it again.
  src.value=restored?.startsWith('data:image/')?restored:fallback.value;
  fullLoaded.value=!!(restored&&restored.startsWith('data:image/'));
  loadingPreview.value=false;
  loadingFull.value=false;
  error.value='';
  if(fullLoaded.value)return;

  if(thumb){
    const abort=new AbortController();
    previewController=abort;
    loadingPreview.value=true;
    void (async()=>{
      try{
        const uri=await requestMedia(profile,generation,thumb,false,
          {signal:abort.signal,priority:'thumbnail'});
        if(token!==revision||abort.signal.aborted||fullLoaded.value)return;
        if(uri?.startsWith('data:image/')||(!isPhoto&&uri?.startsWith('data:video/mp4')))src.value=uri;
        else if(!src.value&&!original)error.value='预览不可用';
      }catch(e){
        if(token===revision&&!abort.signal.aborted&&!src.value&&!original)error.value=String(e);
      }finally{
        if(token===revision)loadingPreview.value=false;
      }
    })();
  }
  if(original){
    // Expanding the media group itself authorizes the original: no individual
    // "view original" clicks. One per-account native request remains in flight.
    const abort=new AbortController();
    fullController=abort;
    loadingFull.value=true;
    void (async()=>{
      try{
        const uri=await requestMedia(profile,generation,original,true,
          {signal:abort.signal,priority:'full'});
        if(token!==revision||abort.signal.aborted)return;
        if(uri?.startsWith('data:image/')){
          src.value=uri;
          fullLoaded.value=true;
        }else{
          error.value='原图暂不可用，已保留预览';
        }
      }catch(e){
        if(token===revision&&!abort.signal.aborted)error.value='原图加载失败：'+String(e);
      }finally{
        if(token===revision)loadingFull.value=false;
      }
    })();
  }
}
watch([thumbnailId,originalId,fallback,()=>props.profileId,()=>props.generation,photo],
  load,{immediate:true});
onUnmounted(()=>{
  ++revision;
  previewController?.abort();
  fullController?.abort();
  clearTimeout(hoverTimer);
});
</script>
<template><div class="media-preview" :class="{'media-preview-photo':photo}" @mouseenter="enter" @mouseleave="leave">
<div v-if="photo" class="media-preview-open">
  <img v-if="src" :src="src" :alt="fullLoaded?'原图':'图片预览'" loading="lazy" decoding="async"/>
  <span v-else>{{loadingFull?'原图加载中…':loadingPreview?'预览加载中…':error||'图片'}}</span>
</div>
<div v-else class="media-video-cover">
  <img v-if="src.startsWith('data:image/')" :src="src" alt="视频封面" loading="lazy" decoding="async"/>
  <video v-else-if="src.startsWith('data:video/mp4')" :src="src" muted playsinline preload="metadata" @loadedmetadata="($event.target as HTMLVideoElement).currentTime=0.05"/>
  <span v-else>{{loadingPreview?'加载封面…':error||'视频'}}</span><span class="video-kind-tag">视频</span>
</div>
<span v-if="photo&&loadingFull" class="media-resolution-state" role="status">原图加载中…</span>
<span v-else-if="photo&&error" class="media-resolution-state media-resolution-error" :title="error">原图暂不可用</span>
<div class="member-metric-overlay" :class="{'is-visible':hovered}" aria-label="当前媒体成员的浏览与转发">
 <span><UiIcon name="eye"/>{{formatted(member.metrics.views)}}</span>
 <span><UiIcon name="forward"/>{{formatted(member.metrics.forwards)}}</span>
</div></div></template>
