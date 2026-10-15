<script setup lang="ts">
import {ref} from 'vue';
import { useWorkspaceContext } from '../useWorkspace';
const {busy,error,chatList,selectedChat,result,scanning,openPost,count,reactionLabel,date}=useWorkspaceContext();
const expanded=ref<Record<string,boolean>>({});
function toggle(key:string){expanded.value[key]=!expanded.value[key];}
</script>
<template>
<section class="results"><div class="panel result-header"><p v-if="error" role="alert" class="error">{{error}}</p><h2>{{selectedChat?.title ?? 'Messages & Results'}} <span v-if="result" class="badge" :class="result.status">{{ result.status }}</span></h2><p v-if="result">已扫描 {{ result.scanned }} 条 · {{ result.pages }} 页 · {{ result.exact ? 'Exact：此 Scope 内精确排名' : 'Partial：临时或不完整排名' }}</p><p v-if="result?.detail && result.status==='Error'" class="muted">{{result.detail}}</p></div>
        <div v-if="result && !result.posts.length" class="panel empty">{{ scanning ? '正在读取历史…' : '当前扫描范围没有匹配帖子。' }}</div>
        <article v-for="post in result?.posts ?? []" :key="post.key" class="panel post">
  <div class="post-top"><strong>{{chatList.find(c=>c.id===post.representative.chat_id)?.title??selectedChat?.title??'Telegram'}}</strong><button :disabled="busy" @click="openPost(post)">查看原文 ↗</button></div>
  <p class="muted">{{date(post.date)}} · {{post.kind==='album'?'媒体组':'消息'}}<span v-if="post.partial"> · 部分内容</span></p>
  <p class="post-text" :class="{'post-clamped':!expanded[post.key]}">{{post.text||'（媒体消息）'}}</p>
  <div class="metrics"><span>浏览 <b>{{count(post.metrics.views)}}</b></span><span>转发 <b>{{count(post.metrics.forwards)}}</b></span><span>评论 <b>{{count(post.metrics.reply_info)}}</b></span></div>
  <div class="reactions"><span v-if="post.metrics.reactions===null">反应 —</span><span v-else-if="!post.metrics.reactions.length">反应 0</span><span v-for="(reaction,i) in post.metrics.reactions??[]" :key="i">{{reactionLabel(reaction.key)}} {{count(reaction.count)}}</span></div>
  <div class="post-actions"><button type="button" :aria-expanded="!!expanded[post.key]" @click="toggle(post.key)">{{expanded[post.key]?'收起详情':'展开详情'}}</button></div>
  <section v-if="expanded[post.key]" class="post-details">
    <div>内容类型：{{post.kind==='album'?'媒体组':'单条消息'}}</div><div>成员：{{post.members.length}}</div>
    <div class="media"><span v-for="member in post.members" :key="member.key.message_id" class="media-kind">{{member.content_kind}}</span></div>
    <details><summary>指标来源</summary><p v-for="(source,field) in post.provenance" :key="field">{{field}} ← 消息 {{source}}</p></details>
  </section>
</article>
        
        
      </section>
    
</template>
