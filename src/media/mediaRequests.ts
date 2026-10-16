import {invoke} from '@tauri-apps/api/core';
import {MediaScheduler,type MediaOptions} from './mediaScheduler.ts';

// A bound queue prevents expanded Albums from triggering hundreds of concurrent
// base64 IPC transfers. Requests for different accounts may progress in parallel;
// each account has at most one media download while TDLib holds its lifecycle lock.
const scheduler=new MediaScheduler(async (profileId,fileId,full)=>{
  for(let attempt=0;attempt<3;attempt++){
    try{
      return await invoke<string|null>('photo_thumbnail',{
        profileId,fileId:fileId===0?null:fileId,full,
      });
    }catch(error){
      if(!String(error).includes('Session operation in progress')||attempt===2)throw error;
      await new Promise(resolve=>setTimeout(resolve,100*(attempt+1)));
    }
  }
  return null;
});

export function requestMedia(
  profileId:string,generation:number,fileId:number,full:boolean,options:MediaOptions={},
):Promise<string|null>{
  return scheduler.request(profileId,generation,fileId,full,options);
}
export function clearMediaRequests(profileId?:string){scheduler.clear(profileId);}
export function mediaCacheStats(){return scheduler.stats();}
