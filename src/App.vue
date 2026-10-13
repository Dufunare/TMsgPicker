<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import FilterEditor from './components/FilterEditor.vue';
import type { Chat, Connection, FieldName, Filter, ForwardReport, Post, QuerySpec, ScanResult } from './types';

const connection = ref<Connection>({ auth_state: 'Disconnected', connection_state: 'Disconnected', detail: '请输入配置并连接' });
const ready = computed(() => connection.value.auth_state === 'authorizationStateReady');
const busy = ref(false), error = ref(''), notice = ref('');
const apiId = ref(''), apiHash = ref(''), passphrase = ref(''), proxyEnabled = ref(false), proxyHost = ref('127.0.0.1'), proxyPort = ref(1080), proxyUser = ref(''), proxyPassword = ref(''), authValue = ref('');
const chatList = ref<Chat[]>([]), search = ref(''), selectedChat = ref<Chat | null>(null);
const scopeKind = ref('recent'), rawCount = ref(50), fromDate = ref(''), toDate = ref(''), budget = ref(5000);
const filter = ref<Filter>({ kind: 'and', items: [] });
const sortField = ref<FieldName>('views'), sortEmoji = ref('👍'), descending = ref(true), outputLimit = ref(20);
const result = ref<ScanResult | null>(null), currentRun = ref(''), scanning = ref(false), selected = ref<string[]>([]);
const target = ref(''), confirmation = ref(false), forwarding = ref(false), report = ref<ForwardReport | null>(null);
const preview = ref<ForwardReport | null>(null), reportTarget = ref('');
let confirmedSelection: {target:string;keys:string[];runId:string} | null = null;
const targetTitle = computed(() => chatList.value.find(c => c.id === target.value)?.title ?? '');
const selectedMessageCount = computed(() => result.value?.posts.filter(p => selected.value.includes(p.key)).reduce((n,p) => n+p.members.length,0) ?? 0);
const authLabel = computed(() => ({ authorizationStateWaitPhoneNumber: '手机号（含国家代码）', authorizationStateWaitCode: 'Telegram 验证码', authorizationStateWaitPassword: '两步验证密码', authorizationStateWaitEmailAddress: '授权邮箱', authorizationStateWaitEmailCode: '邮箱验证码' } as Record<string,string>)[connection.value.auth_state]);
const unlisteners: UnlistenFn[] = [];
const sendUpdates = new Map<string, string>();
async function action(fn: () => Promise<void>) { error.value=''; busy.value=true; try { await fn(); } catch (e) { error.value=String(e); } finally { busy.value=false; } }
async function connect() {
  await action(async () => {
    await invoke('connect', { config: { api_id: apiId.value ? Number(apiId.value) : null, api_hash: apiHash.value || null, passphrase: passphrase.value, proxy: proxyEnabled.value ? { server: proxyHost.value, port: proxyPort.value, username: proxyUser.value, password: proxyPassword.value } : null } });
    notice.value = '配置已提交；代理设置已在初始化前完成。';
  });
  apiHash.value=''; passphrase.value=''; proxyPassword.value='';
}
async function submitAuth() { const value=authValue.value; authValue.value=''; await action(async () => { await invoke('auth_submit',{value}); }); }
async function disconnect() { await action(async () => {await invoke('disconnect');scanning.value=false;chatList.value=[];selectedChat.value=null;result.value=null;selected.value=[];report.value=null;}); }
async function loadChats() { await action(async () => {chatList.value=await invoke<Chat[]>('chats',{search:search.value});}); }
async function resolvePublic() { await action(async () => {const c=await invoke<Chat>('public_chat',{username:search.value}); if (!chatList.value.some(x=>x.id===c.id)) chatList.value.unshift(c); selectedChat.value=c;}); }
async function savedMessages() { await action(async () => {const c=await invoke<Chat>('saved_messages');if (!chatList.value.some(x=>x.id===c.id)) chatList.value.push(c);target.value=c.id;confirmation.value=false;confirmedSelection=null;}); }
function preset() {filter.value={kind:'not',item:{kind:'and',items:[{kind:'is_null',field:{kind:'reply_info'}},{kind:'compare',field:{kind:'post_kind'},op:'eq',value:'single'}]}};}
async function runQuery() {
  error.value=''; if (!selectedChat.value) {error.value='请先选择一个聊天';return;}
  const from=Date.parse(fromDate.value)/1000, to=Date.parse(toDate.value)/1000;
  if (scopeKind.value==='dates' && (!Number.isFinite(from) || !Number.isFinite(to) || from>=to)) {error.value='请输入有效的开始/结束日期';return;}
  const spec: QuerySpec={version:1,chat_id:selectedChat.value.id,scope:scopeKind.value==='recent'?{kind:'recent',count:rawCount.value}:{kind:'dates',from:Math.floor(from),to:Math.floor(to),budget:budget.value},filter:filter.value,sort:[{field:{kind:sortField.value,...(sortField.value==='reaction'?{emoji:sortEmoji.value}:{})},descending:descending.value}],output_limit:outputLimit.value};
  const id=crypto.randomUUID();currentRun.value=id;scanning.value=true;selected.value=[];confirmation.value=false;report.value=null;confirmedSelection=null;
  result.value={run_id:id,status:'Scanning',scanned:0,pages:0,exact:false,detail:'正在读取历史；排名为临时结果',posts:[]};
  try {await invoke('start_query',{spec,runId:id});} catch (e) {if(currentRun.value===id) {scanning.value=false;error.value=String(e);result.value={...result.value!,status:'Error',detail:String(e)};}}
}
async function cancelQuery() {await action(async () => {await invoke('cancel_query',{runId:currentRun.value});});}
async function openPost(post: Post) {await action(async () => {const url=await invoke<string>('message_link',{key:post.representative,album:post.kind==='album'});await openUrl(url);});}
async function prepareForward() {
  const selection={target:target.value,keys:[...selected.value],runId:currentRun.value};
  await action(async () => {
    preview.value=await invoke<ForwardReport>('preview_forward',{target:selection.target,postKeys:selection.keys,runId:selection.runId});
    if(selection.target!==target.value || selection.runId!==currentRun.value || selection.keys.join('|')!==selected.value.join('|')) return;
    confirmedSelection=selection;confirmation.value=true;
  });
}
async function forward() {
  if(!confirmedSelection) return;
  const selection=confirmedSelection;
  forwarding.value=true;error.value='';
  try {
    reportTarget.value=selection.target;
    report.value=await invoke<ForwardReport>('forward_selected',{target:selection.target,postKeys:selection.keys,runId:selection.runId,confirmed:true});
    for (const item of report.value.items) if (item.sent_id) { const state=sendUpdates.get(`${selection.target}:${item.sent_id}`); if(state) item.state=state; }
    confirmation.value=false;
  } catch(e) {error.value=String(e);} finally {forwarding.value=false;}
}
function count(v: number | null | undefined) {return v == null ? '—' : v.toLocaleString();}
function reactionLabel(key: {kind:string;value?:string}) {return key.kind==='emoji'?key.value:key.kind==='custom_emoji'?`自定义 Emoji ${key.value}`:key.kind==='paid'?'付费反应':`其他反应 ${key.value ?? ''}`;}
function date(seconds:number) {return new Date(seconds*1000).toLocaleString();}
onMounted(async () => {
  if(!isTauri()) {error.value='请通过 pnpm tauri dev 启动桌面程序；浏览器预览无法连接 TDLib。';return;}
  try {
    unlisteners.push(await listen<Connection>('connection',e=>{connection.value=e.payload;}));
    unlisteners.push(await listen<ScanResult>('query-progress',e=>{if(e.payload.run_id!==currentRun.value)return;result.value=e.payload;scanning.value=e.payload.status==='Scanning';selected.value=selected.value.filter(key=>e.payload.posts.some(p=>p.key===key));}));
    unlisteners.push(await listen<string>('app-error',e=>{error.value=e.payload;}));
    unlisteners.push(await listen<{target:string;old_message_id:string;state:string}>('forward-update',e=>{sendUpdates.set(`${e.payload.target}:${e.payload.old_message_id}`,e.payload.state);if(e.payload.target===reportTarget.value)for(const item of report.value?.items ?? [])if(item.sent_id===e.payload.old_message_id)item.state=e.payload.state;}));
    connection.value=await invoke<Connection>('connection_status');
  } catch(e) {error.value=String(e);}
});
onUnmounted(()=>unlisteners.forEach(fn=>fn()));
</script>

