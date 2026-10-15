<script setup lang="ts">
import { ref, watch } from 'vue';
import { useAppearance, type Appearance } from '../theme/appearance';
const {appearance,previewAppearance,persistAppearance,defaultAppearance,appearanceReady,appearanceLoadError} = useAppearance();
const draft=ref<Appearance>({...appearance});
const saving=ref(false);
const error=ref('');
const saved=ref(false);
let previewRevision=0;
watch(appearance, value => { if(!saving.value) draft.value={...value}; }, { deep:true });
async function preview(){
  const revision=++previewRevision;saved.value=false;error.value='';
  try{await previewAppearance({...draft.value});}
  catch(e){if(revision===previewRevision)error.value=String(e);}
}
async function save(){
  previewRevision++;saving.value=true;error.value='';
  try { await persistAppearance({...draft.value});saved.value=true; }
  catch(e){error.value=String(e);}
  finally{saving.value=false;}
}
function defaults(){draft.value={...defaultAppearance};preview();}
</script>
<template>
<section class="settings-content appearance-settings">
  <fieldset class="appearance-controls" :disabled="saving||!appearanceReady">
  <section class="settings-card" aria-labelledby="appearance-theme-title">
  <h3 id="appearance-theme-title">主题与界面</h3>
  <div class="appearance-presets" role="group" aria-label="主题">
    <button v-for="option in [{id:'system',name:'跟随系统',icon:'◐'},{id:'light',name:'浅色',icon:'☀'},{id:'dark',name:'深色',icon:'☾'},{id:'glass',name:'液态玻璃',icon:'◈'}]" :key="option.id" type="button" class="theme-tile" :data-theme-option="option.id" :class="{'is-selected':draft.theme===option.id}" :aria-pressed="draft.theme===option.id" @click="draft.theme=option.id as Appearance['theme'];preview()"><span class="theme-glyph" aria-hidden="true">{{option.icon}}</span><span>{{option.name}}</span></button>
  </div>
  <div class="appearance-inline">
    <label>强调色
      <div class="color-input">
        <input v-model="draft.accent" type="color" aria-label="强调颜色" @input="preview">
        <input v-model="draft.accent" maxlength="7" pattern="#[a-fA-F0-9]{6}" aria-label="强调色 HEX" @input="preview">
      </div>
    </label>
    <label>界面密度
      <select v-model="draft.density" @change="preview"><option value="comfortable">舒适</option><option value="compact">紧凑</option></select>
    </label>
  </div>
  </section>
  <details class="settings-card appearance-advanced">
    <summary>高级外观设置</summary>
    <div class="appearance-inline">
      <label><span class="setting-label">圆角 <span class="setting-value">{{draft.radius}}px</span></span><input v-model.number="draft.radius" type="range" min="6" max="22" step="1" @input="preview"></label>
      <label><span class="setting-label">字号 <span class="setting-value">{{draft.font_scale}}%</span></span><input v-model.number="draft.font_scale" type="range" min="85" max="120" step="5" @input="preview"></label>
      <label>阴影层次<select v-model="draft.elevation" @change="preview"><option value="flat">平面</option><option value="soft">柔和</option><option value="raised">浮起</option></select></label>
      <label>滚动条<select v-model="draft.scrollbar" @change="preview"><option value="slim">纤细</option><option value="standard">标准</option></select></label>
    </div>
  </details>
  <div class="buttons appearance-actions">
    <button class="primary" type="button" :disabled="saving" @click="save">{{saving?'应用中…':'应用主题'}}</button>
    <button type="button" :disabled="saving" @click="defaults">恢复默认</button>
    <span v-if="saved" role="status" class="saved-indicator">已应用</span>
  </div>
  </fieldset>
  <p v-if="error||appearanceLoadError" role="alert" class="error">{{error||appearanceLoadError}}</p>
</section>
</template>
