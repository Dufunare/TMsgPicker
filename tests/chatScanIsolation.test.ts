import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createSSRApp,createRenderer,h,nextTick,provide} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {readFile,writeFile,unlink} from 'node:fs/promises';
import {parse,compileScript} from 'vue/compiler-sfc';
import ts from 'typescript';
import {createAccountViews} from '../src/accountViews.ts';
import {useWorkspace,workspaceKey} from '../src/useWorkspace.ts';
import type {Chat,ScanResult} from '../src/types.ts';

const chatA:Chat={id:'chat-a',title:'Chat A',kind:'group'};
const chatB:Chat={id:'chat-b',title:'Chat B',kind:'group'};
const post={key:'post-a',kind:'single' as const,members:[{key:{chat_id:chatA.id,message_id:'1'},date:1,content_kind:'text',text:'A post',media_album_id:null,metrics:{views:7,forwards:null,reply_info:null,reactions:null}}],representative:{chat_id:chatA.id,message_id:'1'},date:1,text:'A post',metrics:{views:7,forwards:null,reply_info:null,reactions:null},provenance:{},completeness:{level:'Verified' as const,reason:''}};
const result=(runId:string,status:ScanResult['status']='Complete'):ScanResult=>({
  run_id:runId,status,stop_reason:status==='Complete'?'PostLimitReached':'UserCancelled',detail:'A scan',
  scope:{kind:'recent',count:1},
  coverage:{requested_posts:1,examined_posts:1,scope_complete:status==='Complete',date_window_exhausted:null},
  stats:{raw_messages_read:1,posts_finalized:1,posts_matched:1,posts_returned:1,pages:1},
  scanned:1,pages:1,exact:status==='Complete',posts:[post],
});
const status=(id:string)=>({profile_id:id,generation:1,runtime_state:'Ready',auth_state:'authorizationStateReady',connection_state:'connectionStateReady',detail:'',user_disconnected:false});
function running(state:ReturnType<typeof createAccountViews>,profile:string,chat:Chat,runId:string){
  state.connection(status(profile));state.selectChat(profile,chat);
  const view=state.get(profile);view.currentRun=runId;view.scanChatId=chat.id;view.scanning=true;
  return view;
}

test('A/B/E: Complete, Interrupted, BudgetExceeded and Error all disappear on A → B → A',()=>{
  for(const scanStatus of ['Complete','Interrupted','BudgetExceeded','Error'] as const){
    const state=createAccountViews(),view=running(state,'profile',chatA,'run-1');
    state.progress({profile_id:'profile',generation:1,result:result('run-1',scanStatus)});
    assert.equal(view.result?.status,scanStatus);
    view.error='old query error';view.selected=['post-a'];view.confirmation=true;
    view.preview={items:[]};view.confirmedSelection={chatId:chatA.id,target:'target',keys:['post-a'],runId:'run-1'};
    view.rawCount=73;view.sortField='date';view.filter={kind:'is_not_null',field:{kind:'views'}};
    view.chatList=[chatA,chatB];view.accountError='unrelated account error';
    state.selectChat('profile',chatB);
    assert.equal(view.selectedChat?.title,'Chat B');assert.equal(view.result,null);
    assert.equal(view.currentRun,'');assert.equal(view.scanChatId,'');assert.equal(view.scanning,false);
    assert.deepEqual(view.selected,[]);assert.equal(view.confirmation,false);
    assert.equal(view.preview,null);assert.equal(view.confirmedSelection,null);assert.equal(view.error,'');
    assert.equal(view.rawCount,73);assert.equal(view.sortField,'date');
    assert.deepEqual(view.filter,{kind:'is_not_null',field:{kind:'views'}});
    assert.equal(view.chatList.length,2);assert.equal(view.accountError,'unrelated account error');
    state.selectChat('profile',chatA);
    assert.equal(view.result,null);
    assert.equal(view.confirmedSelection,null);
  }
});

