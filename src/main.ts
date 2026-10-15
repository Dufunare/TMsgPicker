import { createApp } from 'vue';
import App from './App.vue';
import SettingsPage from './SettingsPage.vue';
import LoginPage from './LoginPage.vue';
import { initializeAppearance } from './theme/appearance';
import './theme/tokens.css';
import './theme/components.css';
import './theme/appearance-settings.css';
import './workspace.css';
import './theme/settings.css';
import './theme/glass.css';

void initializeAppearance();
const view = new URLSearchParams(window.location.search).get('view');
if (view) document.documentElement.classList.add('settings-view');
createApp(view === 'login' ? LoginPage : view === 'settings' || view === 'account' ? SettingsPage : App).mount('#app');
