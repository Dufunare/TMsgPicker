import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRenderer, h } from 'vue';
import { createAccountViews } from '../src/accountViews.ts';
import { useWorkspace } from '../src/useWorkspace.ts';
import {createSSRApp} from 'vue';
import type {SettingsSnapshot} from '../src/types.ts';
import {renderToString} from 'vue/server-renderer';

const fixture:SettingsSnapshot={settings:{schema_version:1,api_id:12,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},profiles:['A','B'].map(id=>({profile_id:id,telegram_user_id:id,display_name:id,username:null,is_legacy_profile:false,session_directory_id:id,created_at:1,last_used_at:0,has_saved_database_secret:false,logout_pending:false})),primary_profile_id:'A',interrupted_profiles:[],debug_tools:true,has_saved_api_hash:true,has_saved_proxy_password:false};

const status = (id: string, generation=1) => ({profile_id:id,generation,runtime_state:'Ready',auth_state:'authorizationStateReady',connection_state:'connectionStateReady',detail:'',user_disconnected:false});
const scan = (run_id: string, scanned: number) => ({run_id,scanned,status:'Complete' as const,pages:1,exact:true,detail:'',posts:[]});
test('background query and send updates belong to the profile, even with identical run and message IDs', () => {
  const state=createAccountViews();
  state.connection(status('A'));state.connection(status('B'));
  state.selectChat('A',{id:'1',title:'A chat',kind:'group'});
  state.selectChat('B',{id:'2',title:'B chat',kind:'group'});
  state.get('A').currentRun='same';state.get('B').currentRun='same';
  state.get('A').scanChatId='1';state.get('B').scanChatId='2';
  state.progress({profile_id:'A',generation:1,result:scan('same',17)});
  assert.equal(state.get('A').result?.scanned,17);assert.equal(state.get('B').result,null);
  for(const id of ['A','B']){
    state.get(id).reportTarget='10';
    state.get(id).report={items:[{source:{chat_id:'1',message_id:'2'},state:'Pending',detail:'',sent_id:'3'}]};
  }
  state.sent({profile_id:'A',generation:1,target:'10',old_message_id:'3',state:'Sent'});
  assert.equal(state.get('A').report?.items[0]?.state,'Sent');
  assert.equal(state.get('B').report?.items[0]?.state,'Pending');
  state.connection(status('A',2));
  state.progress({profile_id:'A',generation:1,result:scan('same',99)});
  state.sent({profile_id:'A',generation:1,target:'10',old_message_id:'3',state:'Failed'});
  assert.equal(state.get('A').result,null);assert.deepEqual(state.get('A').sendUpdates,{});
  assert.equal(state.get('B').report?.items[0]?.state,'Pending');
  assert.equal(state.connection(status('A',1)),false);
});

