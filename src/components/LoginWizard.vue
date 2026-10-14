<script setup lang="ts">
import {useLogin} from '../useLogin';
const {snapshot,connection,phone,passphrase,remember,recoveryId,value,busy,error,label,pending,begin,submit,cancel,openSettings,retryIdentity}=useLogin();
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
  <p class="muted">{{connection?.detail}}</p><p v-if="connection?.connection_state!=='connectionStateReady'" class="muted">等待网络连接；请检查全局 SOCKS5 配置。</p>
  <form v-if="label" @submit.prevent="submit"><label>{{label}}<input v-model="value" :type="connection?.auth_state==='authorizationStateWaitPhoneNumber'||connection?.auth_state==='authorizationStateWaitEmailAddress'?'text':'password'" autocomplete="off" required></label><button class="primary" :disabled="busy">提交</button></form>
  <p v-else-if="connection?.auth_state==='authorizationStateReady'">正在读取 Telegram 身份并登记账号…</p>
  <p v-else-if="connection?.auth_state==='authorizationStateWaitOtherDeviceConfirmation'">请在已登录的 Telegram 设备完成授权确认。</p>
  
  <div class="buttons"><button v-if="connection?.auth_state==='authorizationStateReady'" :disabled="busy" @click="retryIdentity">重试读取并登记身份</button><button :disabled="busy" @click="cancel">取消登录</button></div>
</template>
<button @click="openSettings">打开应用设置</button>
</div></main>
</template>
