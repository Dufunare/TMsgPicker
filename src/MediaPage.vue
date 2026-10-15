<script setup lang="ts">
import {computed,ref,onMounted,onUnmounted,watch,nextTick} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {requestMedia} from './media/mediaRequests.ts';
import {listen} from '@tauri-apps/api/event';
type Gallery={profile_id:string;generation:number;image_file_ids:number[];index:number};
const gallery=ref<Gallery|null>(null),index=ref(0),image=ref(''),loading=ref(false),error=ref('');
const scale=ref(1),x=ref(0),y=ref(0),baseW=ref(0),baseH=ref(0);
const pane=ref<HTMLElement>(),drag=ref<{x:number;y:number;ox:number;oy:number;move:boolean}|null>(null);
let revision=0,unlisten:(()=>void)|undefined;
const count=computed(()=>gallery.value?.image_file_ids.length??0);
const maxX=computed(()=>Math.max(0,(baseW.value*scale.value-(pane.value?.clientWidth??1))/2));
const maxY=computed(()=>Math.max(0,(baseH.value*scale.value-(pane.value?.clientHeight??1))/2));
function clamp(){x.value=Math.min(maxX.value,Math.max(-maxX.value,x.value));y.value=Math.min(maxY.value,Math.max(-maxY.value,y.value));}
function reset(){scale.value=1;x.value=0;y.value=0;}
async function fetchImage(){const g=gallery.value;if(!g)return;const token=++revision;
 reset();image.value='';error.value='';loading.value=true;
 try{const fileId=g.image_file_ids[index.value];if(!fileId)return;
 const response=await requestMedia(g.profile_id,g.generation,fileId,true);
 if(token!==revision)return;
 if(response?.startsWith('data:image/'))image.value=response;
 else error.value='无法载入原图（文件过大或暂未下载完成）';
 }catch(e){if(token===revision)error.value=String(e);}
 finally{if(token===revision)loading.value=false;}}
function resize(){const img=new Image();img.onload=()=>{const w=pane.value?.clientWidth??900,h=pane.value?.clientHeight??650;const fit=Math.min(w/img.width,h/img.height,1);baseW.value=img.width*fit;baseH.value=img.height*fit;reset();};img.src=image.value;}
watch(image,v=>{if(v)void nextTick(resize);});
function setGallery(g:Gallery){gallery.value=g;index.value=g.index;void fetchImage();}
function step(delta:number){if(count.value<2)return;index.value=(index.value+delta+count.value)%count.value;void fetchImage();}
async function close(){await invoke('close_image_gallery');}
function scroll(e:WheelEvent){if(!e.ctrlKey)return;e.preventDefault();scale.value=Math.max(.25,Math.min(12,scale.value*(e.deltaY<0?1.12:1/1.12)));clamp();}
function down(e:PointerEvent){if(e.button!==0)return;drag.value={x:e.clientX,y:e.clientY,ox:x.value,oy:y.value,move:false};(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);}
function move(e:PointerEvent){if(!drag.value)return;const dx=e.clientX-drag.value.x,dy=e.clientY-drag.value.y;if(Math.hypot(dx,dy)>4)drag.value.move=true;
 x.value=drag.value.ox+dx;y.value=drag.value.oy+dy;clamp();}
function up(e:PointerEvent){if(!drag.value)return;const moved=drag.value.move;drag.value=null;(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);if(!moved)void close();}
function keys(e:KeyboardEvent){if(e.key==='Escape')void close();if(e.key==='ArrowLeft')step(-1);if(e.key==='ArrowRight')step(1);}
onMounted(async()=>{gallery.value=await invoke<Gallery>('image_gallery_state');index.value=gallery.value.index;void fetchImage();
 unlisten=await listen<Gallery>('gallery-updated',e=>setGallery(e.payload));window.addEventListener('keydown',keys);window.addEventListener('resize',clamp);});
onUnmounted(()=>{++revision;unlisten?.();window.removeEventListener('keydown',keys);window.removeEventListener('resize',clamp);});
</script>
<template><main class="native-image-viewer">
 <div ref="pane" class="native-image-pane" @wheel="scroll" @pointerdown="down" @pointermove="move" @pointerup="up" @pointercancel="drag=null">
  <img v-if="image" :src="image" alt="原图" draggable="false" :style="{width:baseW+'px',height:baseH+'px',transform:`translate(${x}px,${y}px) scale(${scale})`}"/>
  <span v-else class="viewer-status">{{loading?'加载原图中…':error}}</span>
 </div>
 <button v-if="count>1" type="button" class="viewer-arrow prev" aria-label="上一张" @pointerdown.stop @click.stop="step(-1)">‹</button>
 <button v-if="count>1" type="button" class="viewer-arrow next" aria-label="下一张" @pointerdown.stop @click.stop="step(1)">›</button>
</main></template>