// Mount with Vue's in-memory renderer: no WebView2, TDLib or Telegram connection.
const renderer=createRenderer<object, object>({
  createElement:()=>({}),createText:()=>({}),createComment:()=>({}),
  insert:()=>{},remove:()=>{},setText:()=>{},setElementText:()=>{},patchProp:()=>{},
  parentNode:()=>null,nextSibling:()=>null,
});
test('A → B → A restores chat, query builder and selection and invokes only view preference',async()=>{
  const calls:string[]=[];
  const invoke=async (command:string)=>{calls.push(command);return undefined;};
  let w!:ReturnType<typeof useWorkspace>;
  const app=renderer.createApp({setup(){w=useWorkspace(false,invoke as Parameters<typeof useWorkspace>[1]);return()=>h('div');}});
  app.mount({});
  w.statuses.A={...status('A'),runtime_state:'NeedsDatabasePassphrase',auth_state:'authorizationStateWaitTdlibParameters'};
  w.statuses.B={...status('B'),runtime_state:'AuthorizationRequired',auth_state:'authorizationStateWaitCode'};
  w.profileId.value='A';await w.selectAccount(true);
  w.chatList.value=[{id:'1',title:'A chat',kind:'group'}];w.selectedChat.value=w.chatList.value[0]!;
  w.rawCount.value=71;w.currentRun.value='a-run';w.scanning.value=true;w.selected.value=['a-post'];
  w.profileId.value='B';await w.selectAccount(true);
  assert.deepEqual(w.chatList.value,[]);assert.equal(w.selectedChat.value,null);assert.equal(w.rawCount.value,50);
  w.chatList.value=[{id:'2',title:'B chat',kind:'group'}];w.rawCount.value=82;
  w.statuses.A=status('A');w.statuses.B=status('B');
  w.profileId.value='A';await w.selectAccount(true);
  assert.equal(w.selectedChat.value?.id,'1');assert.equal(w.rawCount.value,71);
  assert.equal(w.currentRun.value,'a-run');assert.equal(w.scanning.value,true);assert.deepEqual(w.selected.value,['a-post']);
  assert.deepEqual(calls,['select_profile','select_profile','select_profile']);
  assert.equal(w.statuses.B.connection_state,'connectionStateReady');
  assert.equal(w.ready.value,true);
  // Background authorization never opens a settings window.
  assert.equal(calls.includes('open_settings'),false);
  app.unmount();
});
test('global rebuild generations replace each profile view independently',()=>{
  const state=createAccountViews();
  state.connection(status('A'));state.connection(status('B'));
  state.get('A').chatList=[{id:'1',title:'A',kind:'group'}];
  state.get('B').chatList=[{id:'2',title:'B',kind:'group'}];
  state.connection({...status('A',2),runtime_state:'Initializing'});
  assert.deepEqual(state.get('A').chatList,[]);assert.equal(state.get('B').chatList[0]?.id,'2');
  state.connection({...status('B',2),runtime_state:'NeedsDatabasePassphrase'});
  assert.deepEqual(state.get('B').chatList,[]);
  assert.equal(state.statuses.A?.generation,2);assert.equal(state.statuses.B?.generation,2);
  assert.equal(state.statuses.B?.runtime_state,'NeedsDatabasePassphrase');
});
test('Unknown receipts survive disconnect and rebuild without changing another account',()=>{
  const state=createAccountViews();state.connection(status('A'));state.connection(status('B'));
  for(const id of ['A','B']){state.get(id).reportTarget='10';state.get(id).report={items:[{source:{chat_id:'1',message_id:'2'},state:'Pending',detail:'',sent_id:'3'}]};}
  state.sent({profile_id:'A',generation:1,target:'10',old_message_id:'3',state:'Unknown'});
  assert.equal(state.get('A').report?.items[0]?.state,'Unknown');assert.equal(state.get('B').report?.items[0]?.state,'Pending');
  state.connection(status('A',2));state.reset('A');
  assert.equal(state.get('A').uncertainSends.length,1);assert.equal(state.get('B').uncertainSends.length,0);
  state.sent({profile_id:'A',generation:1,target:'10',old_message_id:'3',state:'Unknown'});
  assert.equal(state.get('A').uncertainSends.length,1);
  state.sent({profile_id:'A',generation:2,target:'10',old_message_id:'3',state:'Sent'});
  assert.equal(state.get('A').uncertainSends.length,1); // Reused temporary ID in new client.
});
test('pending disconnect cancellation leaves client alone and confirmation passes the reviewed token',async()=>{
  const calls:Array<{command:string;args:unknown}>=[];
  let w!:ReturnType<typeof useWorkspace>;
  const transport=async(command:string,args:unknown)=>{calls.push({command,args});if(command==='close_guard')return {pending_count:1,retry_after_ms:0,token:'reviewed'};};
  const app=renderer.createApp({setup(){w=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1],(async()=>{}) as Parameters<typeof useWorkspace>[2]);return()=>h('div');}});app.mount({});w.profileId.value='A';w.snapshot.value=fixture;
  const cancelled=w.disconnect();await new Promise(resolve=>setImmediate(resolve));assert.match(w.dialog.value?.text??'',/Unknown/);w.answerDialog(null);await cancelled;
  assert.deepEqual(calls.map(c=>c.command),['close_guard']);
  const confirmed=w.disconnect();await new Promise(resolve=>setImmediate(resolve));w.answerDialog({text:'',deleteData:false});await confirmed;
  assert.deepEqual(calls.at(-1),{command:'disconnect',args:{profileId:'A',closeConfirmation:'reviewed'}});
  assert.equal(calls.some(c=>c.command==='logout_account'),false);app.unmount();
});
test('pending wait does not close early or erase unsaved global secrets',async()=>{
  const calls:string[]=[],emissions:string[]=[];let w!:ReturnType<typeof useWorkspace>;
  const transport=async(command:string)=>{calls.push(command);if(command==='close_guard')return {pending_count:1,retry_after_ms:30000,token:null};};
  const app=renderer.createApp({setup(){w=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1],(async(name:string)=>{emissions.push(name);}) as Parameters<typeof useWorkspace>[2]);return()=>h('div');}});app.mount({});w.profileId.value='A';
  w.snapshot.value={settings:{schema_version:1,api_id:1,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},profiles:[],has_saved_api_hash:true,has_saved_proxy_password:false,primary_profile_id:null,interrupted_profiles:[],debug_tools:true};
  w.apiHash.value='unsaved synthetic hash';const save=w.saveSettings();await new Promise(resolve=>setImmediate(resolve));
  assert.match(w.dialog.value?.text??'',/30/);w.answerDialog({text:'',deleteData:false});await save;
  assert.deepEqual(calls,['close_guard']);assert.deepEqual(emissions,[]);assert.equal(w.apiHash.value,'unsaved synthetic hash');app.unmount();
});
test('add uses a dedicated login service, settings carry explicit IDs and dragging does not write IPC',async()=>{
  const calls:Array<{command:string;args:unknown}>=[];let w!:ReturnType<typeof useWorkspace>;
  const transport=async(command:string,args:unknown)=>{calls.push({command,args});};
  await renderToString(createSSRApp({setup(){w=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);return()=>h('div');}}));
  assert.equal(w.profileId.value,'');assert.equal(w.snapshot.value,null);
  await w.addAccount();assert.equal(w.profileId.value,'');
  await w.openSettings();w.profileId.value='A';await w.openAccountSettings();
  w.resizeLayout(.3,.4);assert.equal(w.leftRatio.value,.3);w.resetLayout();assert.equal(w.leftRatio.value,.23);assert.equal(w.rightRatio.value,.29);
  assert.deepEqual(calls,[{command:'open_login',args:undefined},{command:'open_settings',args:{tab:'application',profileId:null}},{command:'open_settings',args:{tab:'account',profileId:'A'}}]);
});
test('bound account settings target remains A while another window switches its active view to B',async()=>{
  const calls:Array<{command:string;args:unknown}>=[];let account!:ReturnType<typeof useWorkspace>,main!:ReturnType<typeof useWorkspace>;
  const transport=async(command:string,args:unknown)=>{calls.push({command,args});if(command==='close_guard')return {pending_count:0,retry_after_ms:0,token:null};};
  await renderToString(createSSRApp({setup(){account=useWorkspace(true,transport as Parameters<typeof useWorkspace>[1],(async()=>{}) as Parameters<typeof useWorkspace>[2]);main=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);return()=>h('div');}}));
  account.snapshot.value=fixture;account.settingsTargetReady.value=true;account.profileId.value='A';main.profileId.value='B';await main.selectAccount();
  await account.disconnect();
  assert.equal(account.profileId.value,'A');assert.deepEqual(calls.at(-1),{command:'disconnect',args:{profileId:'A',closeConfirmation:null}});
});

