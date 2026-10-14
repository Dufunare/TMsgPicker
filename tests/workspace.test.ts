import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRenderer, h } from 'vue';
import { createAccountViews } from '../src/accountViews.ts';
import { useWorkspace } from '../src/useWorkspace.ts';

const status = (id: string, generation=1) => ({profile_id:id,generation,runtime_state:'Ready',auth_state:'authorizationStateReady',connection_state:'connectionStateReady',detail:''});
const scan = (run_id: string, scanned: number) => ({run_id,scanned,status:'Complete' as const,pages:1,exact:true,detail:'',posts:[]});
test('background query and send updates belong to the profile, even with identical run and message IDs', () => {
  const state=createAccountViews();
  state.connection(status('A'));state.connection(status('B'));
  state.get('A').currentRun='same';state.get('B').currentRun='same';
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
