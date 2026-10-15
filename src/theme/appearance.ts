import { reactive, readonly } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type Appearance = {
  theme: 'system' | 'light' | 'dark' | 'glass';
  accent: string;
  density: 'compact' | 'comfortable';
  radius: number;
  font_scale: number;
  elevation: 'flat' | 'soft' | 'raised';
  scrollbar: 'slim' | 'standard';
};

export const defaultAppearance: Appearance = {
  theme: 'system', accent: '#4277cf', density: 'comfortable',
  radius: 12, font_scale: 100, elevation: 'soft', scrollbar: 'slim',
};

const current = reactive<Appearance>({ ...defaultAppearance });
export const appearance = readonly(current);
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)');

export function isDark(settings: Appearance = current): boolean {
  return settings.theme === 'dark' || (settings.theme === 'system' && prefersDark.matches);
}

function asRgb(hex: string): [number, number, number] {
  return [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];
}
function clamp(x: number) { return Math.max(0, Math.min(255, Math.round(x))); }
function mix(hex: string, base: number[], amount: number): string {
  const rgb = asRgb(hex);
  return `rgb(${rgb.map((x, i) => clamp(x * (1 - amount) + base[i]! * amount)).join(', ')})`;
}
function readableText(rgb: [number, number, number]): string {
  const lum = rgb.map(v => {
    const c = v / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return lum[0]! * 0.2126 + lum[1]! * 0.7152 + lum[2]! * 0.0722 > 0.40 ? '#142033' : '#ffffff';
}

export function validAppearance(input: unknown): input is Appearance {
  if (!input || typeof input !== 'object') return false;
  const a = input as Record<string, unknown>;
  return ['system', 'light', 'dark', 'glass'].includes(String(a.theme))
    && /^#[0-9a-fA-F]{6}$/.test(String(a.accent))
    && ['compact', 'comfortable'].includes(String(a.density))
    && Number.isInteger(a.radius) && (a.radius as number) >= 6 && (a.radius as number) <= 22
    && Number.isInteger(a.font_scale) && (a.font_scale as number) >= 85 && (a.font_scale as number) <= 120
    && ['flat', 'soft', 'raised'].includes(String(a.elevation))
    && ['slim', 'standard'].includes(String(a.scrollbar));
}

export function applyAppearance(next: Appearance) {
  if (!validAppearance(next)) return;
  Object.assign(current, next);
  const root = document.documentElement;
  const dark = isDark(next);
  root.dataset.theme = next.theme === 'glass' ? 'glass' : dark ? 'dark' : 'light';
  root.dataset.density = next.density;
  root.dataset.elevation = next.elevation;
  root.dataset.scrollbar = next.scrollbar;
  root.style.colorScheme = dark ? 'dark' : 'light';
  root.style.setProperty('--accent', next.accent);
  root.style.setProperty('--accent-hover', mix(next.accent, dark ? [255, 255, 255] : [0, 0, 0], dark ? 0.16 : 0.13));
  root.style.setProperty('--accent-soft', mix(next.accent, dark ? [32, 43, 60] : [255, 255, 255], dark ? 0.80 : 0.89));
  root.style.setProperty('--accent-contrast', readableText(asRgb(next.accent)));
  root.style.setProperty('--radius', `${next.radius}px`);
  root.style.setProperty('--font-scale', String(next.font_scale / 100));
  root.style.setProperty('--density-factor', next.density === 'compact' ? '0.84' : '1');
  root.style.setProperty('--scrollbar-width', next.scrollbar === 'slim' ? '7px' : '11px');
  if (isTauri()) document.documentElement.dataset.desktop = 'true';
}

export async function persistAppearance(next: Appearance) {
  if (!validAppearance(next)) throw new Error('Invalid appearance settings');
  const saved = await invoke<Appearance>('save_appearance', { appearance: next });
  applyAppearance(saved);
}

export async function initializeAppearance() {
  applyAppearance(defaultAppearance);
  prefersDark.addEventListener('change', () => { if (current.theme === 'system') applyAppearance({ ...current }); });
  if (!isTauri()) return;
  try {
    await listen<Appearance>('appearance-changed', e => applyAppearance(e.payload));
    const snapshot = await invoke<{settings: {appearance?: Appearance}}>('app_settings');
    if (snapshot.settings.appearance) applyAppearance(snapshot.settings.appearance);
  } catch (error) {
    // App startup must not depend on appearance storage.
    console.warn('Appearance unavailable:', error instanceof Error ? error.message : String(error));
  }
}

export function useAppearance() {
  return { appearance, persistAppearance, defaultAppearance, isDark };
}
