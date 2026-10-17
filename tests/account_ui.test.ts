import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile,writeFile,unlink} from 'node:fs/promises';
import {createSSRApp,createRenderer,h,provide,nextTick,type Component} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {parse,compileScript} from 'vue/compiler-sfc';
import ts from 'typescript';
import {useWorkspace,workspaceKey} from '../src/useWorkspace.ts';
import {useSettingsNavigation} from '../src/useSettingsNavigation.ts';
import {accountStatus} from '../src/accountViews.ts';
import {authorizationInput,useLogin} from '../src/useLogin.ts';
import type {SettingsSnapshot} from '../src/types.ts';

const fixture:SettingsSnapshot={settings:{schema_version:1,api_id:12,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},primary_profile_id:'A',interrupted_profiles:[],debug_tools:true,profiles:[{profile_id:'A',telegram_user_id:'101',display_name:'Real A',username:'real_a',is_legacy_profile:false,session_directory_id:'A',created_at:1,last_used_at:0,has_saved_database_secret:false,logout_pending:false}],has_saved_api_hash:true,has_saved_proxy_password:false};
const connection={profile_id:'A',generation:1,runtime_state:'Ready',auth_state:'authorizationStateReady',connection_state:'connectionStateReady',detail:'Session ready',user_disconnected:false};

// Compile the actual SFCs for SSR and in-memory interaction. No Telegram/real data.
async function withComponent<T>(file:string,run:(component:Component)=>Promise<T>) {
  const url=new URL(`../src/components/${file}`,import.meta.url);
  const {descriptor}=parse(await readFile(url,'utf8'));
  let source=compileScript(descriptor,{id:'acceptance',inlineTemplate:true}).content;
  source=source.replace(/from ['"]\.\.\/useWorkspace['"]/g,"from '../useWorkspace.ts'").replace(/from ['"]\.\.\/accountViews['"]/g,"from '../accountViews.ts'");
  source=source.replace(/import UiIcon from [^;]+;/,"const UiIcon={render(){return null;}};");
  source=source.replace(/import AvatarPreview from [^;]+;/,"import {h as avatarNode} from 'vue';const AvatarPreview={props:['profileId','generation','name'],render(){return avatarNode('span',{'data-avatar-profile':this.profileId,'data-avatar-generation':this.generation,'data-avatar-name':this.name});}};");
  source=source.replace(/import AppSelect from [^;]+;/,"const AppSelect={render(){return null;}};");
  source=source.replace(/import AppearanceSettings from [^;]+;/,"const AppearanceSettings={render(){return null;}};");
  const temporary=new URL(`.acceptance-${process.pid}-${file}.mjs`,url);
  await writeFile(temporary,ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText);
  try {
    const component=(await import(temporary.href)).default;
    return await run(component);
  } finally { await unlink(temporary); }
}
async function render(file:string,prepare:(w:ReturnType<typeof useWorkspace>)=>void,settingsView=false) {
  return withComponent(file,async component=>{
    const app=createSSRApp({setup(){const w=useWorkspace(settingsView);prepare(w);provide(workspaceKey,w);return()=>h(component);}});
    return renderToString(app);
  });
}

test('clicking the actual account settings disconnect button sends B and leaves A running',async()=>{
  await withComponent('AccountSettings.vue',async component=>{
    type Element={tag:string;text:string;props:Record<string,any>};
    const elements:Element[]=[];
    const renderer=createRenderer<Element,Element>({
      createElement:tag=>{const el={tag,text:'',props:{}};elements.push(el);return el;},
      createText:text=>({tag:'#text',text,props:{}}),createComment:()=>({tag:'#comment',text:'',props:{}}),
      insert(){},remove(){},setText(el,text){el.text=text;},setElementText(el,text){el.text=text;},
      patchProp(el,key,_old,value){el.props[key]=value;},parentNode:()=>null,nextSibling:()=>null,
    });
    const calls:Array<{command:string;args:any}>=[];
    const transport=async(command:string,args:any)=>{calls.push({command,args});if(command==='close_guard')return {pending_count:0,retry_after_ms:0,token:null};};
    let settings!:ReturnType<typeof useWorkspace>;
    const app=renderer.createApp({setup(){
      settings=useWorkspace(true,transport as Parameters<typeof useWorkspace>[1],(async()=>{}) as Parameters<typeof useWorkspace>[2]);
      settings.snapshot.value={...fixture,profiles:[...fixture.profiles,{...fixture.profiles[0]!,profile_id:'B',display_name:'Synthetic B'}]};
      settings.statuses.A=connection;settings.statuses.B={...connection,profile_id:'B'};
      const navigation=useSettingsNavigation(settings);
      navigation.navigate({tab:'application',profile_id:'B'});navigation.navigate({tab:'account'});
      provide(workspaceKey,settings);return()=>h(component);
    }});
    app.mount({tag:'root',text:'',props:{}});
    try{
      const button=elements.find(el=>el.tag==='button'&&el.text==='断开连接');
      assert.ok(button);assert.equal(button.props.disabled,false);
      await button.props.onClick();await nextTick();
      assert.deepEqual(calls,[{command:'close_guard',args:{profileId:'B'}},{command:'disconnect',args:{profileId:'B',closeConfirmation:null}}]);
      assert.equal(settings.statuses.A?.runtime_state,'Ready');
    }finally{app.unmount();}
  });
});
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
  assert.match(expired,/目标账号已不存在/);assert.match(expired,/添加新账号/);assert.equal(expired.includes('注销账号'),false);
});

