<script setup lang="ts">
import {ref,computed,watch,onUnmounted} from 'vue';
import UiIcon from './UiIcon.vue';
import {requestMedia} from '../media/mediaRequests.ts';
import type {Message} from '../types';
const props=defineProps<{member:Message;profileId:string;generation:number}>();
const hovered=ref(false);let hoverTimer:ReturnType<typeof setTimeout>|undefined;
function enter(){clearTimeout(hoverTimer);hoverTimer=setTimeout(()=>hovered.value=true,420);}
function leave(){clearTimeout(hoverTimer);hovered.value=false;}
const formatted=(value:number|null)=>value===null?'—':Intl.NumberFormat('zh-CN',{notation:'compact',maximumFractionDigits:1}).format(value);
const src=ref(''),loading=ref(false),error=ref('');let revision=0;
const photo=computed(()=>props.member.content_kind==='photo');
const fileId=computed(()=>photo.value?props.member.full_file_id??props.member.thumbnail_file_id:props.member.thumbnail_file_id);
async function load(){const id=fileId.value,token=++revision;src.value='';error.value='';if(!id)return;loading.value=true;
try{const uri=await requestMedia(props.profileId,props.generation,id,photo.value);
if(token!==revision)return;if(uri?.startsWith('data:image/')||(!photo.value&&uri?.startsWith('data:video/mp4')))src.value=uri;else error.value='预览不可用';}
catch(e){if(token===revision)error.value=String(e);}finally{if(token===revision)loading.value=false;}}
watch([fileId,()=>props.profileId,()=>props.generation],()=>void load(),{immediate:true});
onUnmounted(()=>{++revision;clearTimeout(hoverTimer);});
</script>
<template><div class="media-preview" :class="{'media-preview-photo':photo}" @mouseenter="enter" @mouseleave="leave">
<div v-if="photo" class="media-preview-open"><img v-if="src" :src="src" alt="图片原图"/><span v-else>{{loading?'加载中…':error||'图片'}}</span></div>
<div v-else class="media-video-cover"><img v-if="src.startsWith('data:image/')" :src="src" alt="视频封面"/><video v-else-if="src.startsWith('data:video/mp4')" :src="src" muted playsinline preload="auto" @loadedmetadata="($event.target as HTMLVideoElement).currentTime=0.05"/><span v-else>{{loading?'加载封面…':error||'视频'}}</span><span class="video-kind-tag">视频</span></div>
<div class="member-metric-overlay" :class="{'is-visible':hovered}" aria-label="当前媒体成员的浏览与转发">
 <span><UiIcon name="eye"/>{{formatted(member.metrics.views)}}</span>
 <span><UiIcon name="forward"/>{{formatted(member.metrics.forwards)}}</span>
</div></div></template>