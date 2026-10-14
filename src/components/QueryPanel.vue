<script setup lang="ts">
import FilterEditor from './FilterEditor.vue';
import { useWorkspaceContext } from '../useWorkspace';
const {ready,busy,selectedChat,scopeKind,rawCount,fromDate,toDate,budget,filter,sortField,sortEmoji,descending,outputLimit,scanning,forwarding,preset,runQuery,cancelQuery}=useWorkspaceContext();
</script>
<template>
<section class="panel builder"><h2>查询</h2><p v-if="selectedChat" class="muted query-context">{{selectedChat.title}}</p><form @submit.prevent="runQuery">
        <label>扫描范围<select v-model="scopeKind"><option value="recent">最近 N 条原始消息</option><option value="dates">日期范围 [开始,结束)</option></select></label>
        <label v-if="scopeKind==='recent'">原始消息数量<input v-model.number="rawCount" type="number" min="1" max="100000" required></label>
        <template v-else><label>开始（本地时间）<input v-model="fromDate" type="datetime-local" required></label><label>结束（不包含）<input v-model="toDate" type="datetime-local" required></label><label>最多扫描原始消息<input v-model.number="budget" type="number" min="1" max="100000" required></label></template>
        <h3>条件</h3><FilterEditor v-model="filter"/>
        <button type="button" @click="preset">应用「排除疑似推广」预设</button>
        <label>排序字段<select v-model="sortField"><option v-for="[key,label] in [['views','浏览'],['forwards','转发'],['comments','评论'],['reaction_total','反应总数'],['reaction','指定 Emoji'],['date','日期']]" :key="key" :value="key">{{ label }}</option></select></label>
        <label v-if="sortField==='reaction'">Emoji<input v-model="sortEmoji" required></label>
        <label>排序方向<select v-model="descending"><option :value="true">降序</option><option :value="false">升序</option></select></label>
        <label>最多展示帖子<input v-model.number="outputLimit" type="number" min="1" max="1000" required></label>
        <div class="buttons"><button class="primary" :disabled="!ready || !selectedChat || forwarding || busy">{{ scanning ? '重新运行' : '执行查询' }}</button><button type="button" :disabled="!scanning || busy" @click="cancelQuery">取消</button></div>
      </form></section>
      
</template>