test('chat scope is sent in IPC and late replies never replace the selected list', async()=>{
  const pending:Array<{scope:string;resolve:(value:unknown)=>void}>=[];
  let w!:ReturnType<typeof useWorkspace>;
  const transport=async(command:string,args:unknown)=>{
    if(command==='chats')return new Promise<unknown>(resolve=>pending.push({scope:(args as {scope:string}).scope,resolve}));
    return undefined;
  };
  const app=renderer.createApp({setup(){w=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);return()=>h('div');}});
  app.mount({});
  w.profileId.value='A';w.statuses.A=status('A');
  const previous=w.loadChats();await new Promise(resolve=>setImmediate(resolve));
  const switched=w.switchChatScope('archive');await new Promise(resolve=>setImmediate(resolve));
  assert.deepEqual(pending.map(item=>item.scope),['all','archive']);
  pending[1]!.resolve([{id:'2',title:'Archived',kind:'group',archived:true}]);
  await switched;
  assert.deepEqual(w.chatList.value.map(x=>x.id),['2']);
  pending[0]!.resolve([{id:'1',title:'Stale main result',kind:'group',archived:false}]);
  await previous;
  assert.deepEqual(w.chatList.value.map(x=>x.id),['2']);
  const main=w.switchChatScope('main');await new Promise(resolve=>setImmediate(resolve));
  assert.equal(pending.at(-1)!.scope,'main');
  pending.at(-1)!.resolve([{id:'2',title:'Incorrect archived reply',kind:'group',archived:true},{id:'1',title:'Main',kind:'group',archived:false}]);
  await main;
  assert.deepEqual(w.chatList.value.map(x=>x.id),['1']);
  app.unmount();
});