test('global application target B renders B identity, avatar and status after switching to account tab',async()=>{
  const html=await render('AccountSettings.vue',w=>{
    w.snapshot.value={...fixture,profiles:[...fixture.profiles,{...fixture.profiles[0]!,profile_id:'B',telegram_user_id:'202',display_name:'Synthetic B',username:'synthetic_b'}]};
    w.statuses.A=connection;w.statuses.B={...connection,profile_id:'B',generation:7,detail:'Synthetic B connected'};
    const navigation=useSettingsNavigation(w);
    navigation.navigate({tab:'application',profile_id:'B'});navigation.navigate({tab:'account'});
  },true);
  for(const text of ['Synthetic B','@synthetic_b','202','Synthetic B connected','data-avatar-profile="B"','data-avatar-generation="7"','断开连接'])assert.ok(html.includes(text),text);
  for(const text of ['Real A','@real_a','data-avatar-profile="A"'])assert.equal(html.includes(text),false);
});

test('settings initialization and deleted B show no actionable A identity even with a Primary A snapshot',async()=>{
  for(const initialized of [false,true]){
    const html=await render('AccountSettings.vue',w=>{
      w.snapshot.value=fixture;w.statuses.A=connection;w.profileId.value='A';
      if(initialized)useSettingsNavigation(w).navigate({tab:'account',profile_id:'B'});
    },true);
    assert.match(html,initialized?/目标账号已不存在/:/正在加载设置目标账号/);
    for(const text of ['Real A','data-avatar-profile','断开连接','注销账号'])assert.equal(html.includes(text),false);
  }
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
  assert.deepEqual(calls[0],{command:'begin_login',args:{config:{phone:'+123456789',qr_login:false,passphrase:'synthetic key',remember_database:true,recovery_profile_id:null}}});
  login.value.value='synthetic code';login.twoFactorPassword.value='synthetic 2FA';await login.submit();assert.equal(login.value.value,'');assert.equal(login.twoFactorPassword.value,'');
  assert.deepEqual(calls.find(c=>c.command==='login_auth_submit'),{command:'login_auth_submit',args:{profileId:'pending',generation:1,authState:'authorizationStateWaitCode',code:'synthetic code',password:'synthetic 2FA'}});
  await login.cancel();assert.equal(calls.at(-1)?.command,'cancel_login');
  assert.equal(calls.some(c=>c.command==='logout_account'),false);
});
