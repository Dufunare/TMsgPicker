import {invoke} from '@tauri-apps/api/core';
import {reactive} from 'vue';

const MAX_ITEMS=96, TTL=15*60*1000, MAX_ACTIVE=4;
type Item={url:string;expires:number};
const cache=new Map<string,Item>();
const inflight=new Map<string,Promise<string>>();
const urls=reactive<Record<string,string>>({});
const generations=new Map<string,number>();
let active=0;
const waiters:Array<()=>void>=[];

export function avatarKey(profile:string,generation:number,id:number|undefined):string {
  return profile+':'+generation+':'+(id??'me');
}
export function cachedAvatar(key:string){ return urls[key]??''; }
async function slot(){
  if(active>=MAX_ACTIVE)await new Promise<void>(resolve=>waiters.push(resolve));
  active++;
}
function release(){
  active=Math.max(0,active-1);
  const wake=waiters.shift();
  if(wake)wake();
}
export async function requestAvatar(profile:string,generation:number,id?:number):Promise<string>{
  if(!profile || !Number.isInteger(generation) || generation<=0)return '';
  if(id!==undefined && (!Number.isInteger(id)||id<=0))return '';
  const key=avatarKey(profile,generation,id);
  const now=Date.now();
  const cached=cache.get(key);
  if(cached&&cached.expires>now){cache.delete(key);cache.set(key,cached);return cached.url;}
  if(cached){cache.delete(key);delete urls[key];}
  const previous=inflight.get(key);
  if(previous)return previous;
  const revision=generations.get(profile)??0;
  const task=(async()=>{
    await slot();
    try{
      if((generations.get(profile)??0)!==revision)return '';
      const response=await invoke<string|null>('photo_thumbnail',{profileId:profile,fileId:id??null});
      if((generations.get(profile)??0)!==revision)return '';
      const url=response?.startsWith('data:image/jpeg;base64,')?response:'';
      if(url.length>350_000)return '';
      cache.set(key,{url,expires:Date.now()+(url?TTL:60_000)});
      urls[key]=url;
      for(const [name,entry] of cache){
        if(entry.expires<=Date.now()){cache.delete(name);delete urls[name];}
      }
      while(cache.size>MAX_ITEMS){
        const oldest=cache.keys().next().value;
        if(!oldest)break;
        cache.delete(oldest);delete urls[oldest];
      }
      return url;
    }catch {return '';}
    finally{release();}
  })().finally(()=>inflight.delete(key));
  inflight.set(key,task);
  return task;
}
export function clearAvatars(profile?:string){
  if(!profile) {
    const profiles=new Set([...cache.keys(),...inflight.keys()].map(k=>k.split(':')[0]!));
    for(const id of profiles)generations.set(id,(generations.get(id)??0)+1);
  } else generations.set(profile,(generations.get(profile)??0)+1);
  for(const key of [...cache.keys()]){
    if(!profile||key.startsWith(profile+':')){cache.delete(key);delete urls[key];}
  }
  for(const key of [...inflight.keys()])if(!profile||key.startsWith(profile+':'))inflight.delete(key);
}
