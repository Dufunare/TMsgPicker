import {test} from 'node:test';
import assert from 'node:assert/strict';
import {ref} from 'vue';
import {useApplicationSettings} from '../src/useApplicationSettings.ts';
import type {SettingsSnapshot,DebugResetReport} from '../src/types.ts';

const fixture:SettingsSnapshot={settings:{schema_version:1,api_id:12,proxy_enabled:false,proxy_server:'localhost',proxy_port:1080,proxy_username:''},profiles:[],primary_profile_id:null,interrupted_profiles:[],debug_tools:true,has_saved_api_hash:false,has_saved_proxy_password:false};
const success:DebugResetReport={completed:true,accounts:[],error:null};
function setup(answers:Array<string|null>,report:DebugResetReport|Error=success,debug=true,pending:string|null|undefined=null){
  const dialogs:Array<{title:string;text:string;phrase?:string}>=[],calls:Array<{command:string;args:unknown}>=[],errors:string[]=[],notice=ref('');
  const transport=async(command:string,args?:unknown)=>{calls.push({command,args});if(command==='debug_reset'){if(report instanceof Error)throw report;return report;}return fixture;};
  const settings=useApplicationSettings(ref({...fixture,debug_tools:debug}),transport as Parameters<typeof useApplicationSettings>[1],async fn=>{try{await fn();}catch(e){errors.push(String(e));}},async(id,logout)=>{assert.equal(id,null);assert.equal(logout,true);return pending;},async options=>{dialogs.push(options);const text=answers.shift();return text===null||text===undefined?null:{text,deleteData:false};},notice);
  return {settings,dialogs,calls,errors,notice};
}
test('reset requires both confirmations and exact phrase and announces all categories',async()=>{
  for(const answers of [[null],['',null],['','RESET TMsgPicker']]){
    const s=setup(answers);await s.settings.debugReset();assert.equal(s.calls.length,0);
  }
  const s=setup(['','RESET TMSGPICKER']);await s.settings.debugReset();
  assert.equal(s.dialogs.length,2);
  for(const category of ['注销全部','主动断开','ApplicationConfig','AccountRegistry','API ID / API Hash','SOCKS5','DPAPI','数据库口令','Legacy Session','缓存','不可撤销'])assert.ok(s.dialogs[0].text.includes(category),category);
  assert.equal(s.dialogs[1].phrase,'RESET TMSGPICKER');
  assert.deepEqual(s.calls[0],{command:'debug_reset',args:{phrase:'RESET TMSGPICKER',closeConfirmation:null}});
  assert.equal(s.settings.resetBusy.value,false);
});
test('release cannot invoke reset and pending cancellation cannot submit it',async()=>{
  const release=setup(['','RESET TMSGPICKER'],success,false);await release.settings.debugReset();assert.equal(release.dialogs.length,0);assert.equal(release.calls.length,0);
  const cancelled=useApplicationSettings(ref(fixture),async()=>{throw new Error('must not invoke');},async fn=>{await fn();},async()=>undefined,async()=>({text:'RESET TMSGPICKER',deleteData:false}),ref(''));
  await cancelled.debugReset();assert.equal(cancelled.resetReport.value,null);
});
test('partial failure exposes account errors, retains retry action and never announces success',async()=>{
  const report:DebugResetReport={completed:false,error:'完整重置未完成',accounts:[{profile_id:'A',display_name:'A',completed:true,error:null},{profile_id:'B',display_name:'B',completed:false,error:'synthetic network timeout'}]};
  const s=setup(['','RESET TMSGPICKER'],report);await s.settings.debugReset();
  assert.deepEqual(s.settings.resetReport.value,report);assert.equal(s.notice.value,'完整重置未完成');assert.equal(s.calls[1].command,'app_settings');assert.equal(s.settings.resetBusy.value,false);
  const denied=setup(['','RESET TMSGPICKER'],new Error('synthetic deletion denied'));await denied.settings.debugReset();
  assert.equal(denied.notice.value,'完整重置未完成。');assert.match(denied.errors[0],/synthetic deletion denied/);
});
