import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createSSRApp,createRenderer,h,nextTick} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {useWorkspace} from '../src/useWorkspace.ts';
import {useSettingsNavigation,type SettingsTarget} from '../src/useSettingsNavigation.ts';
import type {SettingsSnapshot} from '../src/types.ts';

const fixture:SettingsSnapshot={settings:{schema_version:1,api_id:12,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},profiles:['A','B'].map(id=>({profile_id:id,telegram_user_id:id==='A'?'1':'2',display_name:id,username:null,is_legacy_profile:false,session_directory_id:id,created_at:1,last_used_at:0,has_saved_database_secret:false,logout_pending:false})),primary_profile_id:'A',interrupted_profiles:[],debug_tools:true,has_saved_api_hash:true,has_saved_proxy_password:false};
const renderer=createRenderer<object,object>({createElement:()=>({}),createText:()=>({}),createComment:()=>({}),insert(){},remove(){},setText(){},setElementText(){},parentNode:()=>null,nextSibling:()=>null,patchProp(){}});

type Workspace=ReturnType<typeof useWorkspace>;
type Call={command:string;args:any};
const status=(id:string)=>({profile_id:id,generation:id==='A'?1:2,runtime_state:'Ready',auth_state:'authorizationStateReady',connection_state:'connectionStateReady',detail:`${id} ready`,user_disconnected:false});
async function setup(override?:(command:string,args:any)=>Promise<unknown>){
  const calls:Call[]=[];let settings!:Workspace,main!:Workspace,navigation!:ReturnType<typeof useSettingsNavigation>;
  const transport=async(command:string,args:any)=>{
    calls.push({command,args});
    if(override)return override(command,args);
    if(command==='app_settings')return structuredClone(fixture);
    if(command==='account_statuses')return [status('A'),status('B')];
    if(command==='unknown_sends')return [];
    if(command==='close_guard')return {pending_count:0,retry_after_ms:0,token:null};
  };
  await renderToString(createSSRApp({setup(){
    settings=useWorkspace(true,transport as Parameters<typeof useWorkspace>[1],(async()=>{}) as Parameters<typeof useWorkspace>[2]);
    main=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);
    settings.snapshot.value=structuredClone(fixture);main.snapshot.value=structuredClone(fixture);
    settings.statuses.A=status('A');settings.statuses.B=status('B');
    navigation=useSettingsNavigation(settings);
    return()=>h('div');
  }}));
  return {settings,main,navigation,calls};
}

test('A/F: global entry carries Active B separately from application tab and never writes Primary or secrets',async()=>{
  const {settings,main,navigation,calls}=await setup();
  main.profileId.value='B';await main.selectAccount();await main.openSettings();
  assert.deepEqual(calls,[{command:'select_profile',args:{profileId:'B'}},{command:'open_settings',args:{tab:'application',profileId:'B'}}]);
  navigation.navigate({tab:'application',profile_id:'B'});
  assert.equal(navigation.tab.value,'application');
  navigation.navigate({tab:'account'});
  assert.equal(settings.accountProfile.value?.telegram_user_id,'2');
  assert.equal(settings.connection.value.profile_id,'B');
  await settings.disconnect();
  assert.deepEqual(calls.at(-1),{command:'disconnect',args:{profileId:'B',closeConfirmation:null}});
  assert.equal(settings.statuses.A?.runtime_state,'Ready');
  assert.equal(settings.snapshot.value?.primary_profile_id,'A');assert.equal(main.snapshot.value?.primary_profile_id,'A');
  assert.deepEqual(calls.map(c=>c.command),['select_profile','open_settings','close_guard','disconnect']);
});

test('B: card entry targets B directly and connect/auth/logout use B',async()=>{
  const {settings,main,navigation,calls}=await setup();
  main.profileId.value='B';await main.openAccountSettings();
  assert.deepEqual(calls[0],{command:'open_settings',args:{tab:'account',profileId:'B'}});
  navigation.navigate({tab:'account',profile_id:'B'});
  await settings.connect();settings.authValue.value='synthetic code';await settings.submitAuth();
  const logout=settings.logoutAccount();assert.match(settings.dialog.value?.text??'',/「B」/);
  settings.answerDialog({text:'',deleteData:false});await logout;
  assert.deepEqual(calls.find(c=>c.command==='connect')?.args,{config:{profile_id:'B',passphrase:null,remember_database:false}});
  assert.deepEqual(calls.find(c=>c.command==='auth_submit')?.args,{profileId:'B',value:'synthetic code'});
  assert.deepEqual(calls.find(c=>c.command==='logout_account')?.args,{profileId:'B',confirmed:true});
});

