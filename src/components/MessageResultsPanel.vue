<script setup lang="ts">
import {computed,reactive,ref,watch} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
import LinkedText from './LinkedText.vue';
import MediaPreview from './MediaPreview.vue';
import {scanStatusLabel,scanReasonLabel,postCompletenessLabel,windowPartiallyCovered} from '../scan/scanUi';
const {busy,error,chatList,selectedChat,activeResult:result,scanning,openPost,count,reactionLabel,date,profileId,connection}=useWorkspaceContext();
const expanded=reactive<Record<string,boolean>>({});
// Fixed-size result pages keep mounted DOM and Vue patch work bounded even
// when the requested output_limit is 1000. No new native results are fetched.
const PAGE_SIZE=40;
const page=ref(1);
const posts=computed(()=>result.value?.posts??[]);
const pageCount=computed(()=>Math.max(1,Math.ceil(posts.value.length/PAGE_SIZE)));
const visiblePosts=computed(()=>posts.value.slice((page.value-1)*PAGE_SIZE,page.value*PAGE_SIZE));
watch([profileId,()=>selectedChat.value?.id,()=>result.value?.run_id],()=>{
  page.value=1;
  for(const key of Object.keys(expanded))delete expanded[key];
});
watch(pageCount,()=>{if(page.value>pageCount.value)page.value=pageCount.value;});
function changePage(next:number){
  if(next<1||next>pageCount.value)return;
  page.value=next;
  for(const key of Object.keys(expanded))delete expanded[key];
}
const mediaKinds=new Set(['photo','video','animation','video_note','document','audio','voice_note','sticker']);
function hasMedia(members:{content_kind:string}[]){return members.some(m=>mediaKinds.has(m.content_kind));}
</script>
<template>
  <section class="results">
    <div class="panel result-header">
      <p v-if="error" role="alert" class="error">{{error}}</p>
      <h2>{{selectedChat?.title??'Messages & Results'}} <span v-if="result" class="badge" :class="result.status">{{scanStatusLabel(result.status)}}</span></h2>
      <p v-if="result">范围内 {{result.coverage.examined_posts}} 个 Post · 匹配 {{result.stats.posts_matched}} 个 · 读取底层消息 {{result.stats.raw_messages_read}} 条 · {{result.stats.pages}} 页</p>
      <p v-if="result&&windowPartiallyCovered(result)" class="muted">已达到日期范围内指定的 Post 数量；没有遍历整个日期范围。</p>
      <p v-if="result?.status==='BudgetExceeded'||result?.status==='Interrupted'||result?.status==='Error'" class="muted" role="status">{{scanReasonLabel(result.stop_reason)}} · {{result.detail}}</p>
    </div>
    <div v-if="!result" class="panel empty">{{selectedChat?'请在右侧 Query Builder 执行查询':'请先选择聊天'}}</div>
    <div v-else-if="!result.posts.length" class="panel empty">{{scanning?'正在读取历史…':'当前范围没有匹配帖子'}}</div>

    <article v-for="post in visiblePosts" :key="post.key" class="panel post">
      <div class="post-top"><strong>{{chatList.find(c=>c.id===post.representative.chat_id)?.title??selectedChat?.title??'Telegram'}}</strong><button :disabled="busy" @click="openPost(post)">查看原文 ↗</button></div>
      <p class="muted">{{date(post.date)}} · {{post.kind==='album'?'媒体组':'消息'}}<span v-if="post.completeness.level!=='Verified'" class="post-completeness" :title="post.completeness.reason"> · {{postCompletenessLabel(post.completeness.level)}}</span></p>
      <p v-if="post.text" class="post-text"><LinkedText :text="post.text" :links="post.links"/></p>
      <div v-if="hasMedia(post.members)" class="post-media">
        <button class="media-expander" type="button" :aria-expanded="!!expanded[post.key]" @click="expanded[post.key]=!expanded[post.key]">
          {{post.kind==='album'?'媒体组':post.members.find(m=>mediaKinds.has(m.content_kind))?.content_kind}} · {{post.members.filter(m=>mediaKinds.has(m.content_kind)).length}} 项
          <span>{{expanded[post.key]?'收起媒体':'展开媒体'}}</span>
        </button>
        <div v-if="expanded[post.key]" class="media-grid">
          <MediaPreview v-for="member in post.members.filter(m=>mediaKinds.has(m.content_kind))" :key="member.key.message_id" :member="member" :profile-id="profileId" :generation="connection.generation"/>
        </div>
      </div>
      <div class="metrics"><span>浏览 <b>{{count(post.metrics.views)}}</b></span><span>转发 <b>{{count(post.metrics.forwards)}}</b></span><span>评论 <b>{{count(post.metrics.reply_info)}}</b></span></div>
      <div class="reactions"><span v-if="post.metrics.reactions===null">反应 —</span><span v-else-if="!post.metrics.reactions.length">反应 0</span><span v-for="(reaction,i) in post.metrics.reactions??[]" :key="i">{{reactionLabel(reaction.key)}} {{count(reaction.count)}}</span></div>
    </article>
        <div v-if="posts.length>PAGE_SIZE" class="result-pagination" role="navigation" aria-label="扫描结果分页">
      <span>第 {{page}} / {{pageCount}} 页 · 共 {{posts.length}} 个结果</span>
      <button type="button" :disabled="page<=1" @click="changePage(page-1)" aria-label="上一页">上一页</button>
      <button type="button" :disabled="page>=pageCount" @click="changePage(page+1)" aria-label="下一页">下一页</button>
    </div>
  </section>
</template>
