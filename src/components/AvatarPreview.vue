<script setup lang="ts">
import {computed,ref,watch} from 'vue';
import {isTauri} from '@tauri-apps/api/core';
import {avatarKey,cachedAvatar,requestAvatar} from '../media/avatarCache';
const props=withDefaults(defineProps<{profileId:string;generation:number;fileId?:number;fallbackImage?:string|null;account?:boolean;name:string;size?:'sm'|'md'}>(),{size:'md'});
const active=ref(false);
const root=ref<HTMLElement>();
const key=computed(()=>avatarKey(props.profileId,props.generation,props.account ? undefined : props.fileId));
const cached=computed(()=>cachedAvatar(key.value));
const url=computed(()=>cached.value||props.fallbackImage||'');
let observer:IntersectionObserver|undefined;
watch(key,()=>{if(active.value)void load();});
async function load(){if(isTauri()&&(props.account||props.fileId))await requestAvatar(props.profileId,props.generation,props.account?undefined:props.fileId);}
function observe(el:HTMLElement|null){
  observer?.disconnect();observer=undefined;
  if(!el)return;
  if(typeof IntersectionObserver==='undefined'){active.value=true;void load();return;}
  observer=new IntersectionObserver(([entry])=>{if(entry?.isIntersecting){active.value=true;void load();observer?.disconnect();}},{rootMargin:'180px'});
  observer.observe(el);
}
import {onMounted,onUnmounted,nextTick} from 'vue';
onMounted(async()=>{await nextTick();observe(root.value??null);});
onUnmounted(()=>observer?.disconnect());
const initial=computed(()=>Array.from(props.name)[0]?.toUpperCase()??'?');
</script>
<template><span ref="root" class="avatar-preview" :class="['avatar-'+size]" :title="name">
  <img v-if="url" :src="url" :class="{'is-fallback':!cached}" :alt="name+'头像'" loading="lazy" decoding="async"/>
  <span v-else aria-hidden="true">{{initial}}</span>
</span></template>
