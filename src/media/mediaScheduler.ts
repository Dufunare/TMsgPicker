// Session-scoped media scheduler. Expand may request original photos automatically;
// concurrent viewers share a file request, but each viewer may cancel separately.
// TDLib synchronous downloads cannot be forcibly interrupted once submitted.
export type MediaPriority='full'|'avatar'|'thumbnail';
export interface MediaOptions {priority?:MediaPriority;signal?:AbortSignal}
type FetchMedia=(profileId:string,fileId:number,full:boolean)=>Promise<string|null>;
interface CacheEntry {value:string|null;expires:number;weight:number}
interface Subscriber {
  resolve:(value:string|null)=>void;
  reject:(reason:unknown)=>void;
  signal?:AbortSignal;
  onAbort?:()=>void;
}
interface Job {
  key:string;profileId:string;fileId:number;full:boolean;revision:number;
  priority:number;started:boolean;subscribers:Set<Subscriber>;
  sharedPromise?:Promise<string|null>;
}
const THUMB_MAX_ITEMS=128, THUMB_MAX_CHARS=12*1024*1024, THUMB_MAX_ENTRY=512*1024;
const FULL_MAX_ITEMS=8, FULL_MAX_CHARS=64*1024*1024, FULL_MAX_ENTRY=36*1024*1024;
const POSITIVE_TTL_MS=5*60*1000, NEGATIVE_TTL_MS=10*1000;
// Opening media is an explicit request to see originals. A queued original
// outranks ordinary thumbnails/avatars; work already running is not preempted.
const priorities:Record<MediaPriority,number>={full:0,avatar:1,thumbnail:2};

export class MediaScheduler {
  private readonly fetchMedia:FetchMedia;
  private readonly maxGlobal:number;
  private readonly maxPerProfile:number;
  private readonly queued:Job[]=[];
  private readonly jobs=new Map<string,Job>();
  private readonly busyByProfile=new Map<string,number>();
  private readonly thumbnails=new Map<string,CacheEntry>();
  private readonly originals=new Map<string,CacheEntry>();
  private readonly revisions=new Map<string,number>();
  private thumbnailChars=0;
  private originalChars=0;
  private active=0;

  constructor(fetchMedia:FetchMedia,maxGlobal=2,maxPerProfile=1){
    this.fetchMedia=fetchMedia;
    this.maxGlobal=maxGlobal;
    this.maxPerProfile=maxPerProfile;
  }

  private key(profileId:string,generation:number,fileId:number,full:boolean):string {
    return [profileId,generation,fileId,full].join(':');
  }
  private cacheFor(full:boolean):Map<string,CacheEntry>{
    return full?this.originals:this.thumbnails;
  }
  private lookup(key:string,full:boolean):CacheEntry|undefined {
    const cache=this.cacheFor(full);
    const item=cache.get(key);
    if(!item)return undefined;
    if(item.expires<=Date.now()) {this.evict(key,full);return undefined;}
    cache.delete(key);cache.set(key,item);
    return item;
  }
  /** Only positive entries are exposed for immediately restoring a reopened image. */
  peek(profileId:string,generation:number,fileId:number,full:boolean):string|null {
    if(!profileId||generation<=0||fileId<=0)return null;
    return this.lookup(this.key(profileId,generation,fileId,full),full)?.value??null;
  }
  request(profileId:string,generation:number,fileId:number,full:boolean,options:MediaOptions={}):Promise<string|null>{
    if(!profileId||!Number.isInteger(generation)||generation<=0
      ||!Number.isInteger(fileId)||fileId<0||options.signal?.aborted)
      return Promise.resolve(null);
    const key=this.key(profileId,generation,fileId,full);
    const cached=this.lookup(key,full);
    if(cached)return Promise.resolve(cached.value);
    const revision=this.revisions.get(profileId)??0;
    let job=this.jobs.get(key);
    if(!job || job.revision!==revision){
      job={key,profileId,fileId,full,revision,
        priority:priorities[options.priority??(full?'full':'thumbnail')],
        started:false,subscribers:new Set()};
      this.jobs.set(key,job);
      this.queued.push(job);
    }else{
      // A new high-priority subscriber can promote an already queued request.
      job.priority=Math.min(job.priority,priorities[options.priority??(full?'full':'thumbnail')]);
    }
    if(!options.signal&&job.sharedPromise)return job.sharedPromise;
    const flight=job;
    const promise=new Promise<string|null>((resolve,reject)=>{
      const subscriber:Subscriber={resolve,reject,signal:options.signal};
      flight.subscribers.add(subscriber);
      if(options.signal){
        subscriber.onAbort=()=>{
          if(!flight.subscribers.delete(subscriber))return;
          options.signal?.removeEventListener('abort',subscriber.onAbort!);
          resolve(null);
          // One unmounted card must not cancel the same file for another card.
          if(!flight.started&&flight.subscribers.size===0){
            const index=this.queued.indexOf(flight);
            if(index>=0)this.queued.splice(index,1);
            if(this.jobs.get(key)===flight)this.jobs.delete(key);
            this.pump();
          }
          // An active TDLib download cannot be cancelled safely here. Keep
          // the flight joinable for a quick collapse/reopen until it finishes.
        };
        options.signal.addEventListener('abort',subscriber.onAbort,{once:true});
        if(options.signal.aborted)subscriber.onAbort();
      }
    });
    if(!options.signal)flight.sharedPromise=promise;
    this.pump();
    return promise;
  }

