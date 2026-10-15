<script setup lang="ts">
import { ref, watch } from 'vue';
import { useAppearance, type Appearance } from '../theme/appearance';
import AppSelect from './AppSelect.vue';
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
      <AppSelect v-model="draft.density" label="界面密度" :options="[{value:'comfortable',label:'舒适'},{value:'compact',label:'紧凑'}]" @change="preview"/>
    </label>
  </div>
  </section>
  <details class="settings-card appearance-advanced">
    <summary>高级外观设置</summary>
    <div class="appearance-inline">
      <label><span class="setting-label">圆角 <span class="setting-value">{{draft.radius}}px</span></span><input v-model.number="draft.radius" type="range" min="6" max="22" step="1" @input="preview"></label>
      <label><span class="setting-label">字号 <span class="setting-value">{{draft.font_scale}}%</span></span><input v-model.number="draft.font_scale" type="range" min="85" max="120" step="5" @input="preview"></label>
      <label>阴影层次<AppSelect v-model="draft.elevation" label="阴影层次" :options="[{value:'flat',label:'平面'},{value:'soft',label:'柔和'},{value:'raised',label:'浮起'}]" @change="preview"/></label>
      <label>滚动条<AppSelect v-model="draft.scrollbar" label="滚动条" :options="[{value:'slim',label:'纤细'},{value:'standard',label:'标准'}]" @change="preview"/></label>
      <label class="motion-toggle"><input v-model="draft.animations" type="checkbox" @change="preview">启用界面动画</label>
      <label><span class="setting-label">图片预览大小 <span class="setting-value">{{draft.image_preview_size ?? 250}}px</span></span><input v-model.number="draft.image_preview_size" type="range" min="120" max="520" step="20" @input="preview"></label>
      <label><span class="setting-label">布局间距 <span class="setting-value">{{draft.spacing ?? 100}}%</span></span><input v-model.number="draft.spacing" type="range" min="75" max="140" step="5" @input="preview"></label>
      <label><span class="setting-label">玻璃不透明度 <span class="setting-value">{{draft.glass_opacity ?? 48}}%</span></span><input v-model.number="draft.glass_opacity" type="range" min="20" max="90" step="5" :disabled="draft.theme!=='glass'" @input="preview"></label>
      <label><span class="setting-label">玻璃模糊 <span class="setting-value">{{draft.glass_blur ?? 30}}px</span></span><input v-model.number="draft.glass_blur" type="range" min="0" max="60" step="5" :disabled="draft.theme!=='glass'" @input="preview"></label>
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
