import {onMounted,onUnmounted,ref,watch} from 'vue';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {Workspace} from './useWorkspace';

export interface SettingsTarget {tab:'application'|'account';profile_id:string|null}
export function useSettingsNavigation(workspace:Workspace,transport:typeof invoke=invoke,listenEvent:typeof listen=listen) {
  const tab=ref<'application'|'account'>('application');
  let unlisten:UnlistenFn|undefined,revision=0,disposed=false;
  function navigate(target:SettingsTarget){
    if(disposed||!['application','account'].includes(target.tab))return;
    revision++;tab.value=target.tab;
    if(target.tab==='account'&&target.profile_id&&target.profile_id!==workspace.profileId.value){workspace.profileId.value=target.profile_id;void workspace.selectAccount();}
  }
  watch(()=>workspace.snapshot.value?.profiles.map(p=>p.profile_id).join(','),()=>{
    const snapshot=workspace.snapshot.value;if(!snapshot)return;
    if(!snapshot.profiles.some(p=>p.profile_id===workspace.profileId.value)){
      workspace.profileId.value=snapshot.primary_profile_id??snapshot.profiles[0]?.profile_id??'';
      void workspace.selectAccount();
    }
  },{immediate:true});
  async function load(){
    unlisten=await listenEvent<SettingsTarget>('settings-navigate',e=>navigate(e.payload));
    if(disposed){unlisten();return;}
    const before=revision;
    const target=await transport<SettingsTarget>('settings_target');
    if(before===revision)navigate(target);
  }
  onMounted(()=>{if(isTauri())void load().catch(e=>{workspace.accountError.value=String(e);});});
  onUnmounted(()=>{disposed=true;unlisten?.();});
  return {tab,navigate,load};
}
