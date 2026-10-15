import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createAppearanceClient,type AppearanceSnapshot} from '../src/theme/appearanceClient.ts';
import type {Appearance} from '../src/theme/appearance.ts';

const light:Appearance={theme:'light',accent:'#4277cf',density:'comfortable',radius:12,font_scale:100,elevation:'soft',scrollbar:'slim'};
const glass:Appearance={...light,theme:'glass',accent:'#345678',density:'compact',radius:18,font_scale:110,elevation:'raised',scrollbar:'standard'};
type Transport=Parameters<typeof createAppearanceClient>[0];

test('all windows preview every preference; only apply commits, close restores it, newly opened windows inherit the active preview',async()=>{
  const listeners=new Set<(state:AppearanceSnapshot)=>void>();
  let saved={...light},state:AppearanceSnapshot={appearance:saved,revision:0,editor:0},writes=0;
  function broadcast(){listeners.forEach(receive=>receive(state));}
  const transport=(async(command:string,args?:{appearance:Appearance;editor:number})=>{
    if(command==='appearance_state')return state;
    assert.equal(args?.editor,state.editor);
    if(command==='save_appearance'){saved={...args!.appearance};writes++;}
    state={...state,appearance:{...args!.appearance},revision:state.revision+1};broadcast();return state;
  }) as Transport;
  const subscribe=async(receive:(state:AppearanceSnapshot)=>void)=>{listeners.add(receive);return()=>{listeners.delete(receive);};};
  let main!:Appearance,settings!:Appearance,newWindow!:Appearance;
  const editor=createAppearanceClient(transport,subscribe,next=>{settings=next;});
  await editor.start();await createAppearanceClient(transport,subscribe,next=>{main=next;}).start();
  await editor.preview(glass);
  assert.deepEqual(main,glass);assert.deepEqual(settings,glass);assert.deepEqual(saved,light);assert.equal(writes,0);
  await createAppearanceClient(transport,subscribe,next=>{newWindow=next;}).start();assert.deepEqual(newWindow,glass);
  // Native settings destruction broadcasts the saved baseline with a new editor epoch.
  state={appearance:saved,revision:state.revision+1,editor:state.editor+1};broadcast();
  assert.deepEqual(main,light);assert.deepEqual(newWindow,light);assert.deepEqual(settings,light);
  await editor.preview(glass);await editor.save(glass);assert.equal(writes,1);assert.deepEqual(saved,glass);
  await editor.preview({...light,theme:'dark'});assert.equal(main.theme,'dark');
  state={appearance:saved,revision:state.revision+1,editor:state.editor+1};broadcast();
  assert.deepEqual(main,glass);assert.deepEqual(settings,glass);assert.equal(writes,1);
});

test('late startup snapshots and in-flight preview echoes cannot replace a newer draft; queued slider updates coalesce before apply',async()=>{
  let receive!:(state:AppearanceSnapshot)=>void,resolveInitial!:(state:AppearanceSnapshot)=>void,resolvePreview!:(state:AppearanceSnapshot)=>void;
  let shown!:Appearance;const calls:string[]=[];
  const transport=(async(command:string,args?:{appearance:Appearance})=>{
    calls.push(command);
    if(command==='appearance_state')return new Promise<AppearanceSnapshot>(resolve=>{resolveInitial=resolve;});
    if(command==='preview_appearance')return new Promise<AppearanceSnapshot>(resolve=>{resolvePreview=resolve;});
    return {appearance:args!.appearance,revision:4,editor:0};
  }) as Transport;
  const client=createAppearanceClient(transport,async(callback)=>{receive=callback;return()=>{};},next=>{shown=next;});
  const starting=client.start();await Promise.resolve();receive({appearance:glass,revision:1,editor:0});
  resolveInitial({appearance:light,revision:0,editor:0});await starting;assert.deepEqual(shown,glass);
  const first=client.preview(light);await Promise.resolve();await Promise.resolve();
  const second=client.preview({...glass,radius:19});const third=client.preview({...glass,radius:20});
  receive({appearance:light,revision:2,editor:0});assert.equal(shown.radius,20);
  const saving=client.save({...glass,radius:21});
  resolvePreview({appearance:light,revision:2,editor:0});
  await Promise.all([first,second,third,saving]);assert.equal(shown.radius,21);
  assert.deepEqual(calls,['appearance_state','preview_appearance','save_appearance']);
  receive({appearance:light,revision:3,editor:0});assert.equal(shown.radius,21);
});

test('failed application reports an error and leaves the latest preview available for retry',async()=>{
  let shown!:Appearance,fail=true,state:AppearanceSnapshot={appearance:light,revision:0,editor:0};
  const transport=(async(command:string,args?:{appearance:Appearance})=>{
    if(command==='appearance_state')return state;
    if(command==='save_appearance'&&fail)throw new Error('synthetic write failure');
    state={...state,appearance:args!.appearance,revision:state.revision+1};return state;
  }) as Transport;
  const client=createAppearanceClient(transport,async()=>()=>{},next=>{shown=next;});await client.start();
  await client.preview(glass);await assert.rejects(client.save(glass),/synthetic write failure/);assert.deepEqual(shown,glass);
  fail=false;await client.save(glass);assert.deepEqual(shown,glass);
});
