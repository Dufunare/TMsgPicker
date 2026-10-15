import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createSSRApp,createRenderer,h,nextTick} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {useWorkspace} from '../src/useWorkspace.ts';
import {useSettingsNavigation,type SettingsTarget} from '../src/useSettingsNavigation.ts';
import type {SettingsSnapshot} from '../src/types.ts';

const fixture:SettingsSnapshot={settings:{schema_version:1,api_id:12,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},profiles:['A','B'].map(id=>({profile_id:id,telegram_user_id:id==='A'?'1':'2',display_name:id,username:null,is_legacy_profile:false,session_directory_id:id,created_at:1,last_used_at:0,has_saved_database_secret:false,logout_pending:false})),primary_profile_id:'A',interrupted_profiles:[],debug_tools:true,has_saved_api_hash:true,has_saved_proxy_password:false};
const renderer=createRenderer<object,object>({createElement:()=>({}),createText:()=>({}),createComment:()=>({}),insert(){},remove(){},setText(){},setElementText(){},parentNode:()=>null,nextSibling:()=>null,patchProp(){}});

test('one settings controller switches subpages and explicit account targets without changing main selection or draft options',async()=>{
  const calls:string[]=[];let settings!:ReturnType<typeof useWorkspace>,main!:ReturnType<typeof useWorkspace>,navigation!:ReturnType<typeof useSettingsNavigation>;
  const transport=async(command:string)=>{calls.push(command);};
  const app=renderer.createApp({setup(){settings=useWorkspace(true,transport as Parameters<typeof useWorkspace>[1]);main=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);settings.snapshot.value=fixture;settings.profileId.value='A';navigation=useSettingsNavigation(settings);return()=>h('div');}});app.mount({});
  main.profileId.value='B';settings.apiHash.value='unsaved synthetic API hash';settings.passphrase.value='unsaved synthetic session password';
  navigation.navigate({tab:'account',profile_id:'A'});assert.equal(settings.passphrase.value,'unsaved synthetic session password');
  navigation.navigate({tab:'appearance',profile_id:null});assert.equal(navigation.tab.value,'appearance');assert.equal(settings.profileId.value,'A');assert.equal(settings.passphrase.value,'unsaved synthetic session password');assert.equal(settings.apiHash.value,'unsaved synthetic API hash');
  navigation.navigate({tab:'application',profile_id:null});navigation.navigate({tab:'account',profile_id:'B'});
  assert.equal(navigation.tab.value,'account');assert.equal(settings.profileId.value,'B');assert.equal(main.profileId.value,'B');assert.equal(settings.apiHash.value,'unsaved synthetic API hash');assert.equal(settings.passphrase.value,'');assert.deepEqual(calls,[]);
  settings.snapshot.value={...fixture,profiles:[fixture.profiles[0]]};await nextTick();assert.equal(settings.profileId.value,'A');
  app.unmount();
});

test('new navigation events win over delayed initial settings target reads',async()=>{
  let resolveTarget!:(target:SettingsTarget)=>void,receive!:(event:{payload:SettingsTarget})=>void,navigation!:ReturnType<typeof useSettingsNavigation>,settings!:ReturnType<typeof useWorkspace>;
  const transport=async()=>new Promise<SettingsTarget>(resolve=>{resolveTarget=resolve;});
  const listenEvent=async(_name:string,callback:typeof receive)=>{receive=callback;return()=>{};};
  await renderToString(createSSRApp({setup(){settings=useWorkspace(true);settings.snapshot.value=fixture;navigation=useSettingsNavigation(settings,transport as Parameters<typeof useSettingsNavigation>[1],listenEvent as Parameters<typeof useSettingsNavigation>[2]);return()=>h('div');}}));
  const loading=navigation.load();await Promise.resolve();receive({payload:{tab:'account',profile_id:'B'}});resolveTarget({tab:'application',profile_id:null});await loading;
  assert.equal(navigation.tab.value,'account');assert.equal(settings.profileId.value,'B');
});
