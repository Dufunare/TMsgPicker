<script setup lang="ts">
import {computed,ref,onMounted,onUnmounted} from 'vue';
const props=defineProps<{modelValue:string;label?:string;disabled?:boolean}>();
const emit=defineEmits<{ 'update:modelValue':[value:string] }>();
const expanded=ref(false),host=ref<HTMLElement>();
const today=new Date();
const year=ref(today.getFullYear()),month=ref(today.getMonth()),hour=ref(0),minute=ref(0);
const names=['日','一','二','三','四','五','六'];
function pad(x:number){return String(x).padStart(2,'0');}
const display=computed(()=>props.modelValue?props.modelValue.replace('T',' '):'选择日期与时间');
const first=computed(()=>new Date(year.value,month.value,1).getDay());
const count=computed(()=>new Date(year.value,month.value+1,0).getDate());
const cells=computed(()=>(Array.from({length:first.value},()=>0)).concat(Array.from({length:count.value},(_,i)=>i+1)));
function show(){
 if(props.disabled)return;
 if(props.modelValue){
  const d=new Date(props.modelValue);
  if(!Number.isNaN(d.valueOf())){year.value=d.getFullYear();month.value=d.getMonth();hour.value=d.getHours();minute.value=d.getMinutes();}
 }
 expanded.value=!expanded.value;
}
function move(shift:number){const d=new Date(year.value,month.value+shift,1);year.value=d.getFullYear();month.value=d.getMonth();}
function select(day:number){const hh=Math.min(23,Math.max(0,hour.value||0)),mm=Math.min(59,Math.max(0,minute.value||0));emit('update:modelValue',`${year.value}-${pad(month.value+1)}-${pad(day)}T${pad(hh)}:${pad(mm)}`);expanded.value=false;}
function outside(e:PointerEvent){if(host.value&&!host.value.contains(e.target as Node))expanded.value=false;}
function key(e:KeyboardEvent){if(e.key==='Escape'){expanded.value=false;}}
onMounted(()=>{document.addEventListener('pointerdown',outside);document.addEventListener('keydown',key);});
onUnmounted(()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',key);});
</script>
<template>
 <div ref="host" class="app-date-time">
  <button type="button" class="app-select-trigger" :aria-label="label" :aria-expanded="expanded" aria-haspopup="dialog" :disabled="disabled" @click="show">{{display}}<span aria-hidden="true">▦</span></button>
  <div v-if="expanded" class="app-calendar" role="dialog" :aria-label="label??'选择日期与时间'">
    <div class="calendar-heading"><button type="button" aria-label="上个月" @click="move(-1)">‹</button><strong>{{year}}年{{month+1}}月</strong><button type="button" aria-label="下个月" @click="move(1)">›</button></div>
    <div class="calendar-grid">
      <span v-for="n in names" :key="n" class="muted">{{n}}</span>
      <template v-for="(day,i) in cells" :key="i"><span v-if="!day"/><button v-else type="button" :class="{'chosen-day':modelValue.startsWith(year+'-'+pad(month+1)+'-'+pad(day)+'T')}" @click="select(day)">{{day}}</button></template>
    </div>
    <div class="calendar-time"><label>时<input v-model.number="hour" aria-label="小时" type="number" min="0" max="23"></label><label>分<input v-model.number="minute" aria-label="分钟" type="number" min="0" max="59"></label></div>
  </div>
 </div>
</template>
