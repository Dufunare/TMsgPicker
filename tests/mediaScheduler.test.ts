import {test} from 'node:test';
import assert from 'node:assert/strict';
import {MediaScheduler} from '../src/media/mediaScheduler.ts';

const tick=()=>new Promise<void>(resolve=>setImmediate(resolve));
function deferred<T>(){
  let resolve!:((value:T)=>void),reject!:(reason:unknown)=>void;
  const promise=new Promise<T>((res,rej)=>{resolve=res;reject=rej;});
  return{promise,resolve,reject};
}
const image=(id:number)=>'data:image/jpeg;base64,'+id;

test('per-profile/global limits: automatic originals beat queued thumbnails and avatars',async()=>{
  const calls:Array<{id:number;finish:(value:string|null)=>void}>=[];
  const scheduler=new MediaScheduler(async(_profile,id)=>{
    const d=deferred<string|null>();
    calls.push({id,finish:d.resolve});
    return d.promise;
  },2,1);
  const first=scheduler.request('A',1,1,false);
  const thumb=scheduler.request('A',1,2,false,{priority:'thumbnail'});
  const avatar=scheduler.request('A',1,3,false,{priority:'avatar'});
  const original=scheduler.request('A',1,4,true,{priority:'full'});
  const other=scheduler.request('B',1,5,false);
  await tick();
  assert.deepEqual(calls.map(c=>c.id),[1,5]);
  assert.equal(scheduler.stats().active,2);
  calls[0]!.finish(image(1));await tick();
  assert.deepEqual(calls.map(c=>c.id),[1,5,4]);
  calls[2]!.finish(image(4));await tick();
  assert.deepEqual(calls.map(c=>c.id),[1,5,4,3]);
  calls[3]!.finish(image(3));await tick();
  assert.deepEqual(calls.map(c=>c.id),[1,5,4,3,2]);
  calls[4]!.finish(image(2));calls[1]!.finish(image(5));
  await Promise.all([first,thumb,avatar,original,other]);
  await tick();
  assert.equal(scheduler.stats().active,0);
  assert.equal(scheduler.stats().cachedOriginals,1);
});

test('same file requested with independent AbortSignals downloads only once',async()=>{
  const delayed=deferred<string|null>();
  let reads=0;
  const scheduler=new MediaScheduler(async()=>{reads++;return delayed.promise;},1,1);
  const a=new AbortController(),b=new AbortController();
  const first=scheduler.request('A',1,10,true,{signal:a.signal});
  const second=scheduler.request('A',1,10,true,{signal:b.signal});
  await tick();assert.equal(reads,1);
  a.abort();
  assert.equal(await first,null);
  delayed.resolve(image(10));
  assert.equal(await second,image(10));
  await tick();
  assert.equal(reads,1);
  assert.equal(scheduler.peek('A',1,10,true),image(10));
});

test('collapse/reopen joins an already active original without another native call',async()=>{
  const delayed=deferred<string|null>();
  let calls=0;
  const scheduler=new MediaScheduler(async()=>{calls++;return delayed.promise;},1,1);
  const closed=new AbortController();
  const before=scheduler.request('A',1,20,true,{signal:closed.signal});
  await tick();assert.equal(calls,1);
  closed.abort();assert.equal(await before,null);
  const reopened=scheduler.request('A',1,20,true,{signal:new AbortController().signal});
  await tick();assert.equal(calls,1);
  delayed.resolve(image(20));
  assert.equal(await reopened,image(20));
  await tick();
  assert.equal(scheduler.peek('A',1,20,true),image(20));
  assert.equal(await scheduler.request('A',1,20,true),image(20));
  assert.equal(calls,1);
});

