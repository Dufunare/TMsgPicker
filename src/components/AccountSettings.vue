<script setup lang="ts">
import {computed} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
import {accountStatus} from '../accountViews';
import AvatarPreview from './AvatarPreview.vue';
const {snapshot,profileId,connection,accountError,notice,busy,resetBusy,passphrase,authLabel,authValue,connect,submitAuth,disconnect,logoutAccount,addAccount}=useWorkspaceContext();
const profile=computed(()=>snapshot.value?.profiles.find(p=>p.profile_id===profileId.value));
const needsPassword=computed(()=>connection.value.runtime_state==='NeedsDatabasePassphrase'||(connection.value.user_disconnected&&!profile.value?.has_saved_database_secret)||connection.value.runtime_state==='Error');
</script>
<template>
<section class="settings-content">
  <p v-if="accountError" class="error" role="alert">{{accountError}}</p><p v-if="notice" class="notice">{{notice}}</p>
  <section class="settings-card" aria-labelledby="account-info-title">
  <h3 id="account-info-title">账号信息</h3>
  <div v-if="profile" class="settings-account-badge"><span>{{profile.display_name}}</span><AvatarPreview account :profile-id="profileId" :generation="connection.generation" :name="profile.display_name" size="sm"/></div>
  <template v-if="!profile"><p class="muted">{{snapshot?.profiles.length?'请选择要设置的账号。':'尚未添加 Telegram 账号。'}}</p><button @click="addAccount">添加新账号</button></template>
  <template v-else>
    <p v-if="profile.username">@{{profile.username}}</p><p class="muted">Telegram User ID：{{profile.telegram_user_id}}<br>{{accountStatus(connection)}} · {{connection.detail}}</p>
  </template>
  </section>
  <template v-if="profile">
    <form v-if="needsPassword" class="settings-card" aria-labelledby="session-settings-title" @submit.prevent="connect"><h3 id="session-settings-title">Session 恢复</h3><label>原有本地数据库密码<input v-model="passphrase" type="password" autocomplete="off" required></label><small>用于打开现有加密 Session；不会更换密钥或重新注册账号。</small><div class="buttons"><button class="primary" :disabled="busy">继续恢复 Session</button></div></form>
    <form v-if="authLabel" class="settings-card" aria-labelledby="authorization-settings-title" @submit.prevent="submitAuth"><h3 id="authorization-settings-title">Telegram 授权</h3><p class="muted">TDLib 判断此 Session 需要重新授权。</p><label>{{authLabel}}<input v-model="authValue" :type="connection.auth_state==='authorizationStateWaitPhoneNumber'||connection.auth_state==='authorizationStateWaitEmailAddress'?'text':'password'" autocomplete="off" required></label><div class="buttons"><button :disabled="busy">提交</button></div></form>
    <section class="settings-card" aria-labelledby="account-actions-title">
    <h3 id="account-actions-title">账号操作</h3>
    <div class="buttons"><button v-if="connection.user_disconnected&&!needsPassword" :disabled="busy" @click="connect">恢复连接</button><button v-else-if="!connection.user_disconnected" :disabled="busy||connection.runtime_state==='Closed'||connection.runtime_state==='Closing'||connection.runtime_state==='LogoutCleanupRequired'" @click="disconnect">断开连接</button><button :disabled="busy||(connection.runtime_state!=='Ready'&&connection.runtime_state!=='LogoutCleanupRequired')" @click="logoutAccount">{{connection.runtime_state==='LogoutCleanupRequired'?'重试完成注销':'注销账号'}}</button><button @click="addAccount">添加新账号</button></div>
    </section>
  </template>
</section>
</template>
