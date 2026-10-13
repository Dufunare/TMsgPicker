<script setup lang="ts">
import type { Filter, FieldName, CompareOp } from '../types';
defineOptions({ name: 'FilterEditor' });
const model = defineModel<Filter>({ required: true });
const props = withDefaults(defineProps<{ depth?: number }>(), { depth: 0 });
const fields: [FieldName, string][] = [['views','浏览'],['forwards','转发'],['comments','评论'],['reply_info','回复信息'],['reaction_total','反应总数'],['reaction','指定 Emoji'],['post_kind','Single / Album'],['media_album_id','Album ID'],['content_kind','内容类型'],['date','日期（Unix 秒）']];
function condition(): Filter { return { kind: 'compare', field: { kind: 'views' }, op: 'ge', value: 0 }; }
function changeKind(event: Event) {
  const kind = (event.target as HTMLSelectElement).value;
  if (kind === 'and' || kind === 'or') model.value = { kind, items: [condition()] };
  else if (kind === 'not') model.value = { kind, item: condition() };
  else if (kind === 'is_null' || kind === 'is_not_null') model.value = { kind, field: { kind: 'reply_info' } };
  else model.value = condition();
}
function changeField(event: Event) {
  const f = (event.target as HTMLSelectElement).value as FieldName;
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
    <select :value="model.kind" @change="changeKind" aria-label="条件类型">
      <option value="compare">比较</option><option value="is_null">IS NULL</option><option value="is_not_null">IS NOT NULL</option>
      <option value="and" :disabled="props.depth >= 10">AND</option><option value="or" :disabled="props.depth >= 10">OR</option><option value="not" :disabled="props.depth >= 10">NOT</option>
    </select>
    <template v-if="'field' in model">
      <select :value="model.field.kind" @change="changeField" aria-label="字段"><option v-for="[key,label] in fields" :key="key" :value="key">{{ label }}</option></select>
      <input v-if="model.field.kind === 'reaction'" v-model="model.field.emoji" placeholder="👍" aria-label="Emoji" class="short">
      <template v-if="model.kind === 'compare'">
        <select v-model="model.op" aria-label="比较运算符"><option v-for="[key,label] in ([['eq','='],['ne','!='],['gt','>'],['ge','>='],['lt','<'],['le','<=']] as [CompareOp,string][])" :key="key" :value="key">{{ label }}</option></select>
        <select v-if="model.field.kind === 'post_kind'" v-model="model.value"><option value="single">Single</option><option value="album">Album</option></select>
        <select v-else-if="model.field.kind === 'content_kind'" v-model="model.value"><option v-for="k in ['text','photo','video','animation','document','audio','sticker','poll']" :value="k" :key="k">{{ k }}</option></select>
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
