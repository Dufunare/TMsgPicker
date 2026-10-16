import {test} from 'node:test';
import assert from 'node:assert/strict';
import {MediaScheduler} from '../src/media/mediaScheduler.ts';

const tick=()=>new Promise<void>(resolve=>setImmediate(resolve));
function deferred<T>(){
  let resolve!:((value:T)=>void),reject!:((reason:unknown)=>void);
  const promise=new Promise<T>((res,rej)=>{resolve=res;reject=rej;});
  return{promise,resolve,reject};
}
test('media scheduler limits per-profile and global concurrency, with avatar priority',async()=>{
  const calls:Array<{id:number;finish:(result:string|null)=>void}>=[];
  const scheduler=new MediaScheduler(async (_profile,id)=>{
    const pending=deferred<string|null>();
    calls.push({id,finish:pending.resolve});
    return pending.promise;
  },2,1);
  const first=scheduler.request('A',1,1,false);
  const full=scheduler.request('A',1,2,true);
  const avatar=scheduler.request('A',1,3,false,{priority:'avatar'});
  const second=scheduler.request('B',1,4,false);
  await tick();
  assert.deepEqual(calls.map(c=>c.id),[1,4]);
  assert.equal(scheduler.stats().active,2);
  calls[0]!.finish('data:image/jpeg;base64,1');await tick();
  assert.deepEqual(calls.map(c=>c.id),[1,4,3]);
  calls[2]!.finish('data:image/jpeg;base64,3');await tick();
  assert.deepEqual(calls.map(c=>c.id),[1,4,3,2]);
  calls[1]!.finish('data:image/jpeg;base64,4');
  calls[3]!.finish('data:image/jpeg;base64,2');
  await Promise.all([first,full,avatar,second]);
  await tick();
  assert.deepEqual(scheduler.stats(),{queued:0,active:0,cached:3,cachedChars:3*'data:image/jpeg;base64,1'.length});
});

test('aborted queued thumbnail never invokes TDLib, and aborted active result is discarded',async()=>{
  const started=deferred<string|null>();
  const calls:number[]=[];
  const scheduler=new MediaScheduler(async(_profile,id)=>{
    calls.push(id);
    return id===10?started.promise:'data:image/png;base64,BBBB';
  },1,1);
  const runningController=new AbortController();
  const first=scheduler.request('A',1,10,false,{signal:runningController.signal});
  const queuedController=new AbortController();
  const queued=scheduler.request('A',1,11,false,{signal:queuedController.signal});
  await tick();
  queuedController.abort();
  assert.equal(await queued,null);
  assert.equal(scheduler.stats().queued,0);
  runningController.abort();
  started.resolve('data:image/png;base64,AAAA');
  assert.equal(await first,null);
  await tick();
  assert.deepEqual(calls,[10]);
  assert.equal(scheduler.stats().cached,0);
});

test('cache is bounded, full media is never cached, and clear invalidates active work',async()=>{
  let reads=0;
  const delayed=deferred<string|null>();
  const scheduler=new MediaScheduler(async (_profile,id,full)=>{
    reads++;
    if(id===99)return delayed.promise;
    return 'data:image/jpeg;base64,'+id+'_'+full;
  },1,1);
  // Keep an intentional delayed item out of the initial cache population.
  for(let id=1;id<=140;id++){if(id===99)continue;await scheduler.request('A',1,id,false);}
  assert.equal(scheduler.stats().cached,128);
  const recent=await scheduler.request('A',1,140,false);
  assert.equal(recent,'data:image/jpeg;base64,140_false');
  assert.equal(reads,139);
  await scheduler.request('A',1,140,true);
  await scheduler.request('A',1,140,true);
  assert.equal(reads,141);
  const stale=scheduler.request('A',1,99,false);
  await tick();
  scheduler.clear('A');
  delayed.resolve('data:image/jpeg;base64,99_false');
  assert.equal(await stale,null);
  await tick();
  assert.equal(scheduler.stats().cached,0);
  await scheduler.request('A',2,140,false);
  assert.equal(reads,143);
});

test('non-abortable duplicate requests reuse one in-flight operation',async()=>{
  const deferredResult=deferred<string|null>();
  let count=0;
  const scheduler=new MediaScheduler(async()=>{count++;return deferredResult.promise;},1,1);
  const one=scheduler.request('A',1,123,false);
  const two=scheduler.request('A',1,123,false);
  assert.equal(one,two);
  await tick();
  assert.equal(count,1);
  deferredResult.resolve('data:image/png;base64,AA');
  assert.equal(await one,await two);
  assert.equal(await scheduler.request('A',1,123,false),'data:image/png;base64,AA');
  assert.equal(count,1);
});
