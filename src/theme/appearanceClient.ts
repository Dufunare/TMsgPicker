import type {Appearance} from './appearance';

export interface AppearanceSnapshot {appearance:Appearance;revision:number;editor:number}
type Transport=<T>(command:string,args?:Record<string,unknown>)=>Promise<T>;

/** Serializes local edits and rejects stale snapshots from other windows or startup. */
export function createAppearanceClient(
  transport:Transport,
  subscribe:(receive:(snapshot:AppearanceSnapshot)=>void)=>Promise<()=>void>,
  apply:(appearance:Appearance)=>void,
) {
  let latest:AppearanceSnapshot|undefined,intent=0,settled=0;
  let queue=Promise.resolve();
  function receive(snapshot:AppearanceSnapshot){
    if(latest&&snapshot.revision<latest.revision)return;
    latest=snapshot;
    if(intent===settled)apply(snapshot.appearance);
  }
  async function start(){
    const stop=await subscribe(receive);
    try{receive(await transport<AppearanceSnapshot>('appearance_state'));}
    catch(error){stop();throw error;}
    return stop;
  }
  function edit(command:'preview_appearance'|'save_appearance',appearance:Appearance){
    if(!latest)return Promise.reject(new Error('外观设置尚未加载完成'));
    const version=++intent,editor=latest.editor,next={...appearance};
    apply(next);
    const task=queue.catch(()=>{}).then(async()=>{
      if(command==='preview_appearance'&&version!==intent)return;
      try{
        const snapshot=await transport<AppearanceSnapshot>(command,{appearance:next,editor});
        if(version===intent)settled=version;
        receive(snapshot);
      }catch(error){
        if(version===intent){settled=version;if(latest)apply(latest.appearance);}
        throw error;
      }
    });
    queue=task;
    return task;
  }
  return {start,preview:(next:Appearance)=>edit('preview_appearance',next),save:(next:Appearance)=>edit('save_appearance',next)};
}
