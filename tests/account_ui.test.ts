import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile,writeFile,unlink} from 'node:fs/promises';
import {createSSRApp,h,provide} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {parse,compileScript} from 'vue/compiler-sfc';
import ts from 'typescript';
import {useWorkspace,workspaceKey} from '../src/useWorkspace.ts';
import {accountStatus} from '../src/accountViews.ts';
import {authorizationInput,useLogin} from '../src/useLogin.ts';
import type {SettingsSnapshot} from '../src/types.ts';

const fixture:SettingsSnapshot={settings:{schema_version:1,api_id:12,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},primary_profile_id:'A',interrupted_profiles:[],debug_tools:true,profiles:[{profile_id:'A',telegram_user_id:'101',display_name:'Real A',username:'real_a',is_legacy_profile:false,session_directory_id:'A',created_at:1,last_used_at:0,has_saved_database_secret:false,logout_pending:false}],has_saved_api_hash:true,has_saved_proxy_password:false};
const connection={profile_id:'A',generation:1,runtime_state:'Ready',auth_state:'authorizationStateReady',connection_state:'connectionStateReady',detail:'Session ready',user_disconnected:false};

// Compile the actual SFCs, then render with Vue SSR. No browser/Telegram/real data.
async function render(file:string,prepare:(w:ReturnType<typeof useWorkspace>)=>void) {
  const url=new URL(`../src/components/${file}`,import.meta.url);
  const {descriptor}=parse(await readFile(url,'utf8'));
  let source=compileScript(descriptor,{id:'acceptance',inlineTemplate:true}).content;
  source=source.replace(/from ['"]\.\.\/useWorkspace['"]/g,"from '../useWorkspace.ts'").replace(/from ['"]\.\.\/accountViews['"]/g,"from '../accountViews.ts'");
  source=source.replace(/import UiIcon from [^;]+;/,"const UiIcon={render(){return null;}};");
  source=source.replace(/import AvatarPreview from [^;]+;/,"const AvatarPreview={render(){return null;}};");
  source=source.replace(/import AppSelect from [^;]+;/,"const AppSelect={render(){return null;}};");
  source=source.replace(/import AppearanceSettings from [^;]+;/,"const AppearanceSettings={render(){return null;}};");
  const temporary=new URL(`.acceptance-${process.pid}-${file}.mjs`,url);
  await writeFile(temporary,ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText);
  try {
    const component=(await import(temporary.href)).default;
    const app=createSSRApp({setup(){const w=useWorkspace();prepare(w);provide(workspaceKey,w);return()=>h(component);}});
    return await renderToString(app);
  } finally { await unlink(temporary); }
}
test('empty panel renders only add-account guidance and global gear without avatar, switch or menu',async()=>{
  const html=await render('AccountPanel.vue',w=>{w.snapshot.value={...fixture,profiles:[],primary_profile_id:null};});
  assert.match(html,/尚未添加 Telegram 账号/);assert.match(html,/添加账号/);assert.match(html,/打开应用设置/);
  for(const absent of ['user-avatar','切换账号','更多账号操作','新账号','断开连接','恢复连接'])assert.equal(html.includes(absent),false);
});
test('linked panel menu has disconnect or resume and account settings; network loss is automatic',async()=>{
  const html=await render('AccountPanel.vue',w=>{w.snapshot.value=fixture;w.profileId.value='A';w.statuses.A={...connection,connection_state:'connectionStateConnecting'};});
  for(const text of ['Real A','@real_a','断开连接','账号设置'])assert.match(html,new RegExp(text));
  for(const text of ['恢复连接','注销账号','重命名','移除','解锁','紧凑'])assert.equal(html.includes(text),false);
  const closed=await render('AccountPanel.vue',w=>{w.snapshot.value=fixture;w.profileId.value='A';w.statuses.A={...connection,runtime_state:'Closed',user_disconnected:true};});
  assert.match(closed,/恢复连接/);assert.equal(closed.includes('断开连接'),false);
  assert.equal(accountStatus({...connection,connection_state:'connectionStateConnecting'}),'连接中');
});
test('settings subpages retain their options and empty account settings guide account creation',async()=>{
  const account=await render('AccountSettings.vue',w=>{w.snapshot.value=fixture;w.profileId.value='A';w.statuses.A=connection;});
  for(const text of ['Real A','101','注销账号','添加新账号'])assert.match(account,new RegExp(text));
  for(const text of ['API ID','API Hash','SOCKS5','记住此设备','重命名','紧凑','移除本地'])assert.equal(account.includes(text),false);
  const app=await render('ApplicationSettings.vue',w=>{w.snapshot.value=fixture;});
  for(const text of ['Telegram API ID','Telegram API Hash','SOCKS5','清除代理认证信息','重置 TMsgPicker'])assert.match(app,new RegExp(text));
  assert.ok(app.includes('重置 TMsgPicker'));
  for(const text of ['Real A','注销账号','添加新账号','数据库口令<input','紧凑','三栏宽度'])assert.equal(app.includes(text),false);
  const release=await render('ApplicationSettings.vue',w=>{w.snapshot.value={...fixture,debug_tools:false};});
  assert.equal(release.includes('重置 TMsgPicker'),false);
  const incomplete=await render('ApplicationSettings.vue',w=>{w.snapshot.value=fixture;w.resetReport.value={completed:false,error:'完整重置未完成',accounts:[{profile_id:'B',display_name:'Synthetic B',completed:false,error:'synthetic network timeout'}]};});
  assert.match(incomplete,/Synthetic B/);assert.match(incomplete,/synthetic network timeout/);assert.match(incomplete,/完整重置未完成/);
  const expired=await render('AccountSettings.vue',w=>{w.snapshot.value={...fixture,profiles:[]};w.profileId.value='A';});
  assert.match(expired,/尚未添加 Telegram 账号/);assert.match(expired,/添加新账号/);assert.equal(expired.includes('注销账号'),false);
});
test('wizard submits actual TDLib authorization state and clears transient inputs',async()=>{
  assert.equal(authorizationInput('authorizationStateWaitCode'),'Telegram 验证码');
  assert.equal(authorizationInput('authorizationStateWaitPassword'),'Telegram 两步验证密码');
  assert.equal(authorizationInput('authorizationStateWaitEmailCode'),'邮箱验证码');
  assert.equal(authorizationInput('authorizationStateReady'),undefined);
  const calls:Array<{command:string;args:unknown}>=[];
  let login!:ReturnType<typeof useLogin>;
  const transport=async(command:string,args:unknown)=>{calls.push({command,args});if(command==='login_status')return {...connection,profile_id:'pending',auth_state:'authorizationStateWaitCode'};if(command==='app_settings')return fixture;};
  await renderToString(createSSRApp({setup(){login=useLogin(transport as Parameters<typeof useLogin>[0]);return()=>h('div');}}));
  login.phone.value='+123456789';login.passphrase.value='synthetic key';login.remember.value=true;
  await login.begin();assert.equal(login.phone.value,'');assert.equal(login.passphrase.value,'');assert.equal(login.label.value,'Telegram 验证码');
  assert.deepEqual(calls[0],{command:'begin_login',args:{config:{phone:'+123456789',passphrase:'synthetic key',remember_database:true,recovery_profile_id:null}}});
  login.value.value='synthetic code';login.twoFactorPassword.value='synthetic 2FA';await login.submit();assert.equal(login.value.value,'');assert.equal(login.twoFactorPassword.value,'');
  assert.deepEqual(calls.find(c=>c.command==='login_auth_submit'),{command:'login_auth_submit',args:{profileId:'pending',generation:1,authState:'authorizationStateWaitCode',code:'synthetic code',password:'synthetic 2FA'}});
  await login.cancel();assert.equal(calls.at(-1)?.command,'cancel_login');
  assert.equal(calls.some(c=>c.command==='logout_account'),false);
});
