<script setup lang="ts">
import type {Filter,FieldName,CompareOp} from '../types';
import AppSelect from './AppSelect.vue';
defineOptions({name:'FilterEditor'});
const model=defineModel<Filter>({required:true});
const props=withDefaults(defineProps<{depth?:number}>(),{depth:0});
const fields:[FieldName,string][]=[['views','浏览'],['forwards','转发'],['comments','评论'],['reply_info','回复信息'],['reaction_total','反应总数'],['reaction','指定 Emoji']];
const contents=[['text','文字'],['photo','图片'],['video','视频'],['animation','动图'],['document','文件'],['audio','音频'],['voice_note','语音'],['video_note','视频留言'],['sticker','贴纸'],['poll','投票']];
const condition=():Filter=>({kind:'compare',field:{kind:'views'},op:'ge',value:0});
function changeKind(kind:string){
 if(kind==='and'||kind==='or')model.value={kind,items:[condition()]};
 else if(kind==='not')model.value={kind,item:condition()};
 else if(kind==='contains_content')model.value={kind,content_kind:'photo',exists:true};
 else if(kind==='is_null'||kind==='is_not_null')model.value={kind,field:{kind:'reply_info'}};
 else model.value=condition();
}
function changeField(kind:FieldName){if('field' in model.value)model.value.field={kind,...(kind==='reaction'?{emoji:'👍'}:{})};}
</script>
<template><div class="filter-node">
 <AppSelect :model-value="model.kind" label="谓词" :options="[{value:'compare',label:'比较'},{value:'contains_content',label:'类型'},{value:'is_null',label:'不存在'},{value:'is_not_null',label:'存在'},{value:'and',label:'AND',disabled:props.depth>=10},{value:'or',label:'OR',disabled:props.depth>=10},{value:'not',label:'NOT',disabled:props.depth>=10}]" @update:model-value="changeKind(String($event))"/>
 <template v-if="model.kind==='contains_content'">
  <AppSelect v-model="model.content_kind" label="内容类型" :options="contents.map(([value,label])=>({value,label}))"/>
  <AppSelect v-model="model.exists" label="存在性" :options="[{value:true,label:'存在'},{value:false,label:'不存在'}]"/>
 </template>
 <template v-if="'field' in model">
  <AppSelect :model-value="model.field.kind" label="指标" :options="fields.map(([value,label])=>({value,label}))" @update:model-value="changeField($event as FieldName)"/>
  <input v-if="model.field.kind==='reaction'" v-model="model.field.emoji" placeholder="👍" aria-label="Emoji" class="short"/>
  <template v-if="model.kind==='compare'">
   <AppSelect v-model="model.op" label="比较运算符" :options="([['eq','='],['ne','≠'],['gt','>'],['ge','≥'],['lt','<'],['le','≤']] as [CompareOp,string][]).map(([value,label])=>({value,label}))"/>
   <input v-model.number="model.value" type="number" aria-label="条件值" class="short"/>
  </template>
 </template>
 <div v-if="model.kind==='and'||model.kind==='or'" class="filter-children">
  <div v-for="(_,i) in model.items" :key="i" class="filter-row"><FilterEditor v-model="model.items[i]!" :depth="props.depth+1"/><button type="button" @click="model.items.splice(i,1)" aria-label="删除条件">×</button></div>
  <button type="button" @click="model.items.push(condition())">＋ 条件</button>
 </div>
 <div v-else-if="model.kind==='not'" class="filter-children"><FilterEditor v-model="model.item" :depth="props.depth+1"/></div>
</div></template>