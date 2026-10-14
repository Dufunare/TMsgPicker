
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { inject } from 'vue';
import type { Chat, Connection, ForwardReport, Post, QuerySpec, ScanResult, SettingsSnapshot, CloseChallenge, UnknownSend } from './types';
import { createAccountViews, type AccountView } from './accountViews.ts';

export function useWorkspace(settingsView = false, invokeCommand: typeof invoke = invoke, emitEvent: typeof emit = emit) {
const snapshot=ref<SettingsSnapshot|null>(null), profileId=ref(''), cardCompact=ref(false), rememberDatabase=ref(false), leftRatio=ref(.23), rightRatio=ref(.29);
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
const dialog=ref<{title:string;text:string;input?:string;allowDelete?:boolean}|null>(null);
let dialogResolve:((value:{text:string;deleteData:boolean}|null)=>void)|null=null;
function askDialog(options:NonNullable<typeof dialog.value>){return new Promise<{text:string;deleteData:boolean}|null>(resolve=>{dialog.value=options;dialogResolve=resolve;});}
function answerDialog(value:{text:string;deleteData:boolean}|null){dialog.value=null;dialogResolve?.(value);dialogResolve=null;}

const apiId = ref(''), apiHash = ref(''), passphrase = ref(''), proxyEnabled = ref(false), proxyHost = ref('127.0.0.1'), proxyPort = ref(1080), proxyUser = ref(''), proxyPassword = ref(''), authValue = ref('');
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
async function refreshSettings(){snapshot.value=await invokeCommand<SettingsSnapshot>('app_settings');const states=await invokeCommand<Connection[]>('account_statuses');states.forEach(accountViews.connection);const unknown=await invokeCommand<UnknownSend[]>('unknown_sends');unknown.forEach(accountViews.sent);}
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
async function addAccount(){await accountAction(async()=>{profileId.value=await invokeCommand<string>('add_account',{name:'新账号'});rememberDatabase.value=false;await refreshSettings();});await selectAccount();if(!settingsView)await openSettings();}
async function selectAccount(_activate=false){
  sessionToken++; clearTimeout(chatTimer); passphrase.value='';authValue.value='';
  rememberDatabase.value=snapshot.value?.profiles.find(p=>p.profile_id===profileId.value)?.has_saved_database_secret??false;
  if(profileId.value&&!settingsView)await invokeCommand('select_profile',{profileId:profileId.value});
  if(ready.value&&!settingsView&&!chatList.value.length)scheduleChats();
}
async function openSettings(){try{await invokeCommand('open_settings',{profileId:profileId.value||null});}catch(e){accountError.value=String(e);}}
async function closeSettings(){try{await invokeCommand('close_settings');}catch(e){accountError.value=String(e);}}
async function connectFromCard(){if(rememberDatabase.value&&!authLabel.value)await connect();else await openSettings();}
async function toggleCard(){const compact=!cardCompact.value;try{await invokeCommand('save_ui_preferences',{cardCompact:compact});cardCompact.value=compact;if(snapshot.value)snapshot.value.settings.card_compact=compact;await emitEvent('workspace-settings-changed',{source:settingsView?'settings':'main'});}catch(e){accountError.value=String(e);}}

async function renameAccount(){const id=profileId.value;const answer=await askDialog({title:'账号重命名',text:'此名称仅在本机使用。',input:snapshot.value?.profiles.find(p=>p.profile_id===id)?.display_name??''});if(!answer?.text.trim())return;await accountAction(async()=>{snapshot.value=await invokeCommand('rename_account',{profileId:id,name:answer.text.trim()});});}
async function removeAccount(){const id=profileId.value;const profile=snapshot.value?.profiles.find(p=>p.profile_id===id);const answer=await askDialog({title:'移除本地账号',text:`移除「${profile?.display_name}」的 Profile 和保存口令。默认保留 Session 文件；此操作不执行 Telegram 登出。`,allowDelete:!profile?.is_legacy_profile});if(!answer)return;let deleteConfirmed=false;if(answer.deleteData){deleteConfirmed=!!await askDialog({title:'永久删除本地 Session',text:`再次确认删除「${profile?.display_name}」的数据库与下载文件？此操作不可撤销；再次使用需要登录。`});if(!deleteConfirmed)return;}await accountAction(async()=>{snapshot.value=await invokeCommand('remove_account',{profileId:id,confirmed:true,deleteData:answer.deleteData,deleteConfirmed});profileId.value=snapshot.value?.settings.last_profile_id??'';selectAccount();});}
async function logoutAccount(){const id=profileId.value;const profile=snapshot.value?.profiles.find(p=>p.profile_id===id);if(!id||!await askDialog({title:'退出 Telegram 登录',text:`退出「${profile?.display_name}」？TDLib 将清除该授权的本地数据，之后需要重新登录。其他 Profile 不受影响。`}))return;await accountAction(async()=>{await invokeCommand('logout_account',{profileId:id,confirmed:true});clearSession(id);});}
async function saveSettings(){let saved=false;await accountAction(async()=>{if(!snapshot.value)return;const closeConfirmation=await confirmPendingClose(null);if(closeConfirmation===undefined)return false;const latest=await invokeCommand<SettingsSnapshot>('app_settings');snapshot.value=await invokeCommand('save_settings',{closeConfirmation,edit:{settings:{...latest.settings,api_id:apiId.value?Number(apiId.value):null,proxy_enabled:proxyEnabled.value,proxy_server:proxyHost.value,proxy_port:proxyPort.value,proxy_username:proxyUser.value},api_hash:apiHash.value||null,proxy_password:proxyPassword.value||null}});saved=true;notice.value='全局配置已保存；全部账号已安全重建，请查看各账号状态。';});if(saved){apiHash.value='';proxyPassword.value='';}}
async function clearSecret(key:string){await accountAction(async()=>{const closeConfirmation=key.startsWith('db:')?null:await confirmPendingClose(null);if(closeConfirmation===undefined)return false;snapshot.value=await invokeCommand('clear_secret',{key,closeConfirmation});if(key===`db:${profileId.value}`)rememberDatabase.value=false;if(key==='proxy')proxyUser.value='';notice.value=key.startsWith('db:')?'此账号保存的口令已清除；当前会话保持运行。':'全局凭据已清除；全部账号已安全重建。';});}
async function resetSettings(){await accountAction(async()=>{const closeConfirmation=await confirmPendingClose(null);if(closeConfirmation===undefined)return false;snapshot.value=await invokeCommand('reset_settings',{closeConfirmation});applySettings();});}
function applySettings(){const s=snapshot.value?.settings;if(!s)return;apiId.value=s.api_id?.toString()??'';proxyEnabled.value=s.proxy_enabled;proxyHost.value=s.proxy_server;proxyPort.value=s.proxy_port;proxyUser.value=s.proxy_username;leftRatio.value=s.left_ratio;rightRatio.value=s.right_ratio;cardCompact.value=s.card_compact;}
async function persistLayout(){try{await invokeCommand('save_layout',{left:leftRatio.value,right:rightRatio.value});await emitEvent('workspace-settings-changed',{source:settingsView?'settings':'main'});}catch(e){accountError.value=String(e);}}
function resizeLayout(left:number,right:number){leftRatio.value=left;rightRatio.value=right;if(snapshot.value){snapshot.value.settings.left_ratio=left;snapshot.value.settings.right_ratio=right;}}
function resetLayout(){resizeLayout(.23,.29);void persistLayout();}
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
      try{await refreshSettings();cardCompact.value=snapshot.value?.settings.card_compact??false;leftRatio.value=snapshot.value?.settings.left_ratio??leftRatio.value;rightRatio.value=snapshot.value?.settings.right_ratio??rightRatio.value;if(!settingsView){applySettings();if(!snapshot.value?.profiles.some(p=>p.profile_id===profileId.value)){profileId.value=snapshot.value?.settings.last_profile_id??'';}void selectAccount();if(ready.value&&!chatList.value.length)scheduleChats();}}catch(e){accountError.value=String(e);}
    }));
    if(settingsView)unlisteners.push(await listen<string|null>('settings-focus-profile',async e=>{
      try{await refreshSettings();if(e.payload&&profileId.value!==e.payload&&snapshot.value?.profiles.some(p=>p.profile_id===e.payload)){profileId.value=e.payload;void selectAccount();}}catch(e){accountError.value=String(e);}
    }));
    unlisteners.push(await listen('accounts-changed' ,()=>{refreshSettings().catch(e=>{accountError.value=String(e);});}));
    unlisteners.push(await listen<{profile_id:string;detail:string}>('account-notice',e=>{accountViews.get(e.payload.profile_id).notice=e.payload.detail;}));
    unlisteners.push(await listen<{profile_id:string;target:string;old_message_id:string;state:string;generation:number}>('forward-update',e=>accountViews.sent(e.payload)));
    await refreshSettings();applySettings();const requested=settingsView?new URLSearchParams(window.location.search).get('profile'):null;profileId.value=(requested&&snapshot.value?.profiles.some(p=>p.profile_id===requested)?requested:null)??connection.value.profile_id??snapshot.value?.settings.last_profile_id??snapshot.value?.profiles[0]?.profile_id??'';await selectAccount();if(ready.value&&!settingsView)void loadChats();
  } catch(e) {accountError.value=String(e);}
});
onUnmounted(()=>{unlisteners.forEach(fn=>fn());clearTimeout(chatTimer);answerDialog(null);});

return { dialog,answerDialog,connection,statuses,ready,busy,error,accountError,chatError,notice,apiId,apiHash,passphrase,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,authValue,chatList,search,selectedChat,scopeKind,rawCount,fromDate,toDate,budget,filter,sortField,sortEmoji,descending,outputLimit,result,currentRun,scanning,selected,target,confirmation,forwarding,report,preview,reportTarget,uncertainSends,targetTitle,selectedMessageCount,authLabel,connect,submitAuth,disconnect,loadChats,resolvePublic,savedMessages,preset,runQuery,cancelQuery,openPost,prepareForward,forward,count,reactionLabel,date,snapshot,profileId,cardCompact,openSettings,closeSettings,connectFromCard,toggleCard,persistLayout,rememberDatabase,leftRatio,rightRatio,addAccount,selectAccount,renameAccount,removeAccount,logoutAccount,saveSettings,clearSecret,resetSettings,resetLayout,resizeLayout };
}
export type Workspace = ReturnType<typeof useWorkspace>;
export const workspaceKey=Symbol("workspace");
export function useWorkspaceContext(){return inject<Workspace>(workspaceKey)!;}