test('A/B: the actual results panel renders B with no A badge, statistics or Posts',async()=>{
  const url=new URL('../src/components/MessageResultsPanel.vue',import.meta.url);
  const {descriptor}=parse(await readFile(url,'utf8'));
  const source=compileScript(descriptor,{id:'chat-scan-isolation',inlineTemplate:true}).content
    .replace(/from ['"]\.\.\/useWorkspace['"]/g,"from '../useWorkspace.ts'")
    .replace(/from ['"]\.\.\/scan\/scanUi['"]/g,"from '../scan/scanUi.ts'")
    .replace(/import LinkedText from [^;]+;/,"import {h as stubH} from 'vue';const LinkedText={props:['text'],render(){return stubH('span',null,this.text);}};")
    .replace(/import MediaPreview from [^;]+;/,"const MediaPreview={render(){return null;}};");
  const temporary=new URL(`.chat-scan-isolation-${process.pid}.mjs`,url);
  await writeFile(temporary,ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText);
  try{
    const panel=(await import(temporary.href)).default;
    const w=await workspace(async()=>{});
    const render=()=>renderToString(createSSRApp({setup(){provide(workspaceKey,w);return()=>h(panel);}}));
    for(const scanStatus of ['Complete','Interrupted','BudgetExceeded','Error'] as const){
      w.selectChat(chatA);w.currentRun.value='run-1';w.scanChatId.value=chatA.id;w.result.value=result('run-1',scanStatus);
      const before=await render();
      assert.match(before,/Chat A/);assert.match(before,/范围内 1 个 Post/);assert.match(before,/A post/);
      w.selectChat(chatB);
      const after=await render();
      assert.match(after,/Chat B/);assert.match(after,/请在右侧 Query Builder 执行查询/);
      for(const stale of ['Chat A','A post','范围内 1 个 Post','匹配 1 个',scanStatus])assert.equal(after.includes(stale),false,stale);
    }
    type Element={tag:string;props:Record<string,any>;text:string};
    const elements:Element[]=[];
    const renderer=createRenderer<Element,Element>({
      createElement:tag=>{const el={tag,props:{},text:''};elements.push(el);return el;},
      createText:text=>({tag:'#text',props:{},text}),createComment:()=>({tag:'#comment',props:{},text:''}),
      insert(){},remove(){},setText(el,text){el.text=text;},setElementText(el,text){el.text=text;},
      patchProp(el,key,_old,value){el.props[key]=value;},parentNode:()=>null,nextSibling:()=>null,
    });
    const photo={...post,members:[{...post.members[0]!,content_kind:'photo'}]};
    w.selectChat(chatA);w.currentRun.value='run-a';w.scanChatId.value=chatA.id;
    w.result.value={...result('run-a'),posts:[photo]};
    const app=renderer.createApp({setup(){provide(workspaceKey,w);return()=>h(panel);}});
    app.mount({tag:'root',props:{},text:''});
    try{
      const expander=()=>elements.filter(el=>el.props.class==='media-expander').at(-1);
      assert.equal(String(expander()?.props['aria-expanded']),'false');
      expander()?.props.onClick();await nextTick();
      assert.equal(String(expander()?.props['aria-expanded']),'true');
      w.selectChat(chatB);w.currentRun.value='run-b';w.scanChatId.value=chatB.id;
      w.result.value={...result('run-b'),posts:[{...photo,representative:{...photo.representative,chat_id:chatB.id}}]};
      await nextTick();
      assert.equal(String(expander()?.props['aria-expanded']),'false');
    }finally{app.unmount();}
  }finally{await unlink(temporary);}
});

test('C/D: late progress and final result from run-1 cannot replace B run-2',()=>{
  const state=createAccountViews(),view=running(state,'profile',chatA,'run-1');
  assert.equal(state.selectChat('profile',chatB),'run-1');
  state.progress({profile_id:'profile',generation:1,result:result('run-1')});
  assert.equal(view.result,null);assert.equal(view.scanning,false);
  view.currentRun='run-2';view.scanChatId=chatB.id;view.scanning=true;
  state.progress({profile_id:'profile',generation:1,result:{...result('run-1'),status:'Running'}});
  assert.equal(view.result,null);
  state.progress({profile_id:'profile',generation:1,result:result('run-2')});
  assert.equal(view.result?.run_id,'run-2');assert.equal(view.scanning,false);
  state.progress({profile_id:'profile',generation:2,result:result('run-2','Error')});
  assert.equal(view.result?.status,'Complete');
});

test('F: a new Chat object with the same ID updates metadata without invalidating the scan',()=>{
  const state=createAccountViews(),view=running(state,'profile',chatA,'run-1');
  state.progress({profile_id:'profile',generation:1,result:result('run-1')});
  const sameId={...chatA,title:'Updated title',photo_file_id:42};
  assert.equal(state.selectChat('profile',sameId),null);
  assert.equal(view.selectedChat?.title,'Updated title');assert.equal(view.selectedChat?.photo_file_id,42);
  assert.equal(view.result?.run_id,'run-1');assert.equal(view.scanChatId,chatA.id);
});

test('F: refreshing the chat list updates the selected title without losing its run',async()=>{
  const w=await workspace(async command=>{
    if(command==='chats')return [{...chatA,title:'Refreshed Chat A',photo_file_id:99}];
  });
  w.statuses.profile=status('profile');
  w.selectChat(chatA);w.currentRun.value='run-1';w.scanChatId.value=chatA.id;
  w.result.value=result('run-1');
  await w.loadChats();
  assert.equal(w.selectedChat.value?.title,'Refreshed Chat A');
  assert.equal(w.selectedChat.value?.photo_file_id,99);
  assert.equal(w.activeResult.value?.run_id,'run-1');
});

test('G/H: a chat switch preserves delivery receipts and another account view',()=>{
  const state=createAccountViews(),a=running(state,'A',chatA,'a-run'),b=running(state,'B',chatB,'b-run');
  a.reportTarget='target';a.report={items:[{source:{chat_id:chatA.id,message_id:'1'},state:'Pending',detail:'',sent_id:'sent-1'}]};
  a.uncertainSends=[{profile_id:'A',generation:1,target:'target',old_message_id:'sent-2',state:'Unknown'}];
  a.sendUpdates['target:sent-1']='Pending';
  state.progress({profile_id:'A',generation:1,result:result('a-run')});
  state.progress({profile_id:'B',generation:1,result:result('b-run')});
  state.selectChat('A',chatB);
  assert.equal(a.report?.items[0]?.state,'Pending');assert.equal(a.uncertainSends[0]?.state,'Unknown');
  assert.equal(a.sendUpdates['target:sent-1'],'Pending');
  assert.equal(b.result?.run_id,'b-run');assert.equal(b.selectedChat?.id,chatB.id);
});

async function workspace(transport:(command:string,args:any)=>Promise<unknown>){
  let w!:ReturnType<typeof useWorkspace>;
  await renderToString(createSSRApp({setup(){w=useWorkspace(false,transport as Parameters<typeof useWorkspace>[1]);return()=>h('div');}}));
  w.profileId.value='profile';return w;
}

test('switching during start_query clears synchronously, releases native run and ignores a late rejection',async()=>{
  const calls:Array<{command:string;args:any}>=[];
  let rejectStart!:(error:Error)=>void;
  const w=await workspace(async(command,args)=>{
    calls.push({command,args});
    if(command==='start_query'&&calls.filter(c=>c.command==='start_query').length===1)
      return new Promise((_resolve,reject)=>{rejectStart=reject;});
  });
  w.selectChat(chatA);
  const pending=w.runQuery(),runId=w.currentRun.value;
  assert.equal(w.scanChatId.value,chatA.id);assert.equal(w.scanning.value,true);
  w.selectChat(chatB);
  assert.equal(w.result.value,null);assert.equal(w.activeResult.value,null);
  assert.equal(w.currentRun.value,'');assert.equal(w.scanning.value,false);
  assert.deepEqual(calls.find(c=>c.command==='release_query')?.args,{profileId:'profile',runId});
  rejectStart(new Error('old A failure'));await pending;
  assert.equal(w.error.value,'');assert.equal(w.result.value,null);
  await w.openPost(post);
  assert.equal(calls.some(c=>c.command==='message_link'),false);
  await w.runQuery();
  assert.equal(w.scanChatId.value,chatB.id);assert.equal(w.activeResult.value?.run_id,w.currentRun.value);
});

test('G: late preview cannot restore a switched chat confirmation or submit old Posts',async()=>{
  const calls:Array<{command:string;args:any}>=[];
  let resolvePreview!:(report:{items:[]})=>void;
  const w=await workspace(async(command,args)=>{
    calls.push({command,args});
    if(command==='preview_forward')return new Promise(resolve=>{resolvePreview=resolve;});
  });
  w.selectChat(chatA);w.currentRun.value='run-1';w.scanChatId.value=chatA.id;
  w.result.value=result('run-1');w.selected.value=['post-a'];w.target.value='target';
  const pending=w.prepareForward();
  assert.equal(calls.filter(c=>c.command==='preview_forward').length,1);
  w.selectChat(chatB);
  assert.equal(w.confirmation.value,false);assert.equal(w.preview.value,null);
  resolvePreview({items:[]});await pending;
  assert.equal(w.confirmation.value,false);assert.equal(w.preview.value,null);
  await w.forward();
  assert.equal(calls.some(c=>c.command==='forward_selected'),false);
  assert.equal(w.error.value,'');
});

test('G: an incomplete-Post dialog cannot preview A after switching to B',async()=>{
  const calls:string[]=[];
  const w=await workspace(async command=>{calls.push(command);});
  w.selectChat(chatA);w.currentRun.value='run-1';w.scanChatId.value=chatA.id;
  w.result.value={...result('run-1'),posts:[{...post,completeness:{level:'Unknown',reason:'synthetic boundary'}}]};
  w.selected.value=['post-a'];w.target.value='target';
  const pending=w.prepareForward();
  assert.match(w.dialog.value?.title??'',/可能不完整/);
  w.selectChat(chatB);w.answerDialog({text:'',deleteData:false});await pending;
  assert.equal(calls.includes('preview_forward'),false);
  assert.equal(w.confirmation.value,false);
});

test('G: switching chats does not roll back an already submitted forward or discard its receipt',async()=>{
  const calls:string[]=[];
  let resolveForward!:(report:{items:Array<{source:{chat_id:string;message_id:string};state:string;detail:string;sent_id:string}>})=>void;
  const w=await workspace(async command=>{
    calls.push(command);
    if(command==='preview_forward')return {items:[]};
    if(command==='forward_selected')return new Promise(resolve=>{resolveForward=resolve;});
  });
  w.selectChat(chatA);w.currentRun.value='run-1';w.scanChatId.value=chatA.id;
  w.result.value=result('run-1');w.selected.value=['post-a'];w.target.value='target';
  await w.prepareForward();assert.equal(w.confirmation.value,true);
  const pending=w.forward();assert.equal(calls.filter(command=>command==='forward_selected').length,1);
  w.selectChat(chatB);assert.equal(w.confirmation.value,false);
  resolveForward({items:[{source:post.representative,state:'Pending',detail:'',sent_id:'sent-1'}]});
  await pending;
  assert.equal(w.report.value?.items[0]?.state,'Pending');
  assert.equal(w.result.value,null);
});
