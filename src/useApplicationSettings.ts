import {ref,type Ref} from 'vue';
import type {invoke} from '@tauri-apps/api/core';
import type {SettingsSnapshot} from './types';

type Dialog = {title:string;text:string;input?:string;phrase?:string};
export function useApplicationSettings(
  snapshot:Ref<SettingsSnapshot|null>, transport:typeof invoke,
  action:(fn:()=>Promise<void|boolean>)=>Promise<void>,
  confirmClose:(profileId:string|null)=>Promise<string|null|undefined>,
  ask:(options:Dialog)=>Promise<{text:string;deleteData:boolean}|null>, notice:Ref<string>,
) {
  const apiId=ref(''),apiHash=ref(''),proxyEnabled=ref(false),proxyHost=ref('127.0.0.1'),proxyPort=ref(1080),proxyUser=ref(''),proxyPassword=ref('');
  function applySettings(){const s=snapshot.value?.settings;if(!s)return;apiId.value=s.api_id?.toString()??'';proxyEnabled.value=s.proxy_enabled;proxyHost.value=s.proxy_server;proxyPort.value=s.proxy_port;proxyUser.value=s.proxy_username;}
  async function saveSettings(){
    let saved=false;
    await action(async()=>{
      if(!snapshot.value)return;
      const closeConfirmation=await confirmClose(null);if(closeConfirmation===undefined)return false;
      const latest=await transport<SettingsSnapshot>('app_settings');
      snapshot.value=await transport('save_settings',{closeConfirmation,edit:{settings:{...latest.settings,api_id:apiId.value?Number(apiId.value):null,proxy_enabled:proxyEnabled.value,proxy_server:proxyHost.value,proxy_port:proxyPort.value,proxy_username:proxyUser.value},api_hash:apiHash.value||null,proxy_password:proxyPassword.value||null}});
      saved=true;notice.value='全局配置已保存，请查看各账号状态。';
    });
    if(saved){apiHash.value='';proxyPassword.value='';}
  }
  async function clearSecret(key:'api'|'proxy'){
    await action(async()=>{
      const closeConfirmation=await confirmClose(null);if(closeConfirmation===undefined)return false;
      snapshot.value=await transport('clear_secret',{key,closeConfirmation});
      if(key==='proxy')proxyUser.value='';notice.value='全局凭据已清除；运行中的账号已安全重建。';
    });
  }
  async function debugReset(){
    if(!snapshot.value?.debug_tools)return;
    const answer=await ask({title:'重置 TMsgPicker（清除全部本地数据）',text:'将删除应用配置、全部账号登记、API / 代理凭据、数据库口令、全部 TDLib Session 与 files、Legacy 数据及应用缓存。仅清理本机数据，不执行 Telegram logOut。完成后应用退出，请重新启动。请输入 RESET TMsgPicker 确认。',input:'',phrase:'RESET TMsgPicker'});
    if(answer?.text!=='RESET TMsgPicker')return;
    await action(async()=>{const closeConfirmation=await confirmClose(null);if(closeConfirmation===undefined)return false;await transport('debug_reset',{phrase:answer.text,closeConfirmation});});
  }
  return {apiId,apiHash,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,applySettings,saveSettings,clearSecret,debugReset};
}
