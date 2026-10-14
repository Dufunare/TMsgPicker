<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
const props=defineProps<{left:number;right:number}>();
const emit=defineEmits<{resize:[left:number,right:number];reset:[]}>();
const host=ref<HTMLElement>(); const width=ref(1400); const queryOpen=ref(false); const dragging=ref(false), pressed=ref(false), activeSplitter=ref<'left'|'right'|null>(null);
let observer:ResizeObserver; let side:'left'|'right'='left';
const narrow=computed(()=>width.value<1060);
const dimensions=computed(()=>{
  const available=width.value-12;let left=Math.max(240,available*props.left),right=Math.max(300,available*props.right);
  if(left+right>available-400){const excess=left+right-(available-400),room=(left-240)+(right-300);if(room>0){left-=excess*(left-240)/room;right-=excess*(right-300)/room;}}
  return {left,right,available};
});
const columns=computed(()=>narrow.value?'240px 6px minmax(400px,1fr)':`${dimensions.value.left}px 6px minmax(400px,1fr) 6px ${dimensions.value.right}px`);
function adjust(which:'left'|'right',delta:number){
  const {left,right,available}=dimensions.value;if(narrow.value||available<940)return;
  if(which==='left')emit('resize',Math.max(240,Math.min(available-right-400,left+delta))/available,right/available);
  else emit('resize',left/available,Math.max(300,Math.min(available-left-400,right+delta))/available);
}
let last=0;
function down(e:PointerEvent,which:'left'|'right'){if(narrow.value||e.button!==0)return;side=which;last=e.clientX;pressed.value=true;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);e.preventDefault();}
function move(e:PointerEvent){if(!pressed.value||e.clientX===last)return;dragging.value=true;activeSplitter.value=side;adjust(side,(e.clientX-last)*(side==='left'?1:-1));last=e.clientX;}
function end(){pressed.value=false;activeSplitter.value=null;if(dragging.value){dragging.value=false;}}
function key(e:KeyboardEvent,which:'left'|'right'){if(e.key==='ArrowLeft'||e.key==='ArrowRight'){e.preventDefault();adjust(which,(e.key==='ArrowRight'?20:-20)*(which==='left'?1:-1));}else if(e.key==='Home'){emit('reset');}}
onMounted(()=>{observer=new ResizeObserver(entries=>{width.value=entries[0]!.contentRect.width;});observer.observe(host.value!);});onUnmounted(()=>observer?.disconnect());
</script>
<template>
<div ref="host" class="workspace" :class="{dragging,narrow}" :style="{gridTemplateColumns:columns}">
  <aside class="left-column"><slot name="left"/></aside>
  <div class="splitter" :class="{'is-dragging':activeSplitter==='left'}" :role="narrow?'presentation':'separator'" :aria-disabled="narrow" aria-label="调整账号栏宽度" aria-orientation="vertical" :aria-valuenow="Math.round(dimensions.left/dimensions.available*100)" :aria-valuemin="Math.round(240/dimensions.available*100)" :aria-valuemax="Math.round((dimensions.available-dimensions.right-400)/dimensions.available*100)" :tabindex="narrow ? -1 : 0" @pointerdown="down($event,'left')" @pointermove="move" @pointerup="end" @pointercancel="end" @lostpointercapture="end" @keydown="key($event,'left')"/>
  <section class="middle-column"><button v-if="narrow" class="query-toggle" @click="queryOpen=!queryOpen">{{queryOpen?'关闭查询面板':'打开 Query Builder'}}</button><slot name="middle"/></section>
  <div v-if="!narrow" class="splitter" :class="{'is-dragging':activeSplitter==='right'}" role="separator" aria-label="调整查询栏宽度" aria-orientation="vertical" :aria-valuenow="Math.round(dimensions.right/dimensions.available*100)" :aria-valuemin="Math.round(300/dimensions.available*100)" :aria-valuemax="Math.round((dimensions.available-dimensions.left-400)/dimensions.available*100)" tabindex="0" @pointerdown="down($event,'right')" @pointermove="move" @pointerup="end" @pointercancel="end" @lostpointercapture="end" @keydown="key($event,'right')"/>
  <aside v-show="!narrow||queryOpen" class="right-column" :class="{drawer:narrow}"><button v-if="narrow" @click="queryOpen=false">关闭查询面板</button><slot name="right"/></aside>
</div>
</template>
