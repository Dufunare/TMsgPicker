<script setup lang="ts">
import {computed,ref,onMounted,onUnmounted} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
import {accountStatus} from '../accountViews';
import UiIcon from './UiIcon.vue';
import AvatarPreview from './AvatarPreview.vue';
const {connection,statuses,busy,accountError,notice,snapshot,profileId,openSettings,openAccountSettings,connectFromCard,disconnect,addAccount,selectAccount}=useWorkspaceContext();
const menu=ref<HTMLDetailsElement>();
const profile=computed(()=>snapshot.value?.profiles.find(p=>p.profile_id===profileId.value));
const status=computed(()=>accountStatus(connection.value));
function outside(event:PointerEvent){if(menu.value&&!menu.value.contains(event.target as Node))closeMenu();}
onMounted(()=>document.addEventListener('pointerdown',outside));onUnmounted(()=>document.removeEventListener('pointerdown',outside));
function closeMenu(){if(menu.value)menu.value.open=false;}
async function run(operation:()=>Promise<void>){closeMenu();await operation();}
</script>
<template>
<section class="account-panel">
  <div class="account-strip">
    <div v-if="!profile" class="account-empty"><p>尚未添加 Telegram 账号</p><button class="primary" @click="addAccount">添加账号</button></div>
    <div v-else class="user-card">
      <AvatarPreview :profile-id="profileId" :generation="connection.generation" :name="profile.display_name"/>
      <div class="user-identity"><strong :title="profile.display_name">{{profile.display_name}}</strong><div class="user-meta"><span v-if="profile.username" class="username">@{{profile.username}}</span><span class="status-dot" :class="{online:status==='已连接',connecting:status==='连接中'}" :title="status"/><span class="user-status">{{status}}</span></div></div>
      <div class="account-tools">
        <button class="icon-button" title="添加账号" aria-label="添加账号" @click="addAccount"><UiIcon name="plus"/></button>
        <div class="account-switch"><UiIcon name="chevron"/><select v-model="profileId" aria-label="切换账号" title="切换账号" @change="selectAccount(true)"><option v-for="p in snapshot?.profiles??[]" :key="p.profile_id" :value="p.profile_id">{{p.display_name}} · {{accountStatus(statuses[p.profile_id])}}</option></select></div>
        <details ref="menu" class="account-menu" @keydown.esc="closeMenu"><summary class="icon-button" aria-label="更多账号操作" title="更多账号操作"><UiIcon name="more"/></summary><div class="account-menu-items"><button v-if="connection.user_disconnected" :disabled="busy" @click="run(connectFromCard)">恢复连接</button><button v-else :disabled="busy||connection.runtime_state==='Closed'||connection.runtime_state==='Closing'||connection.runtime_state==='LogoutCleanupRequired'" @click="run(disconnect)">断开连接</button><button @click="run(openAccountSettings)">账号设置</button></div></details>
      </div>
    </div>
    <button class="icon-button settings-gear" title="应用设置" aria-label="打开应用设置" @click="openSettings"><UiIcon name="settings"/></button>
  </div>
  <div v-if="profile&&(accountError||notice||connection.runtime_state==='NeedsDatabasePassphrase'||connection.runtime_state==='Error')" class="account-feedback"><button v-if="connection.runtime_state==='NeedsDatabasePassphrase'" class="brief-status" @click="openAccountSettings">请输入本地数据库密码以恢复 Session</button><button v-else-if="accountError||connection.runtime_state==='Error'" class="brief-error" :title="accountError||connection.detail" @click="openAccountSettings">操作未完成 · 查看详情</button><span v-else :title="notice">{{notice}}</span></div>
  <p v-if="!profile&&accountError" class="error">{{accountError}}</p>
</section>
</template>
