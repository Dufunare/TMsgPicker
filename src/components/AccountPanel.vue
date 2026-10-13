<script setup lang="ts">
import { useWorkspaceContext } from '../useWorkspace';
const {connection,ready,busy,forwarding,accountError,notice,snapshot,profileId,rememberDatabase,passphrase,settingsOpen,authLabel,authValue,connect,disconnect,submitAuth,addAccount,selectAccount,renameAccount,removeAccount,logoutAccount,clearSecret,resetLayout}=useWorkspaceContext();
import AccountSettings from './AccountSettings.vue';
</script>
<template>
<section class="account-panel">
  <div class="brand"><strong>TMsgPicker</strong><button @click="settingsOpen=!settingsOpen" aria-label="账号设置">设置</button></div>
  <label>账号<select v-model="profileId" @change="selectAccount(true)" :disabled="busy||forwarding"><option value="">添加或选择账号</option><option v-for="p in snapshot?.profiles??[]" :key="p.profile_id" :value="p.profile_id">{{p.display_name}}{{p.profile_id===connection.profile_id?' · 当前':''}}</option></select></label><button :disabled="busy||forwarding" @click="addAccount">＋ 添加账号</button>
  <small v-if="connection.profile_id">当前会话：{{snapshot?.profiles.find(p=>p.profile_id===connection.profile_id)?.display_name}}</small><div class="account-status"><span class="status-dot" :class="{online:ready}"/><strong>{{ready?'已登录':connection.auth_state}}</strong><small>{{connection.connection_state}}</small></div>
  <small>{{connection.detail}}</small>
  <p v-if="accountError" class="error" role="alert">{{accountError}}</p><p v-if="notice" class="notice">{{notice}}</p>
  <template v-if="profileId && (connection.profile_id!==profileId || connection.auth_state==='Disconnected' || connection.auth_state==='authorizationStateClosed' || connection.auth_state==='authorizationStateWaitTdlibParameters')">
    <label>此账号数据库口令<input v-model="passphrase" type="password" autocomplete="off" :placeholder="rememberDatabase?'已安全保存；留空自动解锁':'输入原有口令；新账号设置口令'"></label>
    <label class="check"><input v-model="rememberDatabase" type="checkbox" @change="!rememberDatabase && clearSecret(`db:${profileId}`)">记住此设备（Windows 保护）</label>
  </template>
  <div class="buttons"><button class="primary" :disabled="busy||forwarding||!profileId||(connection.profile_id===profileId&&ready)" @click="connect">{{busy?'处理中…':connection.profile_id && connection.profile_id!==profileId?'切换并连接':'连接 / 恢复'}}</button><button :disabled="busy||forwarding||connection.auth_state==='Disconnected'" @click="disconnect">断开</button></div>
  <form v-if="authLabel" @submit.prevent="submitAuth" class="auth-step"><label>{{authLabel}}<input v-model="authValue" :type="connection.auth_state==='authorizationStateWaitPhoneNumber'||connection.auth_state==='authorizationStateWaitEmailAddress'?'text':'password'" required autocomplete="off"></label><button class="primary" :disabled="busy">提交授权</button></form>
  <details class="account-management"><summary>账号管理与布局</summary><div class="buttons"><button :disabled="busy||forwarding" @click="addAccount">添加账号</button><button :disabled="busy||!profileId" @click="renameAccount">重命名</button><button :disabled="busy||!ready" @click="logoutAccount">退出 Telegram 登录</button><button :disabled="busy||!profileId" @click="removeAccount">移除本地账号</button><button @click="resetLayout">重置布局</button></div><small>默认保留 Session 文件；新 Profile 可二次确认删除本地数据，Legacy 保留旧目录。连接所选账号时安全关闭当前会话。</small></details>
  <AccountSettings v-if="settingsOpen"/>
</section>
</template>
