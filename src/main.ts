import { createApp } from 'vue';
import App from './App.vue';
import SettingsPage from './SettingsPage.vue';
import LoginPage from './LoginPage.vue';
import WindowShell from './components/WindowShell.vue';
import { initializeAppearance } from './theme/appearance';
import './theme/tokens.css';
import './theme/components.css';
import './theme/appearance-settings.css';
import './workspace.css';
import './theme/settings.css';
import './theme/selection-motion.css';
import './theme/glass.css';
import './theme/window-chrome.css';

void initializeAppearance();
const view = new URLSearchParams(window.location.search).get('view');
// The native application currently has a main window and two auxiliary
// window types: login and settings. The abandoned ?view=media prototype
// never had a matching component or a Rust window-opening call.
if (view === 'login' || view === 'settings' || view === 'account') {
  document.documentElement.classList.add('settings-view');
}
const page = view === 'login' ? LoginPage : view === 'settings' || view === 'account' ? SettingsPage : App;
createApp(WindowShell, { page, view: view ?? 'main' }).mount('#app');
