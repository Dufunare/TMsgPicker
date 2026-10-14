import {computed,onMounted,onUnmounted,ref} from 'vue';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {Connection,SettingsSnapshot} from './types';
export interface LoginConnection extends Connection {phone_submitting?:boolean}

export function authorizationInput(state:string) {
  return ({authorizationStateWaitPhoneNumber:'手机号（含国际区号）',authorizationStateWaitCode:'Telegram 验证码',authorizationStateWaitPassword:'Telegram 两步验证密码',authorizationStateWaitEmailAddress:'授权邮箱',authorizationStateWaitEmailCode:'邮箱验证码'} as Record<string,string>)[state];
}
export function useLogin(transport:typeof invoke=invoke) {
  const snapshot=ref<SettingsSnapshot|null>(null),connection=ref<LoginConnection|null>(null);
  const phone=ref(''),passphrase=ref(''),remember=ref(false),recoveryId=ref(''),value=ref(''),twoFactorPassword=ref(''),busy=ref(false),error=ref(''),phoneSubmitted=ref(false);
  const unlisteners:UnlistenFn[]=[];
  let revision=0;
  const events=new Map<string,{connection:Connection;revision:number}>();
  const sendingPhone=computed(()=>phoneSubmitted.value&&connection.value?.runtime_state!=='Error'&&['authorizationStateWaitTdlibParameters','authorizationStateWaitPhoneNumber'].includes(connection.value?.auth_state??''));
  const label=computed(()=>sendingPhone.value?undefined:authorizationInput(connection.value?.auth_state??''));
  const combinedAuth=computed(()=>['authorizationStateWaitCode','authorizationStateWaitPassword'].includes(connection.value?.auth_state??''));
  const pending=computed(()=>!!connection.value?.profile_id);
  function updateConnection(next:LoginConnection|null){
    const old=connection.value;
    if(old&&next&&old.profile_id===next.profile_id&&next.generation<old.generation)return;
    revision++;
    if(old&&next&&(old.profile_id!==next.profile_id||next.generation!==old.generation)){value.value='';twoFactorPassword.value='';if(old.generation>0||old.profile_id!==next.profile_id)phoneSubmitted.value=false;}
    connection.value=next;
    if(next?.phone_submitting!==undefined)phoneSubmitted.value=next.phone_submitting;
    if(!next||!['authorizationStateWaitTdlibParameters','authorizationStateWaitPhoneNumber'].includes(next.auth_state))phoneSubmitted.value=false;
    if(next?.auth_state==='authorizationStateReady'){value.value='';twoFactorPassword.value='';}
  }
  function receiveConnection(next:Connection){
    if(!next.profile_id)return;
    const cached=events.get(next.profile_id);
    if(cached&&next.generation<cached.connection.generation)return;
    events.set(next.profile_id,{connection:next,revision:++revision});
    if(next.profile_id===connection.value?.profile_id&&next.generation>=connection.value.generation)updateConnection(next);
  }
  async function sync(){
    const before=revision;
    const status=await transport<LoginConnection|null>('login_status');
    const event=status?.profile_id?events.get(status.profile_id):undefined;
    if(event&&event.revision>before&&event.connection.generation>=status!.generation)updateConnection({...event.connection,...(event.connection.generation===status!.generation?{phone_submitting:status!.phone_submitting}:{})});
    else updateConnection(status);
    if(error.value&&connection.value?.auth_state==='authorizationStateWaitPhoneNumber'){phoneSubmitted.value=false;connection.value.phone_submitting=false;}
    snapshot.value=await transport<SettingsSnapshot>('app_settings');
  }
  async function action(fn:()=>Promise<void>){error.value='';busy.value=true;try{await fn();}catch(e){error.value=String(e);}finally{busy.value=false;}}
  async function begin(){
    if(busy.value)return;
    const config={phone:phone.value,passphrase:passphrase.value,remember_database:remember.value,recovery_profile_id:recoveryId.value||null};
    phoneSubmitted.value=!!config.phone;phone.value='';passphrase.value='';value.value='';twoFactorPassword.value='';
    await action(async()=>{try{
      const id=await transport<string>('begin_login',{config});
      if(id&&!connection.value)updateConnection({profile_id:id,generation:0,runtime_state:'Initializing',auth_state:'authorizationStateWaitTdlibParameters',connection_state:'Disconnected',detail:'正在初始化登录',user_disconnected:false});
    }finally{await sync();}});
  }
  async function submit(){
    if(busy.value)return;
    const state=connection.value,profileId=state?.profile_id,submitted=value.value,password=twoFactorPassword.value;value.value='';twoFactorPassword.value='';
    if(!profileId)return;
    await action(async()=>{try{
      if(['authorizationStateWaitCode','authorizationStateWaitPassword'].includes(state.auth_state))await transport('login_auth_submit',{profileId,generation:state.generation,authState:state.auth_state,code:state.auth_state==='authorizationStateWaitCode'?submitted:'',password});
      else await transport('auth_submit',{profileId,value:submitted});
    }finally{await sync();}});
  }
  async function retryIdentity(){await action(async()=>{await transport('retry_login_identity');});}
  async function cancel(){phone.value='';passphrase.value='';value.value='';twoFactorPassword.value='';phoneSubmitted.value=false;await action(async()=>{await transport('cancel_login');});}
  async function openSettings(){await action(async()=>{await transport('open_settings',{profileId:null});});}
  onMounted(async()=>{
    if(!isTauri())return;
    await action(async()=>{
      unlisteners.push(await listen<Connection>('connection',e=>receiveConnection(e.payload)));
      unlisteners.push(await listen<string>('login-error',e=>{error.value=e.payload;phoneSubmitted.value=false;if(connection.value)connection.value.phone_submitting=false;twoFactorPassword.value='';}));
      await sync();
    });
  });
  onUnmounted(()=>{unlisteners.forEach(fn=>fn());events.clear();phone.value='';passphrase.value='';value.value='';twoFactorPassword.value='';phoneSubmitted.value=false;});
  return {snapshot,connection,phone,passphrase,remember,recoveryId,value,twoFactorPassword,busy,error,label,pending,sendingPhone,combinedAuth,begin,submit,cancel,openSettings,retryIdentity,updateConnection,receiveConnection,sync};
}