  clear(profileId?:string):void {
    const matches=(id:string)=>profileId===undefined||id===profileId;
    const ids=new Set<string>([
      ...this.revisions.keys(),...[...this.jobs.values()].map(job=>job.profileId),
      ...this.busyByProfile.keys(),
      ...[...this.thumbnails.keys(),...this.originals.keys()].map(k=>k.split(':')[0]!),
    ]);
    if(profileId!==undefined)ids.add(profileId);
    for(const id of ids)if(matches(id))this.revisions.set(id,(this.revisions.get(id)??0)+1);
    for(const job of [...this.jobs.values()]){
      if(!matches(job.profileId))continue;
      if(!job.started){
        const pos=this.queued.indexOf(job);
        if(pos>=0)this.queued.splice(pos,1);
      }
      if(this.jobs.get(job.key)===job)this.jobs.delete(job.key);
      this.settle(job,null);
    }
    for(const key of [...this.thumbnails.keys()])if(matches(key.split(':')[0]!))this.evict(key,false);
    for(const key of [...this.originals.keys()])if(matches(key.split(':')[0]!))this.evict(key,true);
    this.pump();
  }

  stats(){
    return {queued:this.queued.length,active:this.active,
      cached:this.thumbnails.size+this.originals.size,
      cachedChars:this.thumbnailChars+this.originalChars,
      cachedOriginals:this.originals.size,originalChars:this.originalChars};
  }

  private evict(key:string,full:boolean){
    const cache=this.cacheFor(full),entry=cache.get(key);
    if(!entry)return;
    cache.delete(key);
    if(full)this.originalChars-=entry.weight;
    else this.thumbnailChars-=entry.weight;
  }
  private remember(key:string,full:boolean,value:string|null){
    const length=value?.length??0;
    const maxEntry=full?FULL_MAX_ENTRY:THUMB_MAX_ENTRY;
    if(length>maxEntry || (value!==null&&!value.startsWith('data:image/')))return;
    const cache=this.cacheFor(full);
    this.evict(key,full);
    cache.set(key,{value,expires:Date.now()+(value===null?NEGATIVE_TTL_MS:POSITIVE_TTL_MS),weight:length});
    if(full)this.originalChars+=length;
    else this.thumbnailChars+=length;
    for(const [name,item] of cache)if(item.expires<=Date.now())this.evict(name,full);
    const maxCount=full?FULL_MAX_ITEMS:THUMB_MAX_ITEMS;
    const maxChars=full?FULL_MAX_CHARS:THUMB_MAX_CHARS;
    while(cache.size>maxCount || (full?this.originalChars:this.thumbnailChars)>maxChars){
      const oldest=cache.keys().next().value;
      if(oldest===undefined)break;
      this.evict(oldest,full);
    }
  }
  private settle(job:Job,value:string|null,error?:unknown){
    for(const subscriber of job.subscribers){
      subscriber.signal?.removeEventListener('abort',subscriber.onAbort!);
      if(error===undefined)subscriber.resolve(value);
      else subscriber.reject(error);
    }
    job.subscribers.clear();
  }
  private pump(){
    while(this.active<this.maxGlobal){
      let best=-1;
      for(let i=0;i<this.queued.length;i++){
        const job=this.queued[i]!;
        if((this.busyByProfile.get(job.profileId)??0)>=this.maxPerProfile)continue;
        if(best===-1||job.priority<this.queued[best]!.priority)best=i;
      }
      if(best===-1)break;
      const job=this.queued.splice(best,1)[0]!;
      if(job.subscribers.size===0){
        if(this.jobs.get(job.key)===job)this.jobs.delete(job.key);
        continue;
      }
      job.started=true;
      this.active++;
      this.busyByProfile.set(job.profileId,(this.busyByProfile.get(job.profileId)??0)+1);
      void Promise.resolve()
        .then(()=>{
          if(job.revision!==(this.revisions.get(job.profileId)??0)||job.subscribers.size===0)return null;
          return this.fetchMedia(job.profileId,job.fileId,job.full);
        })
        .then(value=>{
          const valid=job.revision===(this.revisions.get(job.profileId)??0);
          if(this.jobs.get(job.key)===job)this.jobs.delete(job.key);
          if(valid&&job.subscribers.size>0)this.remember(job.key,job.full,value);
          this.settle(job,valid?value:null);
        },error=>{
          if(this.jobs.get(job.key)===job)this.jobs.delete(job.key);
          const valid=job.revision===(this.revisions.get(job.profileId)??0);
          this.settle(job,null,valid?error:undefined);
        })
        .finally(()=>{
          this.active--;
          const remaining=(this.busyByProfile.get(job.profileId)??1)-1;
          if(remaining<=0)this.busyByProfile.delete(job.profileId);
          else this.busyByProfile.set(job.profileId,remaining);
          this.pump();
        });
    }
  }
}
