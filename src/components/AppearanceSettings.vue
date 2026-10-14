<script setup lang="ts">
import { ref, watch } from 'vue';
import { useAppearance, applyAppearance, type Appearance } from '../theme/appearance';
const {appearance,persistAppearance,defaultAppearance} = useAppearance();
const draft=ref<Appearance>({...appearance});
const saving=ref(false);
const error=ref('');
const saved=ref(false);
watch(appearance, value => { if(!saving.value) draft.value={...value}; }, { deep:true });
function preview(){applyAppearance({...draft.value});saved.value=false;}
async function save(){
  saving.value=true;error.value='';
  try { await persistAppearance({...draft.value});saved.value=true; }
  catch(e){error.value=String(e);}
  finally{saving.value=false;}
}
function defaults(){draft.value={...defaultAppearance};preview();}
</script>
<template>
<section class="appearance-settings">
  <div class="settings-section-heading"><h3>外观</h3><span class="setting-caption">全部窗口同步</span></div>
  <div class="appearance-presets" role="group" aria-label="主题">
    <button v-for="option in [{id:'system',name:'跟随系统',icon:'◐'},{id:'light',name:'浅色',icon:'☀'},{id:'dark',name:'深色',icon:'☾'}]" :key="option.id" type="button" class="theme-tile" :class="{'is-selected':draft.theme===option.id}" :aria-pressed="draft.theme===option.id" @click="draft.theme=option.id as Appearance['theme'];preview()"><span class="theme-glyph" aria-hidden="true">{{option.icon}}</span><span>{{option.name}}</span></button>
  </div>
  <div class="appearance-inline">
    <label>强调色
      <div class="color-input">
        <input v-model="draft.accent" type="color" aria-label="强调颜色" @input="preview">
        <input v-model="draft.accent" maxlength="7" pattern="#[a-fA-F0-9]{6}" aria-label="强调色 HEX" @change="preview">
      </div>
    </label>
    <label>界面密度
      <select v-model="draft.density" @change="preview"><option value="comfortable">舒适</option><option value="compact">紧凑</option></select>
    </label>
  </div>
  <details class="appearance-advanced">
    <summary>高级外观设置</summary>
    <div class="appearance-inline">
      <label>圆角 <span class="setting-value">{{draft.radius}}px</span><input v-model.number="draft.radius" type="range" min="6" max="22" step="1" @input="preview"></label>
      <label>字号 <span class="setting-value">{{draft.font_scale}}%</span><input v-model.number="draft.font_scale" type="range" min="85" max="120" step="5" @input="preview"></label>
      <label>阴影层次<select v-model="draft.elevation" @change="preview"><option value="flat">平面</option><option value="soft">柔和</option><option value="raised">浮起</option></select></label>
      <label>滚动条<select v-model="draft.scrollbar" @change="preview"><option value="slim">纤细</option><option value="standard">标准</option></select></label>
    </div>
  </details>
  <div class="buttons appearance-actions">
    <button class="primary" type="button" :disabled="saving" @click="save">{{saving?'保存中…':'保存外观'}}</button>
    <button type="button" :disabled="saving" @click="defaults">恢复默认</button>
    <span v-if="saved" role="status" class="saved-indicator">已保存</span>
  </div>
  <p v-if="error" role="alert" class="error">{{error}}</p>
</section>
</template>
