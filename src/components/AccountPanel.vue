<script setup lang="ts">
import {computed,ref,onMounted,onUnmounted} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
import UiIcon from './UiIcon.vue';
const {connection,ready,busy,forwarding,accountError,notice,snapshot,profileId,cardCompact,authLabel,openSettings,connectFromCard,disconnect,addAccount,selectAccount,renameAccount,removeAccount,logoutAccount,toggleCard}=useWorkspaceContext();
const menu=ref<HTMLDetailsElement>();
const profile=computed(()=>snapshot.value?.profiles.find(p=>p.profile_id===profileId.value));
const name=computed(()=>profile.value?.display_name??'未选择账号');
const active=computed(()=>connection.value.profile_id===profileId.value);
const status=computed(()=>!active.value?'未连接':authLabel.value?'需要授权':connection.value.connection_state==='connectionStateReady'&&ready.value?'已连接':ready.value?'连接中':connection.value.auth_state==='Disconnected'||connection.value.auth_state==='authorizationStateClosed'?'未连接':'连接中');
function outside(event:PointerEvent){if(menu.value&&!menu.value.contains(event.target as Node))closeMenu();}
onMounted(()=>document.addEventListener('pointerdown',outside));onUnmounted(()=>document.removeEventListener('pointerdown',outside));
function closeMenu(){if(menu.value)menu.value.open=false;}
async function run(operation:()=>Promise<void>){closeMenu();await operation();}
</script>
<template>
<section class="account-panel" :class="{compact:cardCompact}">
  <div class="account-strip">
    <div class="user-card">
      <span class="user-avatar" aria-hidden="true">{{Array.from(name)[0]?.toUpperCase()??'?'}}</span>
      <div class="user-identity"><strong :title="name">{{name}}</strong><div class="user-meta" :aria-label="status"><span v-if="profile?.username" class="username" :title="`@${profile.username}`">@{{profile.username}}</span><span class="status-dot" :class="{online:status==='已连接',connecting:status==='连接中'}" :title="status"/><span class="user-status">{{status}}</span></div></div>
      <div class="account-tools">
        <button class="icon-button" title="添加账号" aria-label="添加账号" :disabled="busy||forwarding" @click="addAccount"><UiIcon name="plus"/></button>
        <div class="account-switch"><UiIcon name="chevron"/><select v-model="profileId" aria-label="切换账号" title="切换账号" :disabled="busy||forwarding" @change="selectAccount(true)"><option v-if="!profileId" value="">选择账号</option><option v-for="p in snapshot?.profiles??[]" :key="p.profile_id" :value="p.profile_id">{{p.display_name}}</option></select></div>
        <details ref="menu" class="account-menu" @keydown.esc="closeMenu"><summary class="icon-button" aria-label="更多账号操作" title="更多账号操作"><UiIcon name="more"/></summary><div class="account-menu-items"><button :disabled="busy||forwarding||!profileId||(active&&ready)" @click="run(connectFromCard)">连接 / 恢复</button><button :disabled="busy||forwarding||connection.auth_state==='Disconnected'" @click="run(disconnect)">断开当前连接</button><button :disabled="busy||!profileId" @click="run(renameAccount)">重命名所选账号</button><button :disabled="busy||!ready" @click="run(logoutAccount)">退出当前 Telegram 登录…</button><button :disabled="busy||!profileId" @click="run(removeAccount)">移除所选本地账号…</button><button @click="closeMenu();toggleCard()">{{cardCompact?'展开用户卡片':'收起用户卡片'}}</button></div></details>
      </div>
    </div>
    <button class="icon-button settings-gear" title="应用设置" aria-label="打开设置窗口" @click="openSettings"><UiIcon name="settings"/></button>
  </div>
  <div v-if="accountError||notice||authLabel" class="account-feedback"><button v-if="accountError" class="brief-error" :title="accountError" @click="openSettings">操作未完成 · 查看详情</button><button v-else-if="authLabel" class="brief-status" @click="openSettings">完成账号授权</button><span v-else :title="notice">设置已更新</span></div>
  <button class="card-collapse" :aria-label="cardCompact?'展开用户卡片':'收起用户卡片'" :title="cardCompact?'展开':'收起'" @click="toggleCard"><UiIcon :name="cardCompact?'expand':'collapse'"/></button>
</section>
</template>
