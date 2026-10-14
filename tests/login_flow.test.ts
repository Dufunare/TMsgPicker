import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createSSRApp,h,provide} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {readFile,writeFile,unlink} from 'node:fs/promises';
import {parse,compileScript} from 'vue/compiler-sfc';
import ts from 'typescript';
import {useLogin} from '../src/useLogin.ts';
import type {SettingsSnapshot} from '../src/types.ts';

const connection={profile_id:'pending',generation:1,runtime_state:'AuthorizationRequired',auth_state:'authorizationStateWaitPhoneNumber',connection_state:'connectionStateReady',detail:'请输入手机号',user_disconnected:false};
const fixture:SettingsSnapshot={settings:{schema_version:1,api_id:12,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},profiles:['A','B'].map(id=>({profile_id:id,telegram_user_id:id==='A'?'1':'2',display_name:id,username:null,is_legacy_profile:false,session_directory_id:id,created_at:1,last_used_at:0,has_saved_database_secret:false,logout_pending:false})),primary_profile_id:'A',interrupted_profiles:[],debug_tools:true,has_saved_api_hash:true,has_saved_proxy_password:false};
async function makeLogin(transport:Parameters<typeof useLogin>[0]){
  let login!:ReturnType<typeof useLogin>;
  await renderToString(createSSRApp({setup(){login=useLogin(transport);return()=>h('div');}}));
  return login;
}
async function wizard(login:ReturnType<typeof useLogin>){
  const url=new URL('../src/components/LoginWizard.vue',import.meta.url);
  const {descriptor}=parse(await readFile(url,'utf8'));
  const source=compileScript(descriptor,{id:'login-acceptance',inlineTemplate:true}).content.replace(/import \{\s*useLogin\s*\} from ['"]\.\.\/useLogin['"];?/,"import {inject} from 'vue';const useLogin=()=>inject('synthetic-login');");
  const temporary=new URL(`.login-acceptance-${process.pid}.mjs`,url);
  await writeFile(temporary,ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText);
  try {const component=(await import(temporary.href)).default;return await renderToString(createSSRApp({setup(){provide('synthetic-login',login);return()=>h(component);}}));}
  finally {await unlink(temporary);}
}
test('automatic phone submission shows a waiting screen, then code and 2FA together without another phone form',async()=>{
  const transport=async(command:string)=>command==='login_status'?{...connection,phone_submitting:true}:command==='app_settings'?fixture:undefined;
  const login=await makeLogin(transport as Parameters<typeof useLogin>[0]);
  login.phone.value='+123456789';login.passphrase.value='synthetic session password';await login.begin();
  assert.equal(login.sendingPhone.value,true);assert.equal(login.label.value,undefined);
  const waiting=await wizard(login);assert.match(waiting,/正在提交手机号并请求验证码/);assert.equal(waiting.includes('type="tel"'),false);assert.equal(waiting.includes('确认短语'),false);assert.equal(waiting.includes('请输入手机号'),false);
  login.updateConnection({...connection,auth_state:'authorizationStateWaitCode',detail:'请输入验证码'});
  const code=await wizard(login);assert.match(code,/Telegram 验证码/);assert.match(code,/Telegram 两步验证密码（2FA）/);assert.match(code,/验证并登录/);assert.equal(code.includes('type="tel"'),false);
  login.updateConnection({...connection,auth_state:'authorizationStateWaitPassword',detail:'请输入两步验证密码'});
  const password=await wizard(login);assert.match(password,/Telegram 验证码/);assert.match(password,/验证码已验证/);assert.match(password,/Telegram 两步验证密码（2FA）/);
});

test('password errors keep the same form; retry sends only password and clears all transient credentials',async()=>{
  const calls:Array<{command:string;args:unknown}>=[];
  let state={...connection,auth_state:'authorizationStateWaitPassword'};
  let fail=true;
  const transport=async(command:string,args?:unknown)=>{calls.push({command,args});if(command==='login_auth_submit'&&fail)throw new Error('synthetic wrong password');if(command==='login_status')return state;if(command==='app_settings')return fixture;};
  const login=await makeLogin(transport as Parameters<typeof useLogin>[0]);login.updateConnection(state);
  login.twoFactorPassword.value='wrong synthetic password';await login.submit();assert.match(login.error.value,/wrong password/);assert.equal(login.twoFactorPassword.value,'');assert.equal(login.combinedAuth.value,true);
  fail=false;login.twoFactorPassword.value='correct synthetic password';await login.submit();
  assert.deepEqual(calls.filter(c=>c.command==='login_auth_submit').at(-1),{command:'login_auth_submit',args:{profileId:'pending',generation:1,authState:'authorizationStateWaitPassword',code:'',password:'correct synthetic password'}});
  login.value.value='synthetic code';login.twoFactorPassword.value='synthetic password';state={...state,generation:2};login.updateConnection(state);assert.equal(login.value.value,'');assert.equal(login.twoFactorPassword.value,'');
  login.twoFactorPassword.value='synthetic password';await login.cancel();assert.equal(login.twoFactorPassword.value,'');
});

test('phone rejection permits correction, while optional email authorization retains its existing path',async()=>{
  const calls:Array<{command:string;args:unknown}>=[];
  const state={...connection,phone_submitting:false};
  const transport=async(command:string,args?:unknown)=>{calls.push({command,args});if(command==='login_status')return state;if(command==='app_settings')return fixture;};
  const login=await makeLogin(transport as Parameters<typeof useLogin>[0]);login.updateConnection(state);assert.equal(login.label.value,'手机号（含国际区号）');
  login.updateConnection({...state,auth_state:'authorizationStateWaitEmailCode'});login.value.value='synthetic email code';await login.submit();
  assert.deepEqual(calls[0],{command:'auth_submit',args:{profileId:'pending',value:'synthetic email code'}});
});

test('new code events win over stale phone snapshots even before the pending owner is known',async()=>{
  let login!:ReturnType<typeof useLogin>;
  let incoming='authorizationStateWaitCode';
  const transport=async(command:string)=>{
    if(command==='begin_login')return 'pending';
    if(command==='login_status'){
      login.receiveConnection({...connection,auth_state:incoming,detail:'synthetic authorization state'});
      login.receiveConnection({...connection,profile_id:'another account',auth_state:'authorizationStateReady'});
      return {...connection,phone_submitting:true}; // Captured before the newer code event.
    }
    if(command==='app_settings')return fixture;
  };
  login=await makeLogin(transport as Parameters<typeof useLogin>[0]);
  await login.sync();assert.equal(login.connection.value?.auth_state,'authorizationStateWaitCode');assert.equal(login.combinedAuth.value,true);assert.equal(login.sendingPhone.value,false);
  login.updateConnection(null);login.phone.value='+123456789';login.passphrase.value='synthetic password';await login.begin();
  assert.equal(login.connection.value?.auth_state,'authorizationStateWaitCode');assert.equal(login.sendingPhone.value,false);
  login.receiveConnection({...connection,generation:0});assert.equal(login.connection.value?.auth_state,'authorizationStateWaitCode');
  incoming='authorizationStateWaitPhoneNumber';login.updateConnection(null);await login.sync();assert.equal(login.sendingPhone.value,true);assert.equal(login.label.value,undefined);
  login.error.value='synthetic phone rejection';await login.sync();assert.equal(login.sendingPhone.value,false);assert.equal(login.label.value,'手机号（含国际区号）');
});
