<script setup lang="ts">
import {useLogin} from '../useLogin';
import AppSelect from './AppSelect.vue';
const {snapshot,connection,phone,passphrase,remember,recoveryId,value,twoFactorPassword,busy,error,label,pending,sendingPhone,combinedAuth,begin,submit,cancel,openSettings,retryIdentity}=useLogin();

// Diagnostic-only, read-only description of Telegram's declared delivery route.
// All raw TDLib fields are allowlisted and stripped of PII by the Rust backend.
const codeChannelLabels:Record<string,string>={
  authenticationCodeTypeTelegramMessage:'已登录的 Telegram 客户端内消息',
  authenticationCodeTypeSms:'短信（SMS）',
  authenticationCodeTypeSmsWord:'短信单词',
  authenticationCodeTypeSmsPhrase:'短信短语',
  authenticationCodeTypeCall:'语音电话',
  authenticationCodeTypeFlashCall:'闪电来电',
  authenticationCodeTypeMissedCall:'未接来电验证',
  authenticationCodeTypeFragment:'Fragment 平台',
  authenticationCodeTypeFirebaseAndroid:'Android Firebase 验证',
  authenticationCodeTypeFirebaseIos:'iOS Firebase 验证',
  authenticationCodeTypeEmailCode:'邮箱验证码',
  authenticationCodeTypeSetUpEmail:'设置验证邮箱',
  authenticationCodeTypeGoogleId:'Google 身份验证',
  authenticationCodeTypeAppleId:'Apple 身份验证'
};
function codeChannelLabel(kind:string|null|undefined):string {
  if(!kind)return '未提供';
  return codeChannelLabels[kind]??'其他或未识别的投递方式';
}
</script>
<template>
<main class="settings-page"><div class="settings-content"><h2>添加 Telegram 账号</h2>
<p v-if="error" class="error" role="alert">{{error}}</p>
<form v-if="!pending" @submit.prevent="begin">
  <label v-if="snapshot?.interrupted_profiles.length">本地会话<AppSelect v-model="recoveryId" label="本地会话" :options="[{value:'',label:'创建新的独立 Session'},...snapshot.interrupted_profiles.map(p=>({value:p.profile_id,label:'恢复未完成登录 · '+(p.is_legacy_profile?'Legacy':p.profile_id)}))]"/></label>
  <label>手机号（含国际区号）<input v-model="phone" type="tel" placeholder="+86…" autocomplete="off" :required="!recoveryId"></label>
  <label>{{recoveryId?'原有本地数据库密码':'设置本地数据库密码'}}<input v-model="passphrase" type="password" autocomplete="off" required></label>
  <label class="check"><input v-model="remember" type="checkbox">记住此设备（Windows DPAPI 保护）</label><small>未记住时，下次启动需输入原数据库密码以恢复 Session。</small>
  <div class="buttons"><button class="primary" :disabled="busy">{{busy?'正在创建会话…':'开始登录'}}</button><button type="button" :disabled="busy" @click="cancel">取消</button></div>
</form>
<template v-else>
  <p class="muted">{{sendingPhone?'正在提交手机号并请求验证码…':connection?.detail}}</p><p v-if="connection?.connection_state!=='connectionStateReady'" class="muted">等待网络连接；请检查全局 SOCKS5 配置。</p>
  <section v-if="connection?.auth_state==='authorizationStateWaitCode'" class="settings-card" aria-label="验证码投递诊断">
    <h3>TDLib 验证码投递诊断</h3>
    <p>授权状态：<code>{{connection.auth_state}}</code></p>
    <p>网络状态：<code>{{connection.connection_state}}</code></p>
    <template v-if="connection.code_diagnostics?.code_info_received">
      <p>本次投递渠道：<strong>{{codeChannelLabel(connection.code_diagnostics.code_type)}}</strong></p>
      <p class="muted">code_type：<code>{{connection.code_diagnostics.code_type??'未提供'}}</code></p>
      <p>备用投递渠道：<strong>{{connection.code_diagnostics.next_code_type?codeChannelLabel(connection.code_diagnostics.next_code_type):'服务端未提供（next_type = null）'}}</strong></p>
      <p class="muted">next_type：<code>{{connection.code_diagnostics.next_code_type??'null'}}</code></p>
      <p>服务器返回的重发等待：{{connection.code_diagnostics.timeout_seconds===null?'未知':connection.code_diagnostics.timeout_seconds+' 秒（状态快照）'}}</p>
      <p>验证码长度：{{connection.code_diagnostics.length===null?'未知':connection.code_diagnostics.length+' 位'}}</p>
    </template>
    <p v-else>当前已进入等待验证码状态，但此授权事件中没有收到有效的 <code>code_info</code>。</p>
    <small>这是 TDLib 报告的投递方式，并不保证验证码实际送达。若显示 Telegram 客户端内消息，请检查已登录官方客户端中的 Telegram 服务通知聊天（通常为 777000，包括归档）。本诊断不会自动重发验证码；不要连续重复请求。</small>
  </section>
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
