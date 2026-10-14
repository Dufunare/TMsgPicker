
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { inject } from 'vue';
import type { Chat, Connection, ForwardReport, Post, QuerySpec, ScanResult, SettingsSnapshot, CloseChallenge, UnknownSend } from './types';
import {useApplicationSettings} from './useApplicationSettings.ts';
import { createAccountViews, type AccountView } from './accountViews.ts';

export function useWorkspace(settingsView = false, invokeCommand: typeof invoke = invoke, emitEvent: typeof emit = emit) {
const snapshot=ref<SettingsSnapshot|null>(null), profileId=ref(''), rememberDatabase=ref(false), leftRatio=ref(.23), rightRatio=ref(.29);
const accountViews = createAccountViews();
const statuses = accountViews.statuses;
const view = computed(() => accountViews.get(profileId.value));
function field<K extends keyof AccountView>(key: K) {
  return computed({ get: () => view.value[key], set: (value: AccountView[K]) => { view.value[key] = value; } });
}
const connection = computed<Connection>(() => statuses[profileId.value] ?? { profile_id:profileId.value||null, generation:0, runtime_state:'Closed', auth_state:'Disconnected', connection_state:'Disconnected', detail:profileId.value?'账号未启动':'尚未添加 Telegram 账号' });
const ready = computed(() => connection.value.runtime_state === 'Ready' && connection.value.auth_state === 'authorizationStateReady');
const busy=field('busy'), error=field('error'), chatError=field('chatError'), accountError=field('accountError'), notice=field('notice');
let sessionToken=0, chatTimer:ReturnType<typeof setTimeout>|undefined;
const dialog=ref<{title:string;text:string;input?:string;allowDelete?:boolean;phrase?:string}|null>(null);
let dialogResolve:((value:{text:string;deleteData:boolean}|null)=>void)|null=null;
function askDialog(options:NonNullable<typeof dialog.value>){if(dialog.value)return Promise.resolve(null);return new Promise<{text:string;deleteData:boolean}|null>(resolve=>{dialog.value=options;dialogResolve=resolve;});}
function answerDialog(value:{text:string;deleteData:boolean}|null){dialog.value=null;dialogResolve?.(value);dialogResolve=null;}

const passphrase=ref(''),authValue=ref('');
const {apiId,apiHash,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,applySettings,saveSettings,clearSecret,debugReset}=useApplicationSettings(snapshot,invokeCommand,accountAction,confirmPendingClose,askDialog,notice);

const chatList = field('chatList');
const search = field('search');
const selectedChat = field('selectedChat');
const scopeKind = field('scopeKind');
const rawCount = field('rawCount');
const fromDate = field('fromDate');
const toDate = field('toDate');
const budget = field('budget');
const filter = field('filter');
const sortField = field('sortField');
const sortEmoji = field('sortEmoji');
const descending = field('descending');
const outputLimit = field('outputLimit');
const result = field('result');
const currentRun = field('currentRun');
const scanning = field('scanning');
const selected = field('selected');
const target = field('target');
const confirmation = field('confirmation');
const forwarding = field('forwarding');
const report = field('report');
const preview = field('preview');
const reportTarget = field('reportTarget');
const uncertainSends = field('uncertainSends');
const targetTitle = computed(() => chatList.value.find(c => c.id === target.value)?.title ?? '');
const selectedMessageCount = computed(() => result.value?.posts.filter(p => selected.value.includes(p.key)).reduce((n,p) => n+p.members.length,0) ?? 0);
const authLabel = computed(() => ({ authorizationStateWaitPhoneNumber: '手机号（含国家代码）', authorizationStateWaitCode: 'Telegram 验证码', authorizationStateWaitPassword: '两步验证密码', authorizationStateWaitEmailAddress: '授权邮箱', authorizationStateWaitEmailCode: '邮箱验证码' } as Record<string,string>)[connection.value.auth_state]);
const unlisteners: UnlistenFn[] = [];

async function action(fn: () => Promise<void>) { const v=view.value;v.error='';v.busy=true;try{await fn();}catch(e){v.error=String(e);}finally{v.busy=false;} }
function clearSession(id=profileId.value) { accountViews.reset(id);if(id===profileId.value){sessionToken++;authValue.value='';} }
async function accountAction(fn:()=>Promise<void|boolean>){const id=profileId.value,v=view.value;v.accountError='';v.busy=true;try{const changed=await fn();if(changed!==false)await emitEvent('workspace-settings-changed',{source:settingsView?'settings':'main'});}catch(e){accountViews.get(id).accountError=String(e);}finally{v.busy=false;}}
async function refreshSettings(){snapshot.value=await invokeCommand<SettingsSnapshot>('app_settings');if(!settingsView&&!snapshot.value.profiles.some(p=>p.profile_id===profileId.value))profileId.value=snapshot.value.primary_profile_id??'';const states=await invokeCommand<Connection[]>('account_statuses');states.forEach(accountViews.connection);const unknown=await invokeCommand<UnknownSend[]>('unknown_sends');unknown.forEach(accountViews.sent);}
async function confirmPendingClose(profileId:string|null, initial?:CloseChallenge):Promise<string|null|undefined>{
  const challenge=initial??await invokeCommand<CloseChallenge>('close_guard',{profileId});
  const scope=profileId===null?'全部账号':`账号「${snapshot.value?.profiles.find(p=>p.profile_id===profileId)?.display_name??profileId}」`;
  if(!challenge.pending_count)return null;
  if(challenge.retry_after_ms>0){await askDialog({title:'等待发送回执',text:`尚有 ${challenge.pending_count} 条 Pending 转发。请等待约 ${Math.ceil(challenge.retry_after_ms/1000)} 秒后重试；届时可以确认安全关闭并将未确认结果记为 Unknown。本次不会关闭客户端。`});return undefined;}
  if(!challenge.token)throw new Error('关闭确认已失效，请重试');
  const answer=await askDialog({title:'确认关闭未完成的发送',text:`${challenge.pending_count} 条转发已超过等待时间。确认安全关闭${scope}并将未确认结果记为 Unknown（送达状态未知）？消息可能已经发送，请先在 Telegram 检查，避免重复转发。不会退出 Telegram 登录；这些未确认记录仅在本进程保留。`});
  return answer?challenge.token:undefined;
}
async function connect() {
  const id=profileId.value;
  if(!id){accountError.value='请添加或选择账号';return;}
  const config={profile_id:id,passphrase:passphrase.value||null,remember_database:rememberDatabase.value};
  passphrase.value='';
  await accountAction(async()=>{await invokeCommand('connect',{config});await refreshSettings();});
  if(profileId.value===id&&!settingsView&&ready.value)scheduleChats();
}
async function submitAuth(){const id=profileId.value;const value=authValue.value;authValue.value='';await accountAction(async()=>{await invokeCommand('auth_submit',{profileId:id,value});});}
async function disconnect(){const id=profileId.value;await accountAction(async()=>{const closeConfirmation=await confirmPendingClose(id);if(closeConfirmation===undefined)return false;await invokeCommand('disconnect',{profileId:id,closeConfirmation});clearSession(id);});}
async function addAccount(){try{await invokeCommand('open_login');}catch(e){accountError.value=String(e);}}
async function selectAccount(_activate=false){
  sessionToken++; clearTimeout(chatTimer); passphrase.value='';authValue.value='';
  rememberDatabase.value=snapshot.value?.profiles.find(p=>p.profile_id===profileId.value)?.has_saved_database_secret??false;
  if(profileId.value&&!settingsView)await invokeCommand('select_profile',{profileId:profileId.value});
  if(ready.value&&!settingsView&&!chatList.value.length)scheduleChats();
}
async function openSettings(){try{await invokeCommand('open_settings',{profileId:null});}catch(e){accountError.value=String(e);}}
async function openAccountSettings(){const id=profileId.value;if(!id)return;try{await invokeCommand('open_settings',{profileId:id});}catch(e){accountError.value=String(e);}}
async function connectFromCard(){if(rememberDatabase.value)await connect();else await openAccountSettings();}
async function logoutAccount(){const id=profileId.value;const profile=snapshot.value?.profiles.find(p=>p.profile_id===id);if(!id||!await askDialog({title:'注销账号',text:`注销「${profile?.display_name}」的 Telegram 授权，并永久清除该账号的本地 Session、文件和保存的数据库口令？其他账号保持运行。`}))return;await accountAction(async()=>{await invokeCommand('logout_account',{profileId:id,confirmed:true});clearSession(id);await refreshSettings();});}
function resizeLayout(left:number,right:number){leftRatio.value=left;rightRatio.value=right;}
function resetLayout(){resizeLayout(.23,.29);}
async function chatAction(fn:()=>Promise<void>){const v=view.value;v.chatError='';v.busy=true;try{await fn();}catch(e){v.chatError=String(e);}finally{v.busy=false;}}
function scheduleChats(attempt=0){
  if(settingsView)return;clearTimeout(chatTimer);const token=sessionToken;
  chatTimer=setTimeout(async()=>{
    if(token!==sessionToken||!ready.value)return;
    const id=profileId.value,v=view.value,generation=connection.value.generation;
    try{const list=await invokeCommand<Chat[]>('chats',{profileId:id,search:v.search});if(statuses[id]?.generation===generation){v.chatList=list;v.chatError='';}}
    catch(e){if(String(e).includes('Session operation in progress')&&attempt<5&&profileId.value===id)scheduleChats(attempt+1);else v.chatError=String(e);}
  },150);
}
async function loadChats() {const id=profileId.value,v=view.value;await chatAction(async()=>{const list=await invokeCommand<Chat[]>('chats',{profileId:id,search:v.search});if(accountViews.views[id]===v)v.chatList=list;});}
async function resolvePublic() {const id=profileId.value,v=view.value;await chatAction(async()=>{const c=await invokeCommand<Chat>('public_chat',{profileId:id,username:v.search});if(accountViews.views[id]!==v)return;if(!v.chatList.some(x=>x.id===c.id))v.chatList.unshift(c);v.selectedChat=c;});}
async function savedMessages() {const id=profileId.value,v=view.value;await action(async()=>{const c=await invokeCommand<Chat>('saved_messages',{profileId:id});if(accountViews.views[id]!==v)return;if(!v.chatList.some(x=>x.id===c.id))v.chatList.push(c);v.target=c.id;v.confirmation=false;v.confirmedSelection=null;});}
function preset() {filter.value={kind:'not',item:{kind:'and',items:[{kind:'is_null',field:{kind:'reply_info'}},{kind:'compare',field:{kind:'post_kind'},op:'eq',value:'single'}]}};}
async function runQuery() {
  const id=profileId.value,v=view.value;
  v.error='';if(!v.selectedChat){v.error='请先选择一个聊天';return;}
  const from=Date.parse(v.fromDate)/1000,to=Date.parse(v.toDate)/1000;
  if(v.scopeKind==='dates'&&(!Number.isFinite(from)||!Number.isFinite(to)||from>=to)){v.error='请输入有效的开始／结束日期';return;}
  const spec:QuerySpec={version:1,chat_id:v.selectedChat.id,scope:v.scopeKind==='recent'?{kind:'recent',count:v.rawCount}:{kind:'dates',from:Math.floor(from),to:Math.floor(to),budget:v.budget},filter:v.filter,sort:[{field:{kind:v.sortField,...(v.sortField==='reaction'?{emoji:v.sortEmoji}:{})},descending:v.descending}],output_limit:v.outputLimit};
  const runId=crypto.randomUUID();v.currentRun=runId;v.scanning=true;v.selected=[];v.confirmation=false;v.report=null;v.confirmedSelection=null;
  v.result={run_id:runId,status:'Scanning',scanned:0,pages:0,exact:false,detail:'正在读取历史',posts:[]};
  try{await invokeCommand('start_query',{profileId:id,spec,runId});}catch(e){if(v.currentRun===runId){v.scanning=false;v.error=String(e);v.result={...v.result!,status:'Error',detail:String(e)};}}
}
async function cancelQuery(){const id=profileId.value,runId=currentRun.value;await action(async()=>{await invokeCommand('cancel_query',{profileId:id,runId});});}
async function openPost(post:Post){const id=profileId.value;await action(async()=>{const url=await invokeCommand<string>('message_link',{profileId:id,key:post.representative,album:post.kind==='album'});await openUrl(url);});}
async function prepareForward(){
  const id=profileId.value,v=view.value,selection={target:v.target,keys:[...v.selected],runId:v.currentRun};
  await action(async()=>{
    const report=await invokeCommand<ForwardReport>('preview_forward',{profileId:id,target:selection.target,postKeys:selection.keys,runId:selection.runId});
    if(accountViews.views[id]!==v||selection.target!==v.target||selection.runId!==v.currentRun||selection.keys.join('|')!==v.selected.join('|'))return;
    v.preview=report;v.confirmedSelection=selection;v.confirmation=true;
  });
}
async function forward(){
  const id=profileId.value,v=view.value,selection=v.confirmedSelection;if(!selection)return;
  v.forwarding=true;v.error='';
  try{
    v.reportTarget=selection.target;
    const report=await invokeCommand<ForwardReport>('forward_selected',{profileId:id,target:selection.target,postKeys:selection.keys,runId:selection.runId,confirmed:true});
    if(accountViews.views[id]!==v)return;
    v.report=report;
    for(const item of report.items)if(item.sent_id){const state=v.sendUpdates[`${selection.target}:${item.sent_id}`];if(state)item.state=state;}
    v.confirmation=false;
  }catch(e){v.error=String(e);}finally{v.forwarding=false;}
}
function count(v: number | null | undefined) {return v == null ? '—' : v.toLocaleString();}
function reactionLabel(key: {kind:string;value?:string}) {return key.kind==='emoji'?key.value:key.kind==='custom_emoji'?`自定义 Emoji ${key.value}`:key.kind==='paid'?'付费反应':`其他反应 ${key.value ?? ''}`;}
function date(seconds:number) {return new Date(seconds*1000).toLocaleString();}
onMounted(async () => {
  if(!isTauri()) {accountError.value='请通过 pnpm tauri dev 启动桌面程序；浏览器预览无法连接 TDLib。';return;}
  try {
    unlisteners.push(await listen<Connection>('connection',e=>{
      const previous=e.payload.profile_id?statuses[e.payload.profile_id]:undefined;
      if(!accountViews.connection(e.payload))return;
      if(!settingsView&&e.payload.profile_id===profileId.value&&e.payload.runtime_state==='Ready'&&previous?.runtime_state!=='Ready')scheduleChats();
    }));
    unlisteners.push(await listen<{profile_id:string;generation:number;result:ScanResult}>('query-progress',e=>accountViews.progress(e.payload)));
    unlisteners.push(await listen<string>('app-error',e=>{accountError.value=e.payload;}));
    if(!settingsView)unlisteners.push(await listen<CloseChallenge>('close-confirmation-required',async e=>{
      if(dialog.value)return;
      try{const closeConfirmation=await confirmPendingClose(null,e.payload);if(closeConfirmation!==undefined)await invokeCommand('exit_application',{closeConfirmation});}catch(error){accountError.value=String(error);}
    }));
    unlisteners.push(await listen<{source:string}>('workspace-settings-changed',async e=>{
      if(e.payload.source===(settingsView?'settings':'main'))return;
      try{await refreshSettings();if(!settingsView){applySettings();void selectAccount();if(ready.value&&!chatList.value.length)scheduleChats();}}catch(e){accountError.value=String(e);}
    }));
    unlisteners.push(await listen('accounts-changed' ,()=>{refreshSettings().then(()=>{if(!settingsView)void selectAccount();}).catch(e=>{accountError.value=String(e);});}));
    if(!settingsView)unlisteners.push(await listen<string>('login-completed',async e=>{try{await refreshSettings();if(snapshot.value?.profiles.some(p=>p.profile_id===e.payload)){profileId.value=e.payload;await selectAccount();scheduleChats();}}catch(e){accountError.value=String(e);}}));
    unlisteners.push(await listen<{profile_id:string;detail:string}>('account-notice',e=>{accountViews.get(e.payload.profile_id).notice=e.payload.detail;}));
    unlisteners.push(await listen<{profile_id:string;target:string;old_message_id:string;state:string;generation:number}>('forward-update',e=>accountViews.sent(e.payload)));
    await refreshSettings();applySettings();const requested=settingsView?new URLSearchParams(window.location.search).get('profile'):null;profileId.value=settingsView?(requested??''):(snapshot.value?.primary_profile_id??snapshot.value?.profiles[0]?.profile_id??'');await selectAccount();if(ready.value&&!settingsView)void loadChats();
  } catch(e) {accountError.value=String(e);}
});
onUnmounted(()=>{unlisteners.forEach(fn=>fn());clearTimeout(chatTimer);answerDialog(null);});

return { dialog,answerDialog,connection,statuses,ready,busy,error,accountError,chatError,notice,apiId,apiHash,passphrase,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,authValue,chatList,search,selectedChat,scopeKind,rawCount,fromDate,toDate,budget,filter,sortField,sortEmoji,descending,outputLimit,result,currentRun,scanning,selected,target,confirmation,forwarding,report,preview,reportTarget,uncertainSends,targetTitle,selectedMessageCount,authLabel,connect,submitAuth,disconnect,loadChats,resolvePublic,savedMessages,preset,runQuery,cancelQuery,openPost,prepareForward,forward,count,reactionLabel,date,snapshot,profileId,openSettings,openAccountSettings,connectFromCard,leftRatio,rightRatio,addAccount,selectAccount,logoutAccount,saveSettings,clearSecret,debugReset,resetLayout,resizeLayout };
}
export type Workspace = ReturnType<typeof useWorkspace>;
export const workspaceKey=Symbol("workspace");
export function useWorkspaceContext(){return inject<Workspace>(workspaceKey)!;}
