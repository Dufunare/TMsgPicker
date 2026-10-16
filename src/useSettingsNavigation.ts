import {onMounted,onUnmounted,ref} from 'vue';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {Workspace} from './useWorkspace';

export interface SettingsTarget {tab:'application'|'account'|'appearance';profile_id?:string|null}
export function useSettingsNavigation(workspace:Workspace,transport:typeof invoke=invoke,listenEvent:typeof listen=listen) {
  const tab=ref<SettingsTarget['tab']>('application');
  let unlisten:UnlistenFn|undefined,revision=0,disposed=false;
  function navigate(target:SettingsTarget){
    if(disposed||!['application','account','appearance'].includes(target.tab))return;
    revision++;tab.value=target.tab;
    // Omitted means a local tab change; explicit null means no account target.
    if(target.profile_id!==undefined){
      const id=target.profile_id??'';
      if(id!==workspace.profileId.value){workspace.profileId.value=id;void workspace.selectAccount();}
      workspace.settingsTargetReady.value=true;
    }
  }
  async function load(){
    const before=revision;
    unlisten=await listenEvent<SettingsTarget>('settings-navigate',e=>navigate(e.payload));
    if(disposed){unlisten();return;}
    const target=await transport<SettingsTarget>('settings_target');
    if(before===revision)navigate(target);
  }
  onMounted(()=>{if(isTauri())void load().catch(e=>{workspace.accountError.value=String(e);});});
  onUnmounted(()=>{disposed=true;unlisten?.();});
  return {tab,navigate,load};
}
