<script setup lang="ts">
import type { Filter, FieldName, CompareOp } from '../types';
import AppSelect from './AppSelect.vue';
defineOptions({ name: 'FilterEditor' });
const model = defineModel<Filter>({ required: true });
const props = withDefaults(defineProps<{ depth?: number }>(), { depth: 0 });
const fields: [FieldName, string][] = [['views','浏览'],['forwards','转发'],['comments','评论'],['reply_info','回复信息'],['reaction_total','反应总数'],['reaction','指定 Emoji'],['post_kind','Single / Album'],['media_album_id','Album ID'],['content_kind','内容类型'],['date','日期（Unix 秒）']];
function condition(): Filter { return { kind: 'compare', field: { kind: 'views' }, op: 'ge', value: 0 }; }
function changeKind(kind: string) {
  if (kind === 'and' || kind === 'or') model.value = { kind, items: [condition()] };
  else if (kind === 'not') model.value = { kind, item: condition() };
  else if (kind === 'is_null' || kind === 'is_not_null') model.value = { kind, field: { kind: 'reply_info' } };
  else model.value = condition();
}
function changeField(f: FieldName) {
  if ('field' in model.value) {
    model.value.field = { kind: f, ...(f === 'reaction' ? { emoji: '👍' } : {}) };
    if (model.value.kind === 'compare') model.value.value = ['post_kind','media_album_id','content_kind'].includes(f) ? (f === 'post_kind' ? 'single' : '') : 0;
  }
}
function changeValue(event: Event) {
  if (model.value.kind === 'compare') {
    const value = (event.target as HTMLInputElement).value;
    model.value.value = ['post_kind','media_album_id','content_kind'].includes(model.value.field.kind) ? value : Number(value);
  }
}
</script>
<template>
  <div class="filter-node">
    <AppSelect :model-value="model.kind" label="条件类型" :options="[{value:'compare',label:'比较'},{value:'is_null',label:'IS NULL'},{value:'is_not_null',label:'IS NOT NULL'},{value:'and',label:'AND',disabled:props.depth>=10},{value:'or',label:'OR',disabled:props.depth>=10},{value:'not',label:'NOT',disabled:props.depth>=10}]" @update:model-value="changeKind(String($event))"/>
    <template v-if="'field' in model">
      <AppSelect :model-value="model.field.kind" label="字段" :options="fields.map(([value,label])=>({value,label}))" @update:model-value="changeField($event as FieldName)"/>
      <input v-if="model.field.kind === 'reaction'" v-model="model.field.emoji" placeholder="👍" aria-label="Emoji" class="short">
      <template v-if="model.kind === 'compare'">
        <AppSelect v-model="model.op" label="比较运算符" :options="([['eq','='],['ne','!='],['gt','>'],['ge','>='],['lt','<'],['le','<=']] as [CompareOp,string][]).map(([value,label])=>({value,label}))"/>
        <AppSelect v-if="model.field.kind === 'post_kind'" v-model="model.value" label="帖子类型" :options="[{value:'single',label:'Single'},{value:'album',label:'Album'}]"/>
        <AppSelect v-else-if="model.field.kind === 'content_kind'" v-model="model.value" label="内容类型" :options="['text','photo','video','animation','document','audio','sticker','poll'].map(value=>({value,label:value}))"/>
        <input v-else :type="model.field.kind === 'media_album_id' ? 'text' : 'number'" :value="model.value" @input="changeValue" aria-label="条件值" class="short">
      </template>
    </template>
    <div v-if="model.kind === 'and' || model.kind === 'or'" class="filter-children">
      <div v-for="(_,i) in model.items" :key="i" class="filter-row"><FilterEditor v-model="model.items[i]!" :depth="props.depth+1"/><button type="button" @click="model.items.splice(i,1)" aria-label="删除条件">×</button></div>
      <button type="button" @click="model.items.push(condition())">＋ 条件</button>
    </div>
    <div v-else-if="model.kind === 'not'" class="filter-children"><FilterEditor v-model="model.item" :depth="props.depth+1"/></div>
  </div>
</template>
