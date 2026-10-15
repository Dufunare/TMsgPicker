import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const read=(name:string)=>readFileSync(new URL('../'+name,import.meta.url),'utf8');

test('all application windows use frameless chrome and explicit window capabilities',()=>{
  const config=JSON.parse(read('src-tauri/tauri.conf.json'));
  const capability=JSON.parse(read('src-tauri/capabilities/default.json'));
  assert.equal(config.app.windows[0].decorations,false);
  for(const label of ['main','settings','login'])assert.ok(capability.windows.includes(label));
  for(const permission of ['close','minimize','toggle-maximize','start-dragging','is-maximized'])
    assert.ok(capability.permissions.includes('core:window:allow-'+permission));
  assert.match(read('src-tauri/src/app.rs'),/\.decorations\(false\)/);
});

test('glass gradient is one window-scoped surface, not repeated in body and content panes',()=>{
  const glass=read('src/theme/glass.css');
  assert.match(glass,/\.window-shell \{ background:var\(--glass-canvas\)/);
  assert.match(glass,/\[data-motion="on"\] \.window-shell/);
  assert.doesNotMatch(glass,/:where\(body,\.workspace,\.settings-page\)/);
  assert.match(read('src/components/WindowTitlebar.vue'),/data-tauri-drag-region/);
  assert.match(read('src/window/windowControls.ts'),/window\.close\(\)/);
});