<template>
  <main>
    <header><div><h1>TMsgPicker</h1><p>按互动指标寻找 Telegram 帖子</p></div><div class="connection"><strong>{{ ready ? '已登录' : connection.auth_state }}</strong><span>{{ connection.connection_state }}</span></div></header>
    <p v-if="error" role="alert" class="error">{{ error }}</p>
    <p v-if="notice" class="notice">{{ notice }}</p>
    <details :open="!ready" class="panel auth">
      <summary>账号与 SOCKS5 配置</summary><p>{{ connection.detail }}</p>
      <form @submit.prevent="connect" class="config-form">
        <label>API ID<input v-model="apiId" type="number" placeholder="或使用环境变量" autocomplete="off"></label>
        <label>API hash<input v-model="apiHash" type="password" placeholder="或使用环境变量" autocomplete="off"></label>
        <label>本地数据库密码<input v-model="passphrase" type="password" required autocomplete="off" placeholder="与现有 Session 密码一致"></label>
        <label class="check"><input v-model="proxyEnabled" type="checkbox">启用 SOCKS5</label>
        <template v-if="proxyEnabled"><label>服务器<input v-model="proxyHost" required></label><label>端口<input v-model.number="proxyPort" type="number" min="1" max="65535" required></label><label>代理用户名<input v-model="proxyUser" autocomplete="off"></label><label>代理密码<input v-model="proxyPassword" type="password" autocomplete="off"></label></template>
        <button :disabled="busy || scanning || forwarding" class="primary">连接 / 初始化</button><button type="button" :disabled="busy || forwarding || connection.auth_state==='Disconnected'" @click="disconnect">安全断开</button>
      </form>
      <form v-if="authLabel" @submit.prevent="submitAuth" class="auth-step"><label>{{ authLabel }}<input v-model="authValue" :type="connection.auth_state==='authorizationStateWaitPhoneNumber' || connection.auth_state==='authorizationStateWaitEmailAddress' ? 'text' : 'password'" required autocomplete="off"></label><button :disabled="busy" class="primary">提交授权</button></form>
      <small>凭据只在内存中使用，不保存到浏览器。每次启动需提供数据库密码；已授权 Session 无需重复登录码。</small>
    </details>
    <div class="workspace">
      <aside class="panel chats"><h2>聊天</h2><form @submit.prevent="loadChats"><input v-model="search" placeholder="已有聊天标题 / 公开 username" aria-label="搜索聊天"><div class="buttons"><button :disabled="!ready || busy">搜索 / 刷新</button><button type="button" :disabled="!ready || busy || !search" @click="resolvePublic">解析 @username</button></div></form><p v-if="!chatList.length" class="muted">登录后刷新聊天列表。可选择频道、群组或私人聊天。</p><button v-for="chat in chatList" :key="chat.id" class="chat" :class="{ chosen: selectedChat?.id===chat.id }" @click="selectedChat=chat"><strong>{{ chat.title }}</strong><small>{{ chat.kind }} · {{ chat.id }}</small></button></aside>
      <section class="panel builder"><h2>查询</h2><p class="muted">{{ selectedChat?.title ?? '先选择聊天' }}</p><form @submit.prevent="runQuery">
        <label>扫描范围<select v-model="scopeKind"><option value="recent">最近 N 条原始消息</option><option value="dates">日期范围 [开始,结束)</option></select></label>
        <label v-if="scopeKind==='recent'">原始消息数量<input v-model.number="rawCount" type="number" min="1" max="100000" required></label>
        <template v-else><label>开始（本地时间）<input v-model="fromDate" type="datetime-local" required></label><label>结束（不包含）<input v-model="toDate" type="datetime-local" required></label><label>最多扫描原始消息<input v-model.number="budget" type="number" min="1" max="100000" required></label></template>
        <h3>条件</h3><FilterEditor v-model="filter"/><small>空 AND = 所有帖子。未知数值不匹配比较；NOT(未知) 仍为未知。</small>
        <button type="button" @click="preset">应用「排除疑似推广」预设</button><small>仅适合观察到该结构规律的频道，可能误判。</small>
        <label>排序字段<select v-model="sortField"><option v-for="[key,label] in [['views','浏览'],['forwards','转发'],['comments','评论'],['reaction_total','反应总数'],['reaction','指定 Emoji'],['date','日期']]" :key="key" :value="key">{{ label }}</option></select></label>
        <label v-if="sortField==='reaction'">Emoji<input v-model="sortEmoji" required></label>
        <label>排序方向<select v-model="descending"><option :value="true">降序</option><option :value="false">升序</option></select></label>
        <label>最多展示帖子<input v-model.number="outputLimit" type="number" min="1" max="1000" required></label><small>展示上限不改变扫描范围；空值始终排最后。</small>
        <div class="buttons"><button class="primary" :disabled="!ready || !selectedChat || forwarding || busy">{{ scanning ? '重新运行' : '执行查询' }}</button><button type="button" :disabled="!scanning || busy" @click="cancelQuery">取消</button></div>
      </form></section>
      <section class="results"><div class="panel result-header"><h2>结果 <span v-if="result" class="badge" :class="result.status">{{ result.status }}</span></h2><p v-if="result">已扫描 {{ result.scanned }} 条 · {{ result.pages }} 页 · {{ result.exact ? 'Exact：此 Scope 内精确排名' : 'Partial：临时或不完整排名' }}</p><p class="muted">{{ result?.detail ?? '选择聊天并执行查询。' }}</p></div>
        <div v-if="result && !result.posts.length" class="panel empty">{{ scanning ? '正在读取历史…' : '当前扫描范围没有匹配帖子。' }}</div>
        <article v-for="post in result?.posts ?? []" :key="post.key" class="panel post"><div class="post-top"><label class="check"><input v-model="selected" type="checkbox" :value="post.key" :disabled="scanning || forwarding || confirmation"><strong>{{ chatList.find(c=>c.id===post.representative.chat_id)?.title ?? post.representative.chat_id }}</strong></label><button :disabled="busy" @click="openPost(post)">打开原文 ↗</button></div><p class="muted">{{ date(post.date) }} · {{ post.kind==='album' ? `Album · ${post.members.length} 个已扫描成员` : 'Single' }} <span v-if="post.partial">· 成员完整性未知</span></p><p class="post-text">{{ post.text || '（无文本 / caption）' }}</p><div class="media"><span v-for="member in post.members" :key="member.key.message_id" class="media-kind">{{ member.content_kind }} · {{ member.key.message_id }}</span></div><div class="metrics"><span>浏览 <b>{{ count(post.metrics.views) }}</b></span><span>转发 <b>{{ count(post.metrics.forwards) }}</b></span><span>评论 <b>{{ count(post.metrics.reply_info) }}</b></span></div><div class="reactions"><span v-if="post.metrics.reactions===null">反应 —</span><span v-else-if="!post.metrics.reactions.length">反应 0</span><span v-for="(r,i) in post.metrics.reactions ?? []" :key="i">{{ reactionLabel(r.key) }} {{ count(r.count) }}</span></div><details><summary>指标来源</summary><p v-for="(source,field) in post.provenance" :key="field">{{ field }} ← 消息 {{ source }}</p><small>同字段取最早的非空成员，计数不累加。</small></details></article>
        <section v-if="result?.posts.length" class="panel actions"><h3>手动转发</h3><p>已选 {{ selected.length }} 个帖子 · {{ selectedMessageCount }} 条成员消息</p><label>目标聊天<select v-model="target" @change="confirmation=false" :disabled="forwarding"><option value="">选择目标（提交时检查权限）</option><option v-for="chat in chatList" :key="chat.id" :value="chat.id">{{ chat.title }}</option></select></label><button :disabled="busy || forwarding || !ready" @click="savedMessages">使用 Saved Messages</button><button :disabled="!selected.length || !target || scanning || forwarding || busy" @click="prepareForward">准备转发</button>
          <div v-if="confirmation" class="confirm" role="dialog" aria-label="确认转发"><p>确认将 {{ selectedMessageCount }} 条成员消息转发到「{{ targetTitle }}」？受保护或权限不足的消息会被拒绝，逐条报告结果。</p><p v-for="item in preview?.items ?? []" :key="`${item.source.chat_id}:${item.source.message_id}`">{{ item.source.message_id }} — {{ item.state }} · {{ item.detail }}</p><button class="primary" :disabled="forwarding || !preview?.items.some(i=>i.state==='Allowed')" @click="forward">{{ forwarding ? '正在转发…' : '确认转发' }}</button><button :disabled="forwarding" @click="confirmation=false">返回</button></div>
          <div v-if="report"><h3>转发结果</h3><p v-for="item in report.items" :key="`${item.source.chat_id}:${item.source.message_id}`">{{ item.source.chat_id }} / {{ item.source.message_id }} — {{ item.state }} · {{ item.detail }}</p><small>Pending 仅表示 TDLib 接受请求；等待发送更新后才计为 Sent。</small></div>
        </section>
      </section>
    </div>
  </main>
