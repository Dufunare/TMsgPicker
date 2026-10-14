<script setup lang="ts">
import {useWorkspaceContext} from '../useWorkspace';
const {accountError,notice,apiId,apiHash,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,busy,snapshot,saveSettings,clearSecret,debugReset,resetReport,resetBusy}=useWorkspaceContext();
</script>
<template>
<main class="settings-page"><div class="settings-content"><h2>应用设置</h2>
<p v-if="accountError" class="error" role="alert">{{accountError}}</p><p v-if="notice" class="notice">{{notice}}</p>
<form @submit.prevent="saveSettings">
<label>Telegram API ID<input v-model="apiId" type="number" min="1" placeholder="或 TMSGPICKER_API_ID" autocomplete="off"></label>
<label>Telegram API Hash<input v-model="apiHash" type="password" :placeholder="snapshot?.has_saved_api_hash?'已保存；留空保留':'或 TMSGPICKER_API_HASH'" autocomplete="off"></label>
<label class="check"><input v-model="proxyEnabled" type="checkbox">启用 SOCKS5</label>
<template v-if="proxyEnabled"><div class="settings-proxy-address"><label>服务器<input v-model="proxyHost" required></label><label>端口<input v-model.number="proxyPort" type="number" min="1" max="65535" required></label></div><label>用户名<input v-model="proxyUser" autocomplete="off"></label><label>代理密码<input v-model="proxyPassword" type="password" :placeholder="snapshot?.has_saved_proxy_password?'已保存；留空保留':'未保存'" autocomplete="off"></label></template>
<button class="primary" :disabled="resetBusy||busy">保存应用配置</button><small>对所有账号生效；保存时安全关闭并重建运行中的账号。</small>
</form>
<h3>全局敏感凭据</h3><div class="buttons"><button :disabled="resetBusy||busy||!snapshot?.has_saved_api_hash" @click="clearSecret('api')">清除保存的 API Hash</button><button :disabled="resetBusy||busy" @click="clearSecret('proxy')">清除代理认证信息</button></div>
<section v-if="snapshot?.debug_tools" class="debug-tools"><h3>Debug 工具</h3><button :disabled="resetBusy||busy" @click="debugReset">重置 TMsgPicker（注销全部账号并清除全部本地数据）</button><small>注销全部账号并永久删除本地数据；成功后退出应用，需要重新启动。</small><div v-if="resetReport&&!resetReport.completed" role="alert"><p class="error">{{resetReport.error||'完整重置未完成'}}</p><ul><li v-for="item in resetReport.accounts" :key="item.profile_id">{{item.display_name}}（{{item.profile_id}}）：{{item.completed?'注销／关闭已完成':`未完成：${item.error}`}}</li></ul><p>恢复失败账号的原数据库口令、授权或网络后，可再次点击重置。已完成注销的账号不会重复注销。</p></div></section>
</div></main>
</template>
