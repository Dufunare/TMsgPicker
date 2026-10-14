import {ref,type Ref} from 'vue';
import type {invoke} from '@tauri-apps/api/core';
import type {SettingsSnapshot,DebugResetReport} from './types';

type Dialog = {title:string;text:string;input?:string;phrase?:string};
export function useApplicationSettings(
  snapshot:Ref<SettingsSnapshot|null>, transport:typeof invoke,
  action:(fn:()=>Promise<void|boolean>)=>Promise<void>,
  confirmClose:(profileId:string|null,logout?:boolean)=>Promise<string|null|undefined>,
  ask:(options:Dialog)=>Promise<{text:string;deleteData:boolean}|null>, notice:Ref<string>,
) {
  const apiId=ref(''),apiHash=ref(''),proxyEnabled=ref(false),proxyHost=ref('127.0.0.1'),proxyPort=ref(1080),proxyUser=ref(''),proxyPassword=ref('');
  const resetReport=ref<DebugResetReport|null>(null),resetBusy=ref(false);
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
    if(!snapshot.value?.debug_tools||resetBusy.value)return;
    resetBusy.value=true;
    try {
      if(!await ask({title:'重置 TMsgPicker（注销全部账号并清除全部本地数据）',text:'这是 Debug 专用的最高级别破坏性操作，将注销全部关联 Telegram 账号，包括已主动断开的账号。全部注销完成后，将删除 ApplicationConfig、AccountRegistry、API ID / API Hash、SOCKS5 设置及认证信息、全部 DPAPI 凭据和数据库口令、全部 TDLib Session、Legacy Session、下载文件、缓存及应用专属目录中的其他持久化数据。操作不可撤销。成功后应用退出，请重新启动并重新配置 API 和账号；部分注销失败时保留恢复所需数据，允许重试。'}))return;
      const answer=await ask({title:'再次确认：注销全部账号并永久删除本地数据',text:'确认执行完整重置？已完成的 Telegram 注销无法撤销。请输入 RESET TMSGPICKER；完成后应用将退出，需要重新启动。',input:'',phrase:'RESET TMSGPICKER'});
      if(answer?.text!=='RESET TMSGPICKER')return;
      await action(async()=>{
        const closeConfirmation=await confirmClose(null,true);if(closeConfirmation===undefined)return false;
        resetReport.value=null;notice.value='正在注销全部账号；请等待逐账号结果。';
        try {
          const report=await transport<DebugResetReport>('debug_reset',{phrase:answer.text,closeConfirmation});
          resetReport.value=report;
          notice.value=report.completed?'完整重置已完成，应用正在退出；请重新启动。':(report.error??'完整重置未完成，请查看账号结果并恢复条件后重试。');
          if(!report.completed)snapshot.value=await transport<SettingsSnapshot>('app_settings');
        } catch(error) {notice.value='完整重置未完成。';throw error;}
      });
    } finally {resetBusy.value=false;}
  }
  return {apiId,apiHash,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,applySettings,saveSettings,clearSecret,debugReset,resetReport,resetBusy};
}