test('C: reopening an existing settings window updates B on any tab and refresh/recreation cannot restore Primary A',async()=>{
  const {settings,main,navigation,calls}=await setup();
  navigation.navigate({tab:'account',profile_id:'A'});
  main.profileId.value='B';await main.openSettings();
  const target=calls.at(-1)!.args;
  navigation.navigate({tab:target.tab,profile_id:target.profileId});
  for(const tab of ['account','appearance','application'] as const){navigation.navigate({tab});assert.equal(settings.profileId.value,'B');}
  await settings.refreshSettings();assert.equal(settings.profileId.value,'B');
  const reopened=await setup();reopened.navigation.navigate({tab:target.tab,profile_id:target.profileId});
  await reopened.settings.refreshSettings();assert.equal(reopened.settings.profileId.value,'B');
});

test('E: unresolved, empty and deleted targets disable identity and reject every account operation without IPC',async()=>{
  const {settings,navigation,calls}=await setup();
  settings.profileId.value='A'; // A snapshot alone must not authorize settings actions.
  for(const target of [undefined,null,'deleted-B'] as const){
    if(target!==undefined)navigation.navigate({tab:'application',profile_id:target});
    assert.equal(settings.accountProfile.value,undefined);assert.ok(settings.targetError.value);
    await settings.connect();await settings.disconnect();await settings.submitAuth();await settings.logoutAccount();
    assert.equal(settings.dialog.value,null);assert.deepEqual(calls,[]);
  }
  navigation.navigate({tab:'account',profile_id:'B'});
  settings.snapshot.value={...fixture,profiles:[fixture.profiles[0]!]};
  await settings.disconnect();await settings.logoutAccount();
  assert.equal(settings.profileId.value,'B');assert.deepEqual(calls,[]);
});

test('disconnect confirmation and logout confirmation retain captured B during navigation to A',async()=>{
  for(const operation of ['disconnect','logoutAccount'] as const){
    const {settings,navigation,calls}=await setup(async(command)=>{
      if(command==='close_guard')return {pending_count:1,retry_after_ms:0,token:'B-token'};
      if(command==='app_settings')return fixture;
      if(command==='account_statuses'||command==='unknown_sends')return [];
    });
    navigation.navigate({tab:'account',profile_id:'B'});
    const pending=settings[operation]();await new Promise(resolve=>setImmediate(resolve));
    assert.match(settings.dialog.value?.text??'',/「B」/);
    navigation.navigate({tab:'application',profile_id:'A'});
    settings.answerDialog({text:'',deleteData:false});await pending;
    const command=operation==='disconnect'?'disconnect':'logout_account';
    assert.equal(calls.find(c=>c.command===command)?.args.profileId,'B');
    assert.equal(settings.profileId.value,'A');assert.equal(settings.accountError.value,'');
  }
});

test('deletion during a close or logout confirmation aborts instead of operating on Primary',async()=>{
  for(const operation of ['disconnect','logoutAccount'] as const){
    const {settings,navigation,calls}=await setup(async(command)=>command==='close_guard'?{pending_count:1,retry_after_ms:0,token:'B-token'}:undefined);
    navigation.navigate({tab:'account',profile_id:'B'});
    const pending=settings[operation]();await new Promise(resolve=>setImmediate(resolve));
    settings.snapshot.value={...fixture,profiles:[fixture.profiles[0]!]};
    settings.answerDialog({text:'',deleteData:false});await pending;
    assert.equal(calls.some(c=>c.command==='disconnect'||c.command==='logout_account'),false);
    assert.match(settings.accountError.value,/目标账号无效/);assert.equal(settings.profileId.value,'B');
  }
});

