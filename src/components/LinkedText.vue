<script setup lang="ts">
import {computed} from 'vue';
import {openUrl} from '@tauri-apps/plugin-opener';
const props=defineProps<{text:string}>();
const parts=computed(()=>{
  const result:Array<{text:string;url?:string}>=[];
  const re=/https?:\/\/[^\s<>"']+/gi;let cursor=0;
  for(const match of props.text.matchAll(re)){
    const start=match.index??0;if(start>cursor)result.push({text:props.text.slice(cursor,start)});
    const raw=match[0];const url=raw.replace(/[.,;!?)，。！？）]+$/u,'');
    result.push({text:url,url});if(url.length<raw.length)result.push({text:raw.slice(url.length)});
    cursor=start+raw.length;
  }
  if(cursor<props.text.length)result.push({text:props.text.slice(cursor)});
  return result;
});
async function open(url:string){
  try{const u=new URL(url);if(u.protocol==='http:'||u.protocol==='https:')await openUrl(u.toString());}catch{ /* Invalid URL is never opened. */ }
}
</script>
<template><span class="linked-text"><template v-for="(part,i) in parts" :key="i"><a v-if="part.url" :href="part.url" rel="noreferrer noopener" target="_blank" @click.prevent="open(part.url)">{{part.text}}</a><template v-else>{{part.text}}</template></template></span></template>
