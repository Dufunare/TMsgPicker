import {computed,onMounted,onUnmounted,ref} from 'vue';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {Connection,SettingsSnapshot} from './types';

export function authorizationInput(state:string) {
  return ({authorizationStateWaitPhoneNumber:'手机号（含国际区号）',authorizationStateWaitCode:'Telegram 验证码',authorizationStateWaitPassword:'Telegram 两步验证密码',authorizationStateWaitEmailAddress:'授权邮箱',authorizationStateWaitEmailCode:'邮箱验证码'} as Record<string,string>)[state];
}
export function useLogin(transport:typeof invoke=invoke) {
  const snapshot=ref<SettingsSnapshot|null>(null),connection=ref<Connection|null>(null);
  const phone=ref(''),passphrase=ref(''),remember=ref(false),recoveryId=ref(''),value=ref(''),busy=ref(false),error=ref('');
  const unlisteners:UnlistenFn[]=[];
  const label=computed(()=>authorizationInput(connection.value?.auth_state??''));
  const pending=computed(()=>!!connection.value?.profile_id);
  async function sync(){connection.value=await transport<Connection|null>('login_status');snapshot.value=await transport<SettingsSnapshot>('app_settings');}
  async function action(fn:()=>Promise<void>){error.value='';busy.value=true;try{await fn();}catch(e){error.value=String(e);}finally{busy.value=false;}}
  async function begin(){
    const config={phone:phone.value,passphrase:passphrase.value,remember_database:remember.value,recovery_profile_id:recoveryId.value||null};
    phone.value='';passphrase.value='';
    await action(async()=>{try{await transport('begin_login',{config});}finally{await sync();}});
  }
  async function submit(){
    const profileId=connection.value?.profile_id,submitted=value.value;value.value='';
    if(!profileId)return;
    await action(async()=>{await transport('auth_submit',{profileId,value:submitted});await sync();});
  }
  async function retryIdentity(){await action(async()=>{await transport('retry_login_identity');});}
  async function cancel(){await action(async()=>{await transport('cancel_login');});}
  async function openSettings(){await action(async()=>{await transport('open_settings',{profileId:null});});}
  onMounted(async()=>{
    if(!isTauri())return;
    await action(async()=>{
      unlisteners.push(await listen<Connection>('connection',e=>{if(e.payload.profile_id===connection.value?.profile_id&&e.payload.generation>=connection.value.generation)connection.value=e.payload;}));
      unlisteners.push(await listen<string>('login-error',e=>{error.value=e.payload;}));
      await sync();
    });
  });
  onUnmounted(()=>{unlisteners.forEach(fn=>fn());phone.value='';passphrase.value='';value.value='';});
  return {snapshot,connection,phone,passphrase,remember,recoveryId,value,busy,error,label,pending,begin,submit,cancel,openSettings,retryIdentity};
}
