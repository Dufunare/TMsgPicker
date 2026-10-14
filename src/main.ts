import { createApp } from 'vue';
import App from './App.vue';
import SettingsPage from './SettingsPage.vue';
import LoginPage from './LoginPage.vue';
import { initializeAppearance } from './theme/appearance';
import './theme/tokens.css';
import './theme/components.css';
import './workspace.css';

void initializeAppearance();
const view = new URLSearchParams(window.location.search).get('view');
if (view) document.documentElement.classList.add('settings-view');
createApp(view === 'login' ? LoginPage : view === 'settings' || view === 'account' ? SettingsPage : App).mount('#app');
