
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { inject } from 'vue';
import type { Chat, Connection, FieldName, Filter, ForwardReport, Post, QuerySpec, ScanResult, SettingsSnapshot } from './types';

export function useWorkspace() {
const connection = ref<Connection>({ profile_id:null, generation:0, auth_state: 'Disconnected', connection_state: 'Disconnected', detail: '请输入配置并连接' });
const ready = computed(() => connection.value.auth_state === 'authorizationStateReady');
const busy = ref(false), error = ref(''), accountError = ref(''), chatError = ref(''), notice = ref('');
const snapshot=ref<SettingsSnapshot|null>(null), profileId=ref(''), settingsOpen=ref(false), rememberDatabase=ref(false), leftRatio=ref(.23), rightRatio=ref(.29);
let sessionToken=0, layoutTimer:ReturnType<typeof setTimeout>|undefined;
const dialog=ref<{title:string;text:string;input?:string;allowDelete?:boolean}|null>(null);
let dialogResolve:((value:{text:string;deleteData:boolean}|null)=>void)|null=null;
function askDialog(options:NonNullable<typeof dialog.value>){return new Promise<{text:string;deleteData:boolean}|null>(resolve=>{dialog.value=options;dialogResolve=resolve;});}
function answerDialog(value:{text:string;deleteData:boolean}|null){dialog.value=null;dialogResolve?.(value);dialogResolve=null;}

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
function clearSession() { sessionToken++; currentRun.value=''; scanning.value=false;chatList.value=[];selectedChat.value=null;result.value=null;selected.value=[];target.value='';confirmation.value=false;confirmedSelection=null;preview.value=null;report.value=null;reportTarget.value='';sendUpdates.clear();authValue.value=''; }
async function accountAction(fn:()=>Promise<void>){accountError.value='';busy.value=true;try{await fn();}catch(e){accountError.value=String(e);}finally{busy.value=false;}}
async function refreshSettings(){snapshot.value=await invoke<SettingsSnapshot>('app_settings');}
async function connect() {
  if(!profileId.value){accountError.value='请添加或选择账号';return;}
  if(report.value?.items.some(i=>i.state==='Pending')){accountError.value='仍有 Pending 发送，请等待最终状态';return;}
  clearSession();
  await accountAction(async()=>{await invoke('connect',{config:{profile_id:profileId.value,passphrase:passphrase.value||null,remember_database:rememberDatabase.value}});await refreshSettings();});
  passphrase.value='';
}
async function submitAuth(){const value=authValue.value;authValue.value='';await accountAction(async()=>{await invoke('auth_submit',{value});});}
async function disconnect(){await accountAction(async()=>{await invoke('disconnect');clearSession();});}
async function addAccount(){await accountAction(async()=>{profileId.value=await invoke<string>('add_account',{name:'新账号'});rememberDatabase.value=false;await refreshSettings();});await selectAccount(true);}
async function selectAccount(activate=false){
  rememberDatabase.value=snapshot.value?.profiles.find(p=>p.profile_id===profileId.value)?.has_saved_database_secret??false;passphrase.value='';
  if(!activate || profileId.value===connection.value.profile_id)return;
  if(report.value?.items.some(i=>i.state==='Pending')||forwarding.value){profileId.value=connection.value.profile_id??'';accountError.value='仍有未确认的发送，暂时不能切换账号';return;}
  if(rememberDatabase.value){await connect();return;}
  if(connection.value.auth_state!=='Disconnected'){
    await accountAction(async()=>{await invoke('disconnect');clearSession();});
    if(accountError.value){profileId.value=connection.value.profile_id??'';return;}
  }else clearSession();
}
async function renameAccount(){const id=profileId.value;const answer=await askDialog({title:'账号重命名',text:'此名称仅在本机使用。',input:snapshot.value?.profiles.find(p=>p.profile_id===id)?.display_name??''});if(!answer?.text.trim())return;await accountAction(async()=>{snapshot.value=await invoke('rename_account',{profileId:id,name:answer.text.trim()});});}
async function removeAccount(){const id=profileId.value;const profile=snapshot.value?.profiles.find(p=>p.profile_id===id);const answer=await askDialog({title:'移除本地账号',text:`移除「${profile?.display_name}」的 Profile 和保存口令。默认保留 Session 文件；此操作不执行 Telegram 登出。`,allowDelete:!profile?.is_legacy_profile});if(!answer)return;let deleteConfirmed=false;if(answer.deleteData){deleteConfirmed=!!await askDialog({title:'永久删除本地 Session',text:`再次确认删除「${profile?.display_name}」的数据库与下载文件？此操作不可撤销；再次使用需要登录。`});if(!deleteConfirmed)return;}await accountAction(async()=>{snapshot.value=await invoke('remove_account',{profileId:id,confirmed:true,deleteData:answer.deleteData,deleteConfirmed});profileId.value=snapshot.value?.settings.last_profile_id??'';selectAccount();});}
async function logoutAccount(){const id=connection.value.profile_id;const profile=snapshot.value?.profiles.find(p=>p.profile_id===id);if(!id||!await askDialog({title:'退出 Telegram 登录',text:`退出「${profile?.display_name}」？TDLib 将清除该授权的本地数据，之后需要重新登录。其他 Profile 不受影响。`}))return;await accountAction(async()=>{await invoke('logout_account',{profileId:id,confirmed:true});clearSession();});}
async function saveSettings(){await accountAction(async()=>{if(!snapshot.value)return;snapshot.value=await invoke('save_settings',{edit:{settings:{...snapshot.value.settings,api_id:apiId.value?Number(apiId.value):null,proxy_enabled:proxyEnabled.value,proxy_server:proxyHost.value,proxy_port:proxyPort.value,proxy_username:proxyUser.value},api_hash:apiHash.value||null,proxy_password:proxyPassword.value||null}});notice.value='配置已保存；连接时应用。';});apiHash.value='';proxyPassword.value='';}
async function clearSecret(key:string){await accountAction(async()=>{snapshot.value=await invoke('clear_secret',{key});if(key.startsWith('db:'))rememberDatabase.value=false;if(key==='proxy')proxyUser.value='';notice.value='?????????????????????????????';});}
async function resetSettings(){await accountAction(async()=>{snapshot.value=await invoke('reset_settings');applySettings();});}
function applySettings(){const s=snapshot.value?.settings;if(!s)return;apiId.value=s.api_id?.toString()??'';proxyEnabled.value=s.proxy_enabled;proxyHost.value=s.proxy_server;proxyPort.value=s.proxy_port;proxyUser.value=s.proxy_username;leftRatio.value=s.left_ratio;rightRatio.value=s.right_ratio;}
function resizeLayout(left:number,right:number){leftRatio.value=left;rightRatio.value=right;if(snapshot.value){snapshot.value.settings.left_ratio=left;snapshot.value.settings.right_ratio=right;}clearTimeout(layoutTimer);layoutTimer=setTimeout(()=>{invoke('save_layout',{left,right}).catch(e=>{accountError.value=String(e);});},250);}
function resetLayout(){resizeLayout(.23,.29);}
async function chatAction(fn:()=>Promise<void>){chatError.value='';busy.value=true;try{await fn();}catch(e){chatError.value=String(e);}finally{busy.value=false;}}
async function loadChats() { await chatAction(async () => {const token=sessionToken;const list=await invoke<Chat[]>('chats',{search:search.value});if(token===sessionToken)chatList.value=list;}); }
async function resolvePublic() { await chatAction(async () => {const token=sessionToken;const c=await invoke<Chat>('public_chat',{username:search.value}); if(token!==sessionToken)return; if (!chatList.value.some(x=>x.id===c.id)) chatList.value.unshift(c); selectedChat.value=c;}); }
async function savedMessages() { await action(async () => {const token=sessionToken;const c=await invoke<Chat>('saved_messages');if(token!==sessionToken)return;if (!chatList.value.some(x=>x.id===c.id)) chatList.value.push(c);target.value=c.id;confirmation.value=false;confirmedSelection=null;}); }
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
  if(!isTauri()) {accountError.value='请通过 pnpm tauri dev 启动桌面程序；浏览器预览无法连接 TDLib。';return;}
  try {
    unlisteners.push(await listen<Connection>('connection',e=>{if(e.payload.generation<connection.value.generation)return;if(e.payload.generation!==connection.value.generation)clearSession();connection.value=e.payload;}));
    unlisteners.push(await listen<ScanResult>('query-progress',e=>{if(e.payload.run_id!==currentRun.value)return;result.value=e.payload;scanning.value=e.payload.status==='Scanning';selected.value=selected.value.filter(key=>e.payload.posts.some(p=>p.key===key));}));
    unlisteners.push(await listen<string>('app-error',e=>{accountError.value=e.payload;}));
    unlisteners.push(await listen('accounts-changed',()=>{refreshSettings().catch(e=>{accountError.value=String(e);});}));
    unlisteners.push(await listen<string>('account-notice',e=>{notice.value=e.payload;}));
    unlisteners.push(await listen<{target:string;old_message_id:string;state:string;generation:number}>('forward-update',e=>{if(e.payload.generation!==connection.value.generation)return;sendUpdates.set(`${e.payload.target}:${e.payload.old_message_id}`,e.payload.state);if(e.payload.target===reportTarget.value)for(const item of report.value?.items ?? [])if(item.sent_id===e.payload.old_message_id)item.state=e.payload.state;}));
    connection.value=await invoke<Connection>('connection_status');
    await refreshSettings();applySettings();profileId.value=snapshot.value?.settings.last_profile_id??snapshot.value?.profiles[0]?.profile_id??'';selectAccount();
  } catch(e) {accountError.value=String(e);}
});
onUnmounted(()=>{unlisteners.forEach(fn=>fn());clearTimeout(layoutTimer);answerDialog(null);});

return { dialog,answerDialog,connection,ready,busy,error,accountError,chatError,notice,apiId,apiHash,passphrase,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,authValue,chatList,search,selectedChat,scopeKind,rawCount,fromDate,toDate,budget,filter,sortField,sortEmoji,descending,outputLimit,result,currentRun,scanning,selected,target,confirmation,forwarding,report,preview,reportTarget,targetTitle,selectedMessageCount,authLabel,connect,submitAuth,disconnect,loadChats,resolvePublic,savedMessages,preset,runQuery,cancelQuery,openPost,prepareForward,forward,count,reactionLabel,date,snapshot,profileId,settingsOpen,rememberDatabase,leftRatio,rightRatio,addAccount,selectAccount,renameAccount,removeAccount,logoutAccount,saveSettings,clearSecret,resetSettings,resetLayout,resizeLayout };
}
export type Workspace = ReturnType<typeof useWorkspace>;
export const workspaceKey=Symbol("workspace");
export function useWorkspaceContext(){return inject<Workspace>(workspaceKey)!;}
