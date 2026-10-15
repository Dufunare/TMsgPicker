<script setup lang="ts">
import {computed,ref,watch} from 'vue';
import {requestAvatar,avatarKey,cachedAvatar} from '../media/avatarCache';
import type {Message} from '../types';
const props=defineProps<{member:Message;profileId:string;generation:number}>();
const busy=ref(false);
const key=computed(()=>avatarKey(props.profileId,props.generation,props.member.thumbnail_file_id??undefined));
const url=computed(()=>cachedAvatar(key.value));
async function load(){
 if(!props.member.thumbnail_file_id||busy.value)return;
 busy.value=true;
 try{await requestAvatar(props.profileId,props.generation,props.member.thumbnail_file_id);}finally{busy.value=false;}
}
watch(()=>props.member.thumbnail_file_id,()=>void load(),{immediate:true});
</script>
<template><div class="media-preview">
 <img v-if="url" :src="url" :alt="member.content_kind+'预览'" loading="lazy" decoding="async"/>
 <div v-else class="media-placeholder"><span>{{member.content_kind==='photo'?'图片':member.content_kind==='video'?'视频':member.content_kind==='animation'?'动图':member.content_kind}}</span><button v-if="member.thumbnail_file_id" type="button" :disabled="busy" @click="load">{{busy?'加载中':'加载预览'}}</button></div>
</div></template>
