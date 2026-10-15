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
import './theme/glass.css';
import './theme/window-chrome.css';

void initializeAppearance();
const view = new URLSearchParams(window.location.search).get('view');
if (view) document.documentElement.classList.add(view==='media'?'media-view':'settings-view');
const page = view === 'login' ? LoginPage : view === 'settings' || view === 'account' ? SettingsPage : App;
// The media viewer is a special-purpose surface, not a decorated application page.
createApp(view === 'media' ? page : WindowShell, view === 'media' ? {} : { page, view: view ?? 'main' }).mount('#app');
