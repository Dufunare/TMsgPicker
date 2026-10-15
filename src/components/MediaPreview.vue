<script setup lang="ts">
import {ref,computed,watch,onUnmounted} from 'vue';
import {requestMedia} from '../media/mediaRequests';
import type {Message} from '../types';
const props=defineProps<{member:Message;profileId:string;generation:number}>();
const emit=defineEmits<{browse:[]}>();
const src=ref(''),loading=ref(false),error=ref('');let revision=0;
const photo=computed(()=>props.member.content_kind==='photo');
const fileId=computed(()=>photo.value?props.member.full_file_id??props.member.thumbnail_file_id:props.member.thumbnail_file_id);
async function load(){const id=fileId.value,token=++revision;src.value='';error.value='';if(!id)return;loading.value=true;
try{const uri=await requestMedia(props.profileId,props.generation,id,photo.value);
if(token!==revision)return;if(uri?.startsWith('data:image/'))src.value=uri;else error.value='预览不可用';}
catch(e){if(token===revision)error.value=String(e);}finally{if(token===revision)loading.value=false;}}
watch([fileId,()=>props.profileId,()=>props.generation],()=>void load(),{immediate:true});
onUnmounted(()=>++revision);
</script>
<template><div class="media-preview" :class="{'media-preview-photo':photo}">
<button v-if="photo" class="media-preview-open" type="button" :disabled="!member.full_file_id" @click="emit('browse')"><img v-if="src" :src="src" alt="图片原图"/><span v-else>{{loading?'加载中…':error||'图片'}}</span></button>
<div v-else class="media-video-cover"><img v-if="src" :src="src" alt="视频封面"/><span v-else>{{loading?'加载封面…':error||'视频'}}</span><span class="video-kind-tag">视频</span></div>
</div></template>