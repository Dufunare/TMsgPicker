// All native window operations live here. Application pages never import Tauri window APIs.
import { getCurrentWindow } from '@tauri-apps/api/window';

export interface WindowControls {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  onResized(handler: () => void): Promise<() => void>;
}
export function createWindowControls(): WindowControls {
  const window = getCurrentWindow();
  return {
    minimize: () => window.minimize(),
    toggleMaximize: () => window.toggleMaximize(),
    // The close request goes through app.rs safeguards (main exit, login cleanup,
    // settings appearance-preview rollback). Never call process.exit here.
    close: () => window.close(),
    isMaximized: () => window.isMaximized(),
    onResized: async handler => window.onResized(() => handler()),
  };
}
