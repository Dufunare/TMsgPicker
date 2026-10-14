<script setup lang="ts">
import {useLogin} from '../useLogin';
const {snapshot,connection,phone,passphrase,remember,recoveryId,value,twoFactorPassword,busy,error,label,pending,sendingPhone,combinedAuth,begin,submit,cancel,openSettings,retryIdentity}=useLogin();
</script>
<template>
<main class="settings-page"><div class="settings-content"><h2>添加 Telegram 账号</h2>
<p v-if="error" class="error" role="alert">{{error}}</p>
<form v-if="!pending" @submit.prevent="begin">
  <label v-if="snapshot?.interrupted_profiles.length">本地会话<select v-model="recoveryId"><option value="">创建新的独立 Session</option><option v-for="p in snapshot.interrupted_profiles" :key="p.profile_id" :value="p.profile_id">恢复未完成登录 · {{p.is_legacy_profile?'Legacy':p.profile_id}}</option></select></label>
  <label>手机号（含国际区号）<input v-model="phone" type="tel" placeholder="+86…" autocomplete="off" :required="!recoveryId"></label>
  <label>{{recoveryId?'原有本地数据库密码':'设置本地数据库密码'}}<input v-model="passphrase" type="password" autocomplete="off" required></label>
  <label class="check"><input v-model="remember" type="checkbox">记住此设备（Windows DPAPI 保护）</label><small>未记住时，下次启动需输入原数据库密码以恢复 Session。</small>
  <div class="buttons"><button class="primary" :disabled="busy">{{busy?'正在创建会话…':'开始登录'}}</button><button type="button" :disabled="busy" @click="cancel">取消</button></div>
</form>
<template v-else>
  <p class="muted">{{sendingPhone?'正在提交手机号并请求验证码…':connection?.detail}}</p><p v-if="connection?.connection_state!=='connectionStateReady'" class="muted">等待网络连接；请检查全局 SOCKS5 配置。</p>
  <form v-if="combinedAuth" class="authorization-form" @submit.prevent="submit"><label>Telegram 验证码<input v-model="value" type="text" inputmode="numeric" autocomplete="off" :disabled="busy||connection?.auth_state==='authorizationStateWaitPassword'" :required="connection?.auth_state==='authorizationStateWaitCode'" :placeholder="connection?.auth_state==='authorizationStateWaitPassword'?'验证码已验证':''"></label><label>Telegram 两步验证密码（2FA）<input v-model="twoFactorPassword" type="password" autocomplete="off" :disabled="busy" :required="connection?.auth_state==='authorizationStateWaitPassword'" placeholder="已启用两步验证时填写"></label><small>已启用两步验证请同时填写密码；未启用可留空。验证码通过后会自动完成两步验证。</small><button class="primary" :disabled="busy">{{busy?'正在验证…':connection?.auth_state==='authorizationStateWaitPassword'?'提交两步验证密码':'验证并登录'}}</button></form>
  <form v-else-if="label" @submit.prevent="submit"><label>{{label}}<input v-model="value" :type="connection?.auth_state==='authorizationStateWaitPhoneNumber'||connection?.auth_state==='authorizationStateWaitEmailAddress'?'text':'password'" autocomplete="off" required></label><button class="primary" :disabled="busy">提交</button></form>
  <p v-else-if="sendingPhone" class="muted">验证码发送后会自动显示验证表单，无需再次输入手机号。</p>
  <p v-else-if="connection?.auth_state==='authorizationStateReady'">正在读取 Telegram 身份并登记账号…</p>
  <p v-else-if="connection?.auth_state==='authorizationStateWaitOtherDeviceConfirmation'">请在已登录的 Telegram 设备完成授权确认。</p>
  <p v-else class="muted">登录步骤由 Telegram 返回的授权状态决定。初始化失败时可取消后使用原密码重试。</p>
  <div class="buttons"><button v-if="connection?.auth_state==='authorizationStateReady'" :disabled="busy" @click="retryIdentity">重试读取并登记身份</button><button :disabled="busy" @click="cancel">取消登录</button></div>
</template>
<p class="muted">API 与代理配置来自应用设置，对所有账号生效。</p><button @click="openSettings">打开应用设置</button>
</div></main>
</template>
