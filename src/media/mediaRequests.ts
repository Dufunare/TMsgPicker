import {invoke} from '@tauri-apps/api/core';

// TDLib operations on a profile currently hold one account lifecycle lock.
// Serialized image requests avoid lost previews when an album is expanded all at once.
let tail:Promise<unknown>=Promise.resolve();
const inflight=new Map<string,Promise<string|null>>();
export function requestMedia(profileId:string,generation:number,fileId:number,full:boolean){
 if(!profileId||generation<=0||fileId<0)return Promise.resolve(null);
 const key=[profileId,generation,fileId,full].join(':');
 const old=inflight.get(key);if(old)return old;
 const run=async():Promise<string|null>=>{
  for(let attempt=0;attempt<3;attempt++){
   try{return await invoke<string|null>('photo_thumbnail',{profileId,fileId:fileId===0?null:fileId,full});}
   catch(error){if(!String(error).includes('Session operation in progress')||attempt===2)throw error;await new Promise(resolve=>setTimeout(resolve,100*(attempt+1)));}
  }
  return null;
 };
 const task=tail.then(run);
 tail=task.catch(()=>{});
 inflight.set(key,task);
 void task.finally(()=>{if(inflight.get(key)===task)inflight.delete(key);}).catch(()=>{});
 return task;
}
