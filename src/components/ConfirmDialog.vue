<script setup lang="ts">
import {nextTick,ref,watch} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
const {dialog,answerDialog}=useWorkspaceContext();
const element=ref<HTMLDialogElement>(), text=ref(''), deleteData=ref(false);
watch(dialog,async value=>{await nextTick();if(value){text.value=value.input??'';deleteData.value=false;element.value?.showModal();}else element.value?.close();});
</script>
<template><dialog ref="element" class="confirmation-dialog" @cancel.prevent="answerDialog(null)"><form v-if="dialog" @submit.prevent="answerDialog({text,deleteData})"><h2>{{dialog.title}}</h2><p>{{dialog.text}}</p><label v-if="dialog.input!==undefined">确认短语<input v-model="text" maxlength="200" required></label><div class="buttons"><button type="button" autofocus @click="answerDialog(null)">取消</button><button class="primary" :disabled="!!dialog.phrase&&text!==dialog.phrase">确认</button></div></form></dialog></template>