test('late operation failures stay with captured B, and a backend missing-account rejection never retries A',async()=>{
  for(const operation of ['connect','submitAuth','disconnect','logoutAccount'] as const){
    let reject!:(error:Error)=>void;
    const {settings,navigation,calls}=await setup(async(command)=>{
      if(command==='close_guard')return {pending_count:0,retry_after_ms:0,token:null};
      return new Promise((_resolve,fail)=>{reject=fail;});
    });
    navigation.navigate({tab:'account',profile_id:'B'});
    const pending=settings[operation]();
    if(operation==='logoutAccount')settings.answerDialog({text:'',deleteData:false});
    await new Promise(resolve=>setImmediate(resolve));
    navigation.navigate({tab:'application',profile_id:'A'});reject(new Error('Unknown account'));await pending;
    assert.equal(settings.accountError.value,'');
    navigation.navigate({tab:'account',profile_id:'B'});assert.match(settings.accountError.value,/Unknown account/);
    assert.ok(calls.every(c=>(c.args.profileId??c.args.config?.profile_id)==='B'));
  }
});

test('one settings controller switches subpages and explicit account targets without changing main selection or draft options',async()=>{
  const calls:string[]=[];let settings!:ReturnType<typeof useWorkspace>,main!:ReturnType<typeof useWorkspace>,navigation!:ReturnType<typeof useSettingsNavigation>;
  const transport=async(command:string)=>{calls.push(command);};
  const app=renderer.createApp({setup(){settings=useWorkspace(true,transport as Parameters<typeof useWorkspace>[1]);main=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);settings.snapshot.value=fixture;settings.profileId.value='A';navigation=useSettingsNavigation(settings);return()=>h('div');}});app.mount({});
  main.profileId.value='B';settings.apiHash.value='unsaved synthetic API hash';settings.passphrase.value='unsaved synthetic session password';
  navigation.navigate({tab:'account',profile_id:'A'});assert.equal(settings.passphrase.value,'unsaved synthetic session password');
  navigation.navigate({tab:'appearance'});assert.equal(navigation.tab.value,'appearance');assert.equal(settings.profileId.value,'A');assert.equal(settings.passphrase.value,'unsaved synthetic session password');assert.equal(settings.apiHash.value,'unsaved synthetic API hash');
  navigation.navigate({tab:'application'});navigation.navigate({tab:'account',profile_id:'B'});
  assert.equal(navigation.tab.value,'account');assert.equal(settings.profileId.value,'B');assert.equal(main.profileId.value,'B');assert.equal(settings.apiHash.value,'unsaved synthetic API hash');assert.equal(settings.passphrase.value,'');assert.deepEqual(calls,[]);
  settings.snapshot.value={...fixture,profiles:[fixture.profiles[0]]};await nextTick();assert.equal(settings.profileId.value,'B');assert.equal(settings.accountProfile.value,undefined);assert.ok(settings.targetError.value);
  app.unmount();
});

test('new navigation events win over delayed initial settings target reads',async()=>{
  let resolveTarget!:(target:SettingsTarget)=>void,receive!:(event:{payload:SettingsTarget})=>void,navigation!:ReturnType<typeof useSettingsNavigation>,settings!:ReturnType<typeof useWorkspace>;
  const transport=async()=>new Promise<SettingsTarget>(resolve=>{resolveTarget=resolve;});
  const listenEvent=async(_name:string,callback:typeof receive)=>{receive=callback;return()=>{};};
  await renderToString(createSSRApp({setup(){settings=useWorkspace(true);settings.snapshot.value=fixture;navigation=useSettingsNavigation(settings,transport as Parameters<typeof useSettingsNavigation>[1],listenEvent as Parameters<typeof useSettingsNavigation>[2]);return()=>h('div');}}));
  const loading=navigation.load();await Promise.resolve();receive({payload:{tab:'application',profile_id:'B'}});resolveTarget({tab:'account',profile_id:'A'});await loading;
  assert.equal(navigation.tab.value,'application');assert.equal(settings.profileId.value,'B');
});

test('a navigation event during listener registration also wins over the initial target response',async()=>{
  let navigation!:ReturnType<typeof useSettingsNavigation>,settings!:Workspace;
  const transport=async()=>({tab:'account',profile_id:'A'});
  const listenEvent=async(_name:string,callback:(event:{payload:SettingsTarget})=>void)=>{callback({payload:{tab:'appearance',profile_id:'B'}});return()=>{};};
  await renderToString(createSSRApp({setup(){settings=useWorkspace(true);settings.snapshot.value=fixture;navigation=useSettingsNavigation(settings,transport as Parameters<typeof useSettingsNavigation>[1],listenEvent as Parameters<typeof useSettingsNavigation>[2]);return()=>h('div');}}));
  await navigation.load();assert.equal(navigation.tab.value,'appearance');assert.equal(settings.profileId.value,'B');
});
