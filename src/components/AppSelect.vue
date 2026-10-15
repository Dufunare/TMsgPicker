<script setup lang="ts">
import {ref,computed,onMounted,onUnmounted,nextTick} from 'vue';
type Option={value:string|number|boolean;label:string;disabled?:boolean};
const props=defineProps<{modelValue:string|number|boolean;options:Option[];disabled?:boolean;label?:string}>();
const emit=defineEmits<{ 'update:modelValue':[value:string|number|boolean];change:[value:string|number|boolean] }>();
const root=ref<HTMLElement>(),menu=ref<HTMLElement>(),open=ref(false),active=ref(0);
const current=computed(()=>props.options.findIndex(item=>item.value===props.modelValue));
const display=computed(()=>props.options[current.value]?.label??'请选择');
function close(){open.value=false;}
function toggle(){if(props.disabled)return;open.value=!open.value;active.value=Math.max(0,current.value);if(open.value)void nextTick(()=>menu.value?.focus());}
function pick(item:Option){if(item.disabled)return;emit('update:modelValue',item.value);emit('change',item.value);close();root.value?.querySelector<HTMLButtonElement>('.app-select-trigger')?.focus();}
function keydown(e:KeyboardEvent){
 if(!open.value){if(['Enter',' ','ArrowDown','ArrowUp'].includes(e.key)){e.preventDefault();toggle();}return;}
 if(e.key==='Escape'||e.key==='Tab'){close();return;}
 if(e.key==='ArrowDown'||e.key==='ArrowUp'||e.key==='Home'||e.key==='End'){
  e.preventDefault();const n=props.options.length;if(!n)return;
  if(e.key==='Home')active.value=0;else if(e.key==='End')active.value=n-1;
  else active.value=(active.value+(e.key==='ArrowDown'?1:-1)+n)%n;
 }
 if(e.key==='Enter'||e.key===' '){e.preventDefault();const selected=props.options[active.value];if(selected)pick(selected);}
}
function outside(e:PointerEvent){if(root.value&&!root.value.contains(e.target as Node))close();}
onMounted(()=>document.addEventListener('pointerdown',outside));onUnmounted(()=>document.removeEventListener('pointerdown',outside));
</script>
<template><div ref="root" class="app-select" @keydown="keydown">
  <button type="button" class="app-select-trigger" :disabled="disabled" :aria-label="label" :aria-expanded="open" aria-haspopup="listbox" @click="toggle">{{display}}<span aria-hidden="true" class="app-select-chevron">⌄</span></button>
  <div v-if="open" ref="menu" role="listbox" class="app-select-menu" tabindex="-1" :aria-label="label">
   <button v-for="(item,i) in options" :key="String(item.value)" type="button" role="option" class="app-select-option" :aria-selected="item.value===modelValue" :class="{active:active===i}" :disabled="item.disabled" @mouseenter="active=i" @click="pick(item)">{{item.label}}</button>
  </div>
</div></template>