test('incomplete Post confirmation cannot preview an old account after switching',async()=>{
  const calls:Array<{command:string;args:any}>=[];
  let w!:ReturnType<typeof useWorkspace>;
  const transport=async(command:string,args:any)=>{
    calls.push({command,args});
    if(command==='preview_forward')return {items:[]};
  };
  const app=renderer.createApp({setup(){
    w=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);
    return()=>h('div');
  }});app.mount({});
  w.profileId.value='A';w.selectChat({id:'1',title:'A chat',kind:'group'});
  w.currentRun.value='test-run';w.scanChatId.value='1';w.target.value='-1002';w.selected.value=['p'];
  w.result.value={
    run_id:'test-run',status:'Complete',stop_reason:'PostLimitReached',detail:'',
    scope:{kind:'recent',count:1},
    coverage:{requested_posts:1,examined_posts:1,scope_complete:true,date_window_exhausted:null},
    stats:{raw_messages_read:1,posts_finalized:1,posts_matched:1,posts_returned:1,pages:1},
    scanned:1,pages:1,exact:true,
    posts:[{key:'p',kind:'album',members:[],representative:{chat_id:'1',message_id:'2'},
      date:100,text:'',metrics:{views:null,forwards:null,reply_info:null,reactions:null},
      provenance:{},completeness:{level:'Unknown',reason:'older_boundary_not_observed'}}],
  };
  const pending=w.prepareForward();await new Promise(resolve=>setImmediate(resolve));
  assert.match(w.dialog.value?.title??'',/可能不完整/);
  w.profileId.value='B';w.answerDialog({text:'',deleteData:false});await pending;
  assert.equal(calls.some(x=>x.command==='preview_forward'),false);
  assert.equal(w.report.value,null);
  w.profileId.value='A';
  const confirmed=w.prepareForward();await new Promise(resolve=>setImmediate(resolve));
  w.answerDialog({text:'',deleteData:false});await confirmed;
  assert.deepEqual(calls.at(-1),{command:'preview_forward',args:{
    profileId:'A',target:'-1002',postKeys:['p'],runId:'test-run',allowIncomplete:true,
  }});
  assert.equal(w.confirmation.value,true);
  app.unmount();
});
