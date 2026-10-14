<script setup lang="ts">
import {ref,watch} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
const {connection,ready,accountError,notice,apiId,apiHash,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,passphrase,rememberDatabase,authLabel,authValue,busy,forwarding,snapshot,profileId,cardCompact,connect,disconnect,submitAuth,addAccount,selectAccount,renameAccount,removeAccount,logoutAccount,saveSettings,clearSecret,resetSettings,resetLayout,toggleCard}=useWorkspaceContext();
const tab=ref<'account'|'connection'|'security'>('connection');
watch(authLabel,value=>{if(value)tab.value='account';});
watch(profileId,()=>{tab.value='account';});
</script>
<template>
<main class="settings-page">
  <div class="settings-heading"></div>
  <nav class="settings-tabs" aria-label="设置分类"><button :aria-pressed="tab==='account'" @click="tab='account'">账号与授权</button><button :aria-pressed="tab==='connection'" @click="tab='connection'">API 与代理</button><button :aria-pressed="tab==='security'" @click="tab='security'">安全与界面</button></nav>
  <div class="settings-content">
    <p v-if="accountError" class="error" role="alert">{{accountError}}</p><p v-if="notice" class="notice" role="status">{{notice}}</p>
    <section v-show="tab==='account'">
      <label>所选账号<select v-model="profileId" @change="selectAccount(true)"><option value="">添加或选择账号</option><option v-for="p in snapshot?.profiles??[]" :key="p.profile_id" :value="p.profile_id">{{p.display_name}}</option></select></label>
      <div class="buttons"><button :disabled="busy||forwarding" @click="addAccount">添加账号</button><button :disabled="busy||!profileId" @click="renameAccount">重命名</button></div>
      <p class="muted">{{connection.detail}}<br><small>{{connection.connection_state}}</small></p>
      <form v-if="profileId" @submit.prevent="connect">
        <label>所选账号数据库口令<input v-model="passphrase" type="password" autocomplete="off" :placeholder="rememberDatabase?'已安全保存；留空使用保存值':'输入原有口令；新账号设置口令'"></label>
        <label class="check"><input v-model="rememberDatabase" type="checkbox" @change="!rememberDatabase&&clearSecret(`db:${profileId}`)">记住此设备（Windows 保护）</label><small>勾选后在连接时保存，取消立即清除保存口令。不会改变原数据库密钥。</small>
        <div class="buttons"><button class="primary" :disabled="busy||forwarding||ready||connection.runtime_state==='AuthorizationRequired'||connection.runtime_state==='Initializing'||connection.runtime_state==='Closing'">{{busy?'处理中…':'连接 / 恢复所选账号'}}</button><button type="button" :disabled="busy||forwarding||connection.auth_state==='Disconnected'" @click="disconnect">断开当前连接</button></div>
      </form>
      <form v-if="authLabel" class="authorization-form" @submit.prevent="submitAuth"><h3>当前会话授权</h3><label>{{authLabel}}<input v-model="authValue" :type="connection.auth_state==='authorizationStateWaitPhoneNumber'||connection.auth_state==='authorizationStateWaitEmailAddress'?'text':'password'" autocomplete="off" required></label><button class="primary" :disabled="busy">提交授权</button></form>
      <div class="settings-account-actions"><button :disabled="busy||!ready" @click="logoutAccount">退出当前 Telegram 登录…</button><button :disabled="busy||!profileId" @click="removeAccount">移除所选本地账号…</button></div>
    </section>
    <section v-show="tab==='connection'">
      <form @submit.prevent="saveSettings">
        <label>API ID<input v-model="apiId" type="number" min="1" placeholder="或 TMSGPICKER_API_ID" autocomplete="off"></label>
        <label>API Hash<input v-model="apiHash" type="password" :placeholder="snapshot?.has_saved_api_hash?'已保存；留空保留':'或 TMSGPICKER_API_HASH'" autocomplete="off"></label>
        <label class="check"><input v-model="proxyEnabled" type="checkbox">启用 SOCKS5</label>
        <template v-if="proxyEnabled"><div class="settings-proxy-address"><label>服务器<input v-model="proxyHost" required></label><label>端口<input v-model.number="proxyPort" type="number" min="1" max="65535" required></label></div><label>用户名<input v-model="proxyUser" autocomplete="off"></label><label>代理密码<input v-model="proxyPassword" type="password" :placeholder="snapshot?.has_saved_proxy_password?'已保存；留空保留':'未保存'" autocomplete="off"></label></template>
        <div class="buttons"><button class="primary" :disabled="busy">保存连接设置</button></div><div></div>
      </form>
    </section>
    <section v-show="tab==='security'">
      <h3>保存的凭据</h3><small>Windows DPAPI 保护保存副本。清除凭据不会删除 Session。</small>
      <div class="settings-security-actions"><button :disabled="busy||!snapshot?.has_saved_api_hash" @click="clearSecret('api')">清除保存的 API Hash</button><button :disabled="busy" @click="clearSecret('proxy')">清除代理认证信息</button><button :disabled="busy||!profileId" @click="clearSecret(`db:${profileId}`)">清除所选账号保存的数据库口令</button></div>
      <h3>界面</h3><label class="check"><input :checked="cardCompact" type="checkbox" @change="toggleCard">紧凑用户卡片</label><button @click="resetLayout">重置三栏宽度</button>
      <h3>普通配置</h3><button :disabled="busy" @click="resetSettings">恢复普通配置默认值</button><small>安全重建全部账号；保留账号、秘密和 Telegram Session。</small>
    </section>
  </div>
</main>
</template>
