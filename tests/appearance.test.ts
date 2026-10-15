import {test} from 'node:test';
import assert from 'node:assert/strict';
import 'vue'; // Load Vue in its existing SSR environment before installing the synthetic DOM.

// Synthetic DOM only: never load user settings or start a native window.
const root={dataset:{} as Record<string,string>,style:{colorScheme:'',setProperty(name:string,value:string){variables.set(name,value);}}};
const variables=new Map<string,string>();
Object.defineProperty(globalThis,'window',{value:{matchMedia:()=>({matches:true,addEventListener(){}})},configurable:true});
Object.defineProperty(globalThis,'document',{value:{documentElement:root},configurable:true});
const {applyAppearance,appearance,defaultAppearance,isDark,validAppearance}=await import('../src/theme/appearance.ts');

test('liquid glass validates alongside existing themes and uses light controls even on a dark OS',()=>{
  for(const theme of ['system','light','dark','glass'])assert.equal(validAppearance({...defaultAppearance,theme}),true);
  assert.equal(validAppearance({...defaultAppearance,theme:'unknown'}),false);
  assert.equal(validAppearance({...defaultAppearance,theme:'glass',accent:'invalid'}),false);
  assert.equal(isDark({...defaultAppearance,theme:'glass'}),false);
  assert.equal(isDark({...defaultAppearance,theme:'system'}),true);
});

test('glass preview preserves custom preferences and switching themes replaces the glass selector',()=>{
  applyAppearance({...defaultAppearance,theme:'glass',accent:'#345678',radius:18,density:'compact'});
  assert.equal(appearance.theme,'glass');assert.equal(root.dataset.theme,'glass');assert.equal(root.style.colorScheme,'light');
  assert.equal(variables.get('--accent'),'#345678');assert.equal(variables.get('--radius'),'18px');assert.equal(root.dataset.density,'compact');
  applyAppearance({...appearance,theme:'dark'});
  assert.equal(root.dataset.theme,'dark');assert.equal(root.style.colorScheme,'dark');assert.equal(variables.get('--accent'),'#345678');
  applyAppearance({...defaultAppearance,theme:'light'});
  assert.equal(root.dataset.theme,'light');assert.equal(root.style.colorScheme,'light');
});

test('appearance motion can be disabled without losing previous palette preferences',()=>{
  applyAppearance({...defaultAppearance,theme:'glass',animations:false});
  assert.equal(root.dataset.motion,'off');
  assert.equal(root.dataset.theme,'glass');
  applyAppearance({...defaultAppearance,theme:'glass',animations:true});
  assert.equal(root.dataset.motion,'on');
  assert.equal(validAppearance({...defaultAppearance,animations:'no'}),false);
});
