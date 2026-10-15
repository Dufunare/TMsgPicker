import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=(path:string)=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('chat and settings navigation share an indexed sliding selection treatment',()=>{
  const settings=read('src/SettingsPage.vue');
  const chat=read('src/components/ChatPanel.vue');
  const css=read('src/theme/selection-motion.css');
  assert.match(settings,/class="settings-tabs segmented-tabs"/);
  assert.match(chat,/class="chat-scope segmented-tabs"/);
  assert.match(settings,/--segment-index':tab==='application'\?0:tab==='account'\?1:2/);
  assert.match(chat,/--segment-index':chatScope==='all'\?0:chatScope==='main'\?1:2/);
  for(const tab of ['application','account','appearance'])assert.match(settings,new RegExp("tab==='"+tab+"'"));
  assert.match(css,/:root\[data-motion="on"\] \.segmented-tabs::before/);
  assert.match(css,/transition:transform \.28s cubic-bezier/);
  assert.doesNotMatch(css,/:root\[data-motion="off"\] \.segmented-tabs::before/);
  assert.match(read('src/main.ts'),/selection-motion\.css/);
});

test('settings panels retain form state while switching and obey animation preferences',()=>{
  const settings=read('src/SettingsPage.vue');
  const css=read('src/theme/selection-motion.css');
  const layout=read('src/theme/settings.css');
  for(const component of ['ApplicationSettings','AccountSettings','AppearanceSettings'])
    assert.match(settings,new RegExp('<Transition name="selection-fade"><'+component+' v-show='));
  assert.doesNotMatch(settings,/v-if="tab===/);
  assert.match(layout,/\.settings-panels > \.settings-content \{ position:absolute/);
  assert.match(css,/:root\[data-motion="off"\] \.selection-fade-enter-active/);
  assert.match(css,/@media\(prefers-reduced-motion:reduce\)/);
});
