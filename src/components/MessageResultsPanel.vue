<script setup lang="ts">
import {reactive,ref} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
import LinkedText from './LinkedText.vue';
import MediaPreview from './MediaPreview.vue';
import MediaBrowser from './MediaBrowser.vue';
const {busy,error,chatList,selectedChat,result,scanning,openPost,count,reactionLabel,date,profileId,connection}=useWorkspaceContext();
const expanded=reactive<Record<string,boolean>>({});
const browser=ref<{items:import('../types').Message[];index:number}|null>(null);
const mediaKinds=new Set(['photo','video','animation','video_note','document','audio','voice_note','sticker']);
function hasMedia(members:{content_kind:string}[]){return members.some(m=>mediaKinds.has(m.content_kind));}
</script>
<template>
  <section class="results">
    <div class="panel result-header">
      <p v-if="error" role="alert" class="error">{{error}}</p>
      <h2>{{selectedChat?.title??'Messages & Results'}} <span v-if="result" class="badge" :class="result.status">{{result.status}}</span></h2>
      <p v-if="result">已扫描 {{result.scanned}} 条 · {{result.pages}} 页 · {{result.exact?'精确':'部分结果'}}</p>
      <p v-if="result?.detail&&result.status==='Error'" class="muted">{{result.detail}}</p>
    </div>
    <div v-if="result&&!result.posts.length" class="panel empty">{{scanning?'正在读取历史…':'当前范围没有匹配帖子'}}</div>
    <article v-for="post in result?.posts??[]" :key="post.key" class="panel post">
      <div class="post-top"><strong>{{chatList.find(c=>c.id===post.representative.chat_id)?.title??selectedChat?.title??'Telegram'}}</strong><button :disabled="busy" @click="openPost(post)">查看原文 ↗</button></div>
      <p class="muted">{{date(post.date)}} · {{post.kind==='album'?'媒体组':'消息'}}<span v-if="post.partial"> · 部分内容</span></p>
      <p v-if="post.text" class="post-text"><LinkedText :text="post.text"/></p>
      <div v-if="hasMedia(post.members)" class="post-media">
        <button class="media-expander" type="button" :aria-expanded="!!expanded[post.key]" @click="expanded[post.key]=!expanded[post.key]">
          {{post.kind==='album'?'媒体组':post.members.find(m=>mediaKinds.has(m.content_kind))?.content_kind}} · {{post.members.filter(m=>mediaKinds.has(m.content_kind)).length}} 项
          <span>{{expanded[post.key]?'收起媒体':'展开媒体'}}</span>
        </button>
        <div v-if="expanded[post.key]" class="media-grid">
          <MediaPreview v-for="member in post.members.filter(m=>mediaKinds.has(m.content_kind))" :key="member.key.message_id" :member="member" :profile-id="profileId" :generation="connection.generation" @browse="browser={items:post.members.filter(m=>mediaKinds.has(m.content_kind)),index:post.members.filter(m=>mediaKinds.has(m.content_kind)).findIndex(m=>m.key.message_id===member.key.message_id)}"/>
        </div>
      </div>
      <div class="metrics"><span>浏览 <b>{{count(post.metrics.views)}}</b></span><span>转发 <b>{{count(post.metrics.forwards)}}</b></span><span>评论 <b>{{count(post.metrics.reply_info)}}</b></span></div>
      <div class="reactions"><span v-if="post.metrics.reactions===null">反应 —</span><span v-else-if="!post.metrics.reactions.length">反应 0</span><span v-for="(reaction,i) in post.metrics.reactions??[]" :key="i">{{reactionLabel(reaction.key)}} {{count(reaction.count)}}</span></div>
    </article>
  <MediaBrowser v-if="browser" :items="browser.items" :index="browser.index" :profile-id="profileId" :generation="connection.generation" @close="browser=null"/>
  </section>
</template>
