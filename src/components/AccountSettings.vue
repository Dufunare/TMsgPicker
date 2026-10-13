<script setup lang="ts">
import {ref,onMounted} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
const element=ref<HTMLDialogElement>();onMounted(()=>element.value?.showModal());
const {accountError,notice,apiId,apiHash,proxyEnabled,proxyHost,proxyPort,proxyUser,proxyPassword,busy,snapshot,profileId,settingsOpen,saveSettings,clearSecret,resetSettings}=useWorkspaceContext();
</script>
<template><dialog ref="element" class="settings-dialog" aria-label="应用设置" @cancel.prevent="settingsOpen=false"><div class="brand"><h2>应用设置</h2><button @click="settingsOpen=false" autofocus>关闭</button></div><p v-if="accountError" class="error" role="alert">{{accountError}}</p><p v-if="notice" class="notice">{{notice}}</p><p>API 与代理设置供所有账号共用。修改前请安全断开当前会话。</p><form @submit.prevent="saveSettings">
<label>API ID<input v-model="apiId" type="number" min="1" placeholder="或 TMSGPICKER_API_ID" autocomplete="off"></label>
<label>API Hash<input v-model="apiHash" type="password" :placeholder="snapshot?.has_saved_api_hash?'已保存；留空保留':'或 TMSGPICKER_API_HASH'" autocomplete="off"></label>
<button type="button" :disabled="busy||!snapshot?.has_saved_api_hash" @click="clearSecret('api')">清除已保存 API Hash</button>
<label class="check"><input v-model="proxyEnabled" type="checkbox">启用 SOCKS5</label>
<template v-if="proxyEnabled"><label>服务器<input v-model="proxyHost" required></label><label>端口<input v-model.number="proxyPort" type="number" min="1" max="65535" required></label><label>用户名<input v-model="proxyUser" autocomplete="off"></label><label>代理密码<input v-model="proxyPassword" type="password" :placeholder="snapshot?.has_saved_proxy_password?'已保存；留空保留':'未保存'" autocomplete="off"></label></template>
<button type="button" :disabled="busy" @click="clearSecret('proxy')">清除代理认证信息</button>
<button type="button" :disabled="busy||!profileId" @click="clearSecret(`db:${profileId}`)">清除所选账号保存的数据库口令</button>
<div class="buttons"><button class="primary" :disabled="busy">保存设置</button><button type="button" :disabled="busy" @click="resetSettings">恢复普通配置默认值</button></div>
<small>秘密不回填。留空保留已保存值；使用清除按钮删除。数据库口令保持原值，取消记住不会删除 Session。配置读取失败会报错，不会覆盖损坏文件。</small>
</form></dialog></template>
