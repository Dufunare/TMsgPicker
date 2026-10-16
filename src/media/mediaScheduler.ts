// Pure, testable scheduler: native media requests run outside the WebView's
// render loop, with bounded concurrency and a small completed-thumbnail LRU.
// A pending thumbnail belonging to an unmounted component can be abandoned.
export type MediaPriority='avatar'|'thumbnail'|'full';
export interface MediaOptions {priority?:MediaPriority;signal?:AbortSignal}
type FetchMedia=(profileId:string,fileId:number,full:boolean)=>Promise<string|null>;
interface CacheEntry {value:string|null;expires:number;weight:number}
interface Job {
  key:string;profileId:string;fileId:number;full:boolean;revision:number;
  priority:number;signal?:AbortSignal;onAbort?:()=>void;started:boolean;
  resolve:(value:string|null)=>void;reject:(error:unknown)=>void;
}
const MAX_CACHE_ITEMS=128;
const MAX_CACHE_CHARS=12*1024*1024;
const MAX_ENTRY_CHARS=512*1024;
const POSITIVE_TTL_MS=5*60*1000;
const NEGATIVE_TTL_MS=10*1000;
const priorities:Record<MediaPriority,number>={avatar:0,thumbnail:1,full:2};

export class MediaScheduler {
  private readonly queued:Job[]=[];
  private readonly busyByProfile=new Map<string,number>();
  private readonly shared=new Map<string,Promise<string|null>>();
  private readonly cache=new Map<string,CacheEntry>();
  private readonly revisions=new Map<string,number>();
  private totalCacheChars=0;
  private active=0;

  constructor(
    private readonly fetchMedia:FetchMedia,
    private readonly maxGlobal=2,
    private readonly maxPerProfile=1,
  ) {}

  request(profileId:string,generation:number,fileId:number,full:boolean,options:MediaOptions={}):Promise<string|null>{
    if(!profileId||!Number.isInteger(generation)||generation<=0||!Number.isInteger(fileId)||fileId<0||options.signal?.aborted)
      return Promise.resolve(null);
    const key=[profileId,generation,fileId,full].join(':');
    if(!full) {
      const hit=this.cache.get(key);
      if(hit) {
        if(hit.expires>Date.now()) {
          this.cache.delete(key);this.cache.set(key,hit); // LRU touch
          return Promise.resolve(hit.value);
        }
        this.evict(key);
      }
    }
    if(!options.signal) {
      const existing=this.shared.get(key);
      if(existing)return existing;
    }
    const promise=new Promise<string|null>((resolve,reject)=>{
      const job:Job={key,profileId,fileId,full,revision:this.revisions.get(profileId)??0,
        priority:priorities[options.priority??(full?'full':'thumbnail')],
        signal:options.signal,started:false,resolve,reject};
      if(options.signal){
        job.onAbort=()=>{
          if(job.started)return; // TDLib call cannot be reliably aborted here.
          const position=this.queued.indexOf(job);
          if(position>=0)this.queued.splice(position,1);
          options.signal?.removeEventListener('abort',job.onAbort!);
          resolve(null);
          this.pump();
        };
        options.signal.addEventListener('abort',job.onAbort,{once:true});
        if(options.signal.aborted){job.onAbort();return;}
      }
      this.queued.push(job);
      this.pump();
    });
    if(!options.signal){
      this.shared.set(key,promise);
      void promise.finally(()=>{if(this.shared.get(key)===promise)this.shared.delete(key);}).catch(()=>{});
    }
    return promise;
  }

  clear(profileId?:string):void {
    const matches=(id:string)=>profileId===undefined||id===profileId;
    const ids=new Set<string>([
      ...this.revisions.keys(),...this.queued.map(job=>job.profileId),
      ...this.busyByProfile.keys(),...[...this.cache.keys()].map(k=>k.split(':')[0]!),
    ]);
    if(profileId!==undefined)ids.add(profileId);
    for(const id of ids)if(matches(id))this.revisions.set(id,(this.revisions.get(id)??0)+1);
    for(let i=this.queued.length-1;i>=0;i--){
      const job=this.queued[i]!;
      if(!matches(job.profileId))continue;
      this.queued.splice(i,1);
      job.signal?.removeEventListener('abort',job.onAbort!);
      job.resolve(null);
    }
    for(const key of [...this.cache.keys()])if(matches(key.split(':')[0]!))this.evict(key);
    for(const key of [...this.shared.keys()])if(matches(key.split(':')[0]!))this.shared.delete(key);
    this.pump();
  }

  stats(){return{queued:this.queued.length,active:this.active,cached:this.cache.size,cachedChars:this.totalCacheChars};}

  private evict(key:string){
    const old=this.cache.get(key);
    if(old){this.totalCacheChars-=old.weight;this.cache.delete(key);}
  }
  private remember(key:string,value:string|null){
    const length=value?.length??0;
    if(length>MAX_ENTRY_CHARS || (value!==null&&!value.startsWith('data:image/')))return;
    this.evict(key);
    const entry:CacheEntry={value,expires:Date.now()+(value===null?NEGATIVE_TTL_MS:POSITIVE_TTL_MS),weight:length};
    this.cache.set(key,entry);this.totalCacheChars+=length;
    for(const [name,item] of this.cache){
      if(item.expires<=Date.now())this.evict(name);
    }
    while(this.cache.size>MAX_CACHE_ITEMS||this.totalCacheChars>MAX_CACHE_CHARS){
      const oldest=this.cache.keys().next().value;
      if(oldest===undefined)break;
      this.evict(oldest);
    }
  }
  private pump(){
    while(this.active<this.maxGlobal) {
      let best=-1;
      for(let i=0;i<this.queued.length;i++){
        const job=this.queued[i]!;
        if((this.busyByProfile.get(job.profileId)??0)>=this.maxPerProfile)continue;
        if(best===-1||job.priority<this.queued[best]!.priority)best=i;
      }
      if(best===-1)break;
      const job=this.queued.splice(best,1)[0]!;
      if(job.signal?.aborted){job.resolve(null);continue;}
      job.started=true;
      this.active++;
      this.busyByProfile.set(job.profileId,(this.busyByProfile.get(job.profileId)??0)+1);
      void Promise.resolve().then(()=>this.fetchMedia(job.profileId,job.fileId,job.full))
        .then(value=>{
          const valid=!(job.signal?.aborted)&&job.revision===(this.revisions.get(job.profileId)??0);
          if(valid&&!job.full)this.remember(job.key,value);
          job.resolve(valid?value:null);
        },error=>{
          if(job.signal?.aborted||job.revision!==(this.revisions.get(job.profileId)??0))job.resolve(null);
          else job.reject(error);
        }).finally(()=>{
          job.signal?.removeEventListener('abort',job.onAbort!);
          this.active--;
          const next=(this.busyByProfile.get(job.profileId)??1)-1;
          if(next===0)this.busyByProfile.delete(job.profileId);
          else this.busyByProfile.set(job.profileId,next);
          this.pump();
        });
    }
  }
}
