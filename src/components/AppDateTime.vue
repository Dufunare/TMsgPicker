<script setup lang="ts">
import {computed,ref,watch,nextTick,onMounted} from 'vue';
const props=defineProps<{modelValue:string;label?:string;disabled?:boolean}>();
const emit=defineEmits<{ 'update:modelValue':[value:string] }>();
const now=new Date();
const y=ref(now.getFullYear()),mo=ref(now.getMonth()+1),d=ref(now.getDate()),h=ref(0),mi=ref(0);
const refs=ref<HTMLElement[]>([]);
const pad=(v:number)=>String(v).padStart(2,'0');
const days=computed(()=>new Date(y.value,mo.value,0).getDate());
const selections=computed(()=>[y.value,mo.value,d.value,h.value,mi.value]);
const columns=computed(()=>[
 {name:'年',items:Array.from({length:151},(_,i)=>1950+i)},
 {name:'月',items:Array.from({length:12},(_,i)=>i+1)},
 {name:'日',items:Array.from({length:days.value},(_,i)=>i+1)},
 {name:'时',items:Array.from({length:24},(_,i)=>i)},
 {name:'分',items:Array.from({length:60},(_,i)=>i)}
]);
let applying=false;
function sync(){if(applying)return;applying=true;
 const match=props.modelValue.match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/);
 if(match){y.value=+match[1]!;mo.value=+match[2]!;d.value=+match[3]!;h.value=+match[4]!;mi.value=+match[5]!;}
 applying=false;void nextTick(align);
}
function align(){refs.value.forEach((el,i)=>{if(el)el.scrollTop=Math.max(0,columns.value[i]!.items.indexOf(selections.value[i]!))*32;});}
function choose(i:number,val:number){
 if(props.disabled)return;
 if(i===0)y.value=val;else if(i===1)mo.value=val;else if(i===2)d.value=val;else if(i===3)h.value=val;else mi.value=val;
 d.value=Math.min(d.value,days.value);
 emit('update:modelValue',`${y.value}-${pad(mo.value)}-${pad(d.value)}T${pad(h.value)}:${pad(mi.value)}`);
 void nextTick(align);
}
function wheel(i:number,e:WheelEvent){e.preventDefault();const a=columns.value[i]!.items,v=selections.value[i]!,n=a.indexOf(v);choose(i,a[Math.min(a.length-1,Math.max(0,n+(e.deltaY>0?1:-1)))]!);}
watch(()=>props.modelValue,sync,{immediate:true});onMounted(align);
</script>
<template><div class="inline-date-wheel" :aria-label="label">
 <div v-for="(column,i) in columns" :key="column.name" class="inline-date-col">
  <div class="inline-date-label">{{column.name}}</div>
  <div :ref="el=>{if(el)refs[i]=el as HTMLElement}" class="inline-date-track" @wheel="wheel(i,$event)">
   <button v-for="item in column.items" :key="item" type="button" :class="{active:selections[i]===item}" :disabled="disabled" :aria-pressed="selections[i]===item" @click="choose(i,item)">{{i===0?item:pad(item)}}</button>
  </div>
 </div>
</div></template>
