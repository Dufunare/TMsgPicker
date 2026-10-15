import {invoke} from '@tauri-apps/api/core';
import {reactive} from 'vue';

const MAX_ITEMS=96;
const TTL=15*60*1000;
type Item={url:string;expires:number};
const cache=new Map<string,Item>();
const inflight=new Map<string,Promise<string>>();
const urls=reactive<Record<string,string>>({});

export function avatarKey(profile:string,generation:number,id:number|undefined):string {
  return profile+':'+generation+':'+(id??'me');
}
export function cachedAvatar(key:string){ return urls[key]??''; }
export async function requestAvatar(profile:string,generation:number,id?:number):Promise<string>{
  if(!profile || !Number.isInteger(generation) || generation<=0)return '';
  if(id!==undefined && (!Number.isInteger(id)||id<=0))return '';
  const key=avatarKey(profile,generation,id);
  const hit=cache.get(key);
  if(hit&&hit.expires>Date.now()){cache.delete(key);cache.set(key,hit);return hit.url;}
  const previous=inflight.get(key);
  if(previous)return previous;
  const task=invoke<string|null>('photo_thumbnail',{profileId:profile,fileId:id??null}).then(value=>{
    const url=value?.startsWith('data:image/jpeg;base64,')?value:'';
    if(url.length>350_000)return '';
    cache.delete(key);cache.set(key,{url,expires:Date.now()+(url?TTL:60_000)});
    urls[key]=url;
    while(cache.size>MAX_ITEMS){
      const oldest=cache.keys().next().value;
      if(!oldest)break;
      cache.delete(oldest);delete urls[oldest];
    }
    return url;
  }).catch(()=> '').finally(()=>inflight.delete(key));
  inflight.set(key,task);
  return task;
}
export function clearAvatars(profile?:string){
  for(const key of [...cache.keys()]){
    if(!profile||key.startsWith(profile+':')){cache.delete(key);delete urls[key];}
  }
  for(const key of [...inflight.keys()])if(!profile||key.startsWith(profile+':'))inflight.delete(key);
}