test('aborting every queued subscriber removes only that job',async()=>{
  const delayed=deferred<string|null>();
  const calls:number[]=[];
  const scheduler=new MediaScheduler(async(_profile,id)=>{
    calls.push(id);return id===1?delayed.promise:image(id);
  },1,1);
  const blocker=scheduler.request('A',1,1,false);
  const a=new AbortController(),b=new AbortController();
  const one=scheduler.request('A',1,2,false,{signal:a.signal});
  const two=scheduler.request('A',1,2,false,{signal:b.signal});
  await tick();a.abort();assert.equal(await one,null);
  assert.equal(scheduler.stats().queued,1);
  b.abort();assert.equal(await two,null);
  assert.equal(scheduler.stats().queued,0);
  delayed.resolve(image(1));await blocker;await tick();
  assert.deepEqual(calls,[1]);
});

test('a late high-priority subscriber can promote one queued job without duplicating it',async()=>{
  const wait=deferred<string|null>();
  const calls:number[]=[];
  const scheduler=new MediaScheduler(async(_profile,id)=>{
    calls.push(id);return id===1?wait.promise:image(id);
  },1,1);
  const one=scheduler.request('A',1,1,false);
  const low=scheduler.request('A',1,2,false,{priority:'thumbnail'});
  const next=scheduler.request('A',1,3,false,{priority:'thumbnail'});
  const promoted=scheduler.request('A',1,3,false,{priority:'full'});
  await tick();wait.resolve(image(1));await one;await tick();
  assert.deepEqual(calls.slice(0,2),[1,3]);
  await Promise.all([low,next,promoted]);
  assert.deepEqual(calls,[1,3,2]);
});

test('thumbnail and original caches have independent bounded LRU capacities',async()=>{
  let reads=0;
  const scheduler=new MediaScheduler(async(_profile,id,full)=>{
    reads++;return image(id)+(full?'_original':'_thumb');
  },1,1);
  for(let i=1;i<=140;i++)await scheduler.request('A',1,i,false);
  assert.equal(scheduler.stats().cached,128);
  assert.equal(scheduler.peek('A',1,1,false),null);
  assert.equal(scheduler.peek('A',1,140,false),image(140)+'_thumb');
  for(let i=1;i<=10;i++)await scheduler.request('A',1,i,true);
  assert.equal(scheduler.stats().cachedOriginals,8);
  assert.equal(scheduler.peek('A',1,1,true),null);
  assert.equal(scheduler.peek('A',1,10,true),image(10)+'_original');
  assert.equal(await scheduler.request('A',1,10,true),image(10)+'_original');
  assert.equal(reads,150);
  scheduler.clear('A');
  assert.equal(scheduler.stats().cached,0);
  assert.equal(scheduler.stats().cachedOriginals,0);
  assert.equal(scheduler.peek('A',1,10,true),null);
  assert.equal(await scheduler.request('A',2,10,true),image(10)+'_original');
  assert.equal(reads,151);
});

test('clear invalidates active subscribers without erasing other profiles',async()=>{
  const delayed=deferred<string|null>();
  const scheduler=new MediaScheduler(async(profile,id)=>{
    if(profile==='A'&&id===99)return delayed.promise;
    return image(id);
  },2,1);
  await scheduler.request('B',1,10,true);
  const stale=scheduler.request('A',1,99,true,{signal:new AbortController().signal});
  await tick();
  scheduler.clear('A');
  assert.equal(await stale,null);
  delayed.resolve(image(99));
  await tick();
  assert.equal(scheduler.peek('A',1,99,true),null);
  assert.equal(scheduler.peek('B',1,10,true),image(10));
});

test('non-abortable duplicate requests still reuse the same Promise',async()=>{
  const delayed=deferred<string|null>();
  let calls=0;
  const scheduler=new MediaScheduler(async()=>{calls++;return delayed.promise;},1,1);
  const one=scheduler.request('A',1,123,false);
  const two=scheduler.request('A',1,123,false);
  assert.equal(one,two);
  await tick();assert.equal(calls,1);
  delayed.resolve(image(123));
  assert.equal(await one,await two);
  assert.equal(await scheduler.request('A',1,123,false),image(123));
  assert.equal(calls,1);
});
