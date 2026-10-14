import { createApp } from 'vue';
import App from './App.vue';
import SettingsPage from './SettingsPage.vue';
import './workspace.css';
const settingsView=new URLSearchParams(window.location.search).get('view')==='settings';
if(settingsView)document.documentElement.classList.add('settings-view');
createApp(settingsView?SettingsPage:App).mount('#app');
