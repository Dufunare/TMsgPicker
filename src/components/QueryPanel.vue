<script setup lang="ts">
import FilterEditor from './FilterEditor.vue';
import AppSelect from './AppSelect.vue';
import { useWorkspaceContext } from '../useWorkspace';
const {ready,busy,selectedChat,scopeKind,rawCount,fromDate,toDate,budget,filter,sortField,sortEmoji,descending,outputLimit,scanning,forwarding,runQuery,cancelQuery}=useWorkspaceContext();
</script>
<template>
<section class="panel builder"><h2>查询</h2><p v-if="selectedChat" class="muted query-context">{{selectedChat.title}}</p><form @submit.prevent="runQuery">
        <label>扫描范围<AppSelect v-model="scopeKind" :options="[{value:'recent',label:'最近 N 条原始消息'},{value:'dates',label:'日期范围'}]" label="扫描范围"/></label>
        <label v-if="scopeKind==='recent'">原始消息数量<input v-model.number="rawCount" type="number" min="1" max="100000" required></label>
        <template v-else><label>开始（本地时间）<input v-model="fromDate" type="datetime-local" required></label><label>结束（不包含）<input v-model="toDate" type="datetime-local" required></label><label>最多扫描原始消息<input v-model.number="budget" type="number" min="1" max="100000" required></label></template>
        <h3>条件</h3><FilterEditor v-model="filter"/>

        <label>排序字段<AppSelect v-model="sortField" :options="[['views','浏览'],['forwards','转发'],['comments','评论'],['reaction_total','反应总数'],['reaction','指定 Emoji'],['date','日期']].map(([value,label])=>({value,label}))" label="排序字段"/></label>
        <label v-if="sortField==='reaction'">Emoji<input v-model="sortEmoji" required></label>
        <label>排序方向<AppSelect v-model="descending" :options="[{value:true,label:'降序'},{value:false,label:'升序'}]" label="排序方向"/></label>
        <label>最多展示帖子<input v-model.number="outputLimit" type="number" min="1" max="1000" required></label>
        <div class="buttons"><button class="primary" :disabled="!ready || !selectedChat || forwarding || busy">{{ scanning ? '重新运行' : '执行查询' }}</button><button type="button" :disabled="!scanning || busy" @click="cancelQuery">取消</button></div>
      </form></section>
      
</template>
