<script setup lang="ts">
import {computed,ref,onMounted,onUnmounted,nextTick} from 'vue';
const props=defineProps<{modelValue:string;label?:string;disabled?:boolean}>();
const emit=defineEmits<{ 'update:modelValue':[value:string] }>();
const open=ref(false),host=ref<HTMLElement>(),columns=ref<HTMLElement[]>([]);
const now=new Date();
const year=ref(now.getFullYear()),month=ref(now.getMonth()+1),day=ref(now.getDate()),hour=ref(0),minute=ref(0);
const pad=(x:number)=>String(x).padStart(2,'0');
const display=computed(()=>props.modelValue?.replace('T',' ')||'选择日期与时间');
const days=computed(()=>new Date(year.value,month.value,0).getDate());
const configs=computed(()=>[
 {key:'year',name:'年',items:Array.from({length:151},(_,i)=>1950+i)},
 {key:'month',name:'月',items:Array.from({length:12},(_,i)=>i+1)},
 {key:'day',name:'日',items:Array.from({length:days.value},(_,i)=>i+1)},
 {key:'hour',name:'时',items:Array.from({length:24},(_,i)=>i)},
 {key:'minute',name:'分',items:Array.from({length:60},(_,i)=>i)}
]);
const values=computed(()=>[year.value,month.value,day.value,hour.value,minute.value]);
function set(index:number,value:number){if(index===0)year.value=value;else if(index===1)month.value=value;else if(index===2)day.value=value;else if(index===3)hour.value=value;else minute.value=value;
 day.value=Math.min(day.value,days.value);}
function scrollToSelection(){columns.value.forEach((el,i)=>{el?.scrollTo({top:Math.max(0,configs.value[i]!.items.indexOf(values.value[i]!))*36,behavior:'instant'});});}
function toggle(){if(props.disabled)return;if(props.modelValue){const d=new Date(props.modelValue);if(!Number.isNaN(d.valueOf())){year.value=d.getFullYear();month.value=d.getMonth()+1;day.value=d.getDate();hour.value=d.getHours();minute.value=d.getMinutes();}}
 open.value=!open.value;if(open.value)void nextTick(scrollToSelection);}
function save(){emit('update:modelValue',`${year.value}-${pad(month.value)}-${pad(day)}T${pad(hour)}:${pad(minute)}`);open.value=false;}
function wheel(index:number,event:WheelEvent){event.preventDefault();const items=configs.value[index]!.items;const current=values.value[index]!;const next=items[Math.max(0,Math.min(items.length-1,items.indexOf(current)+(event.deltaY>0?1:-1)))];if(next!==undefined)set(index,next);void nextTick(scrollToSelection);}
function outside(e:PointerEvent){if(host.value&&!host.value.contains(e.target as Node))open.value=false;}
function escape(e:KeyboardEvent){if(e.key==='Escape')open.value=false;}
onMounted(()=>{document.addEventListener('pointerdown',outside);document.addEventListener('keydown',escape);});
onUnmounted(()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',escape);});
</script>
<template><div ref="host" class="app-date-time">
 <button type="button" class="app-select-trigger" :aria-label="label" :aria-expanded="open" :disabled="disabled" @click="toggle">{{display}}<span aria-hidden="true">▦</span></button>
 <div v-if="open" class="wheel-date-picker" role="dialog" :aria-label="label??'选择日期时间'">
  <div class="wheel-columns"><div v-for="(config,i) in configs" :key="config.key" class="wheel-column">
   <div class="wheel-label">{{config.name}}</div>
   <div :ref="el=>{if(el)columns[i]=el as HTMLElement}" class="wheel-track" @wheel="wheel(i,$event)">
    <button v-for="item in config.items" :key="item" type="button" :class="{selected:item===values[i]}" @click="set(i,item);nextTick(scrollToSelection)">{{pad(item)}}</button>
   </div>
  </div></div>
  <div class="buttons"><button type="button" @click="open=false">取消</button><button type="button" class="primary" @click="save">确定</button></div>
 </div>
</div></template>