</template>

<style>
:root{font-family:Inter,'Segoe UI','Microsoft YaHei',sans-serif;color:#213141;background:#eef3f7;font-synthesis:none;font-size:14px;line-height:1.6}*{box-sizing:border-box}body{margin:0}main{max-width:1600px;margin:auto;padding:24px}header{display:flex;justify-content:space-between;align-items:center;margin-bottom:20px}h1{font-size:28px;margin:0;letter-spacing:-1px}h2{font-size:18px;margin:0 0 14px}h3{font-size:15px;margin:18px 0 10px}header p{margin:0;color:#63778a}.connection{display:flex;flex-direction:column;text-align:right}.panel{background:#fff;border:1px solid #d9e3ec;border-radius:12px;padding:20px;box-shadow:0 3px 12px #19365005}.auth{margin-bottom:18px}summary{cursor:pointer;font-weight:600}.config-form{display:flex;flex-wrap:wrap;align-items:end;gap:12px}.config-form label{flex:1;min-width:150px}.auth-step{display:flex;gap:12px;align-items:end;margin:16px 0}label{display:flex;flex-direction:column;margin:10px 0;gap:4px}.check{display:flex;flex-direction:row;align-items:center;gap:8px}.check input{width:auto}.workspace{display:grid;grid-template-columns:240px 350px minmax(300px,1fr);gap:18px;align-items:start}input,select,button{font:inherit;padding:8px 10px;border:1px solid #cbd9e5;border-radius:6px;background:#fff;color:inherit;max-width:100%}input,select{width:100%;min-width:0}button{cursor:pointer;font-weight:600}button:hover:enabled{border-color:#307ab5;background:#edf6ff}button:disabled{opacity:.45;cursor:default}.primary{background:#246ea6;color:white;border-color:#246ea6}.primary:hover:enabled{background:#1c5c8d;color:white}.buttons{display:flex;gap:8px;flex-wrap:wrap;margin-top:12px}.chat{display:block;width:100%;text-align:left;margin-top:8px;padding:12px;border:1px solid transparent;background:#f4f7fa}.chat small{display:block;overflow-wrap:anywhere}.chat.chosen{border-color:#398cc5;background:#eaf5ff}small,.muted{color:#667d90}small{display:block;font-size:12px}.error{background:#ffebe9;color:#912a2a;padding:12px;border-radius:8px;overflow-wrap:anywhere}.notice{background:#e2f2ef;padding:10px;border-radius:8px}.filter-node{border:1px solid #dce5ee;border-radius:8px;padding:10px;display:flex;flex-wrap:wrap;gap:6px;background:#f8fafc}.filter-node>select{width:auto;max-width:150px}.filter-node>.short{width:95px}.filter-children{width:100%;padding-left:8px;border-left:2px solid #9cbbd2}.filter-row{display:flex;gap:5px;margin-bottom:8px;align-items:start}.filter-row>.filter-node{flex:1;min-width:0}.results{display:flex;flex-direction:column;gap:14px}.result-header p{margin:5px 0}.badge{font-size:12px;border-radius:20px;background:#e8f2fc;color:#22669a;padding:4px 10px;margin-left:8px}.badge.Complete{background:#dcf5e7;color:#1c7653}.badge.Partial,.badge.Cancelled{background:#fff0d1;color:#8c5f13}.badge.Error{background:#ffebe9;color:#912a2a}.post-top{display:flex;align-items:center;justify-content:space-between;gap:8px}.post-text{white-space:pre-wrap;overflow-wrap:anywhere}.media,.reactions,.metrics{display:flex;gap:10px;flex-wrap:wrap;margin:12px 0}.media-kind{background:#f0f4f8;color:#587187;border:1px dashed #bfd0dd;border-radius:6px;padding:8px}.metrics{padding:12px;background:#f7fafc;border-radius:8px}.metrics span{margin-right:10px}.reactions span{background:#edf4fa;padding:2px 8px;border-radius:12px}.post details{font-size:12px;color:#667d90}.post details p{margin:2px 0}.empty{text-align:center;padding:35px;color:#667d90}.actions>button{margin-right:8px;margin-top:8px}.actions p{overflow-wrap:anywhere}.confirm{background:#fff5e0;border-radius:8px;padding:14px;margin-top:14px}.confirm button{margin-right:8px}:focus-visible{outline:2px solid #328aca;outline-offset:2px}@media(max-width:1150px){.workspace{grid-template-columns:210px minmax(280px,1fr)}.results{grid-column:1/-1}}@media(max-width:650px){main{padding:12px}.workspace{grid-template-columns:1fr}.results{grid-column:auto}.connection{max-width:50%;font-size:11px;overflow-wrap:anywhere}.config-form{display:block}.post-top{flex-wrap:wrap}}
</style>
