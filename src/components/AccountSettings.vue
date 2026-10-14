<script setup lang="ts">
import {computed} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
import {accountStatus} from '../accountViews';
const {snapshot,profileId,connection,accountError,notice,busy,resetBusy,passphrase,authLabel,authValue,connect,submitAuth,disconnect,logoutAccount,addAccount,selectAccount}=useWorkspaceContext();
const profile=computed(()=>snapshot.value?.profiles.find(p=>p.profile_id===profileId.value));
const needsPassword=computed(()=>connection.value.runtime_state==='NeedsDatabasePassphrase'||(connection.value.user_disconnected&&!profile.value?.has_saved_database_secret)||connection.value.runtime_state==='Error');
</script>
<template>
<section class="settings-content">
  <h2>账号设置</h2><p v-if="accountError" class="error" role="alert">{{accountError}}</p><p v-if="notice" class="notice">{{notice}}</p>
  <label v-if="snapshot?.profiles.length">当前设置的账号<select v-model="profileId" :disabled="busy||resetBusy" @change="selectAccount()"><option v-for="account in snapshot.profiles" :key="account.profile_id" :value="account.profile_id">{{account.display_name}}</option></select></label>
  <template v-if="!profile"><p class="muted">{{snapshot?.profiles.length?'请选择要设置的账号。':'尚未添加 Telegram 账号。'}}</p><button @click="addAccount">添加新账号</button></template>
  <template v-else>
    <h3>{{profile.display_name}}</h3><p v-if="profile.username">@{{profile.username}}</p><p class="muted">Telegram User ID：{{profile.telegram_user_id}}<br>{{accountStatus(connection)}} · {{connection.detail}}</p>
    <form v-if="needsPassword" @submit.prevent="connect"><label>原有本地数据库密码<input v-model="passphrase" type="password" autocomplete="off" required></label><small>用于打开现有加密 Session；不会更换密钥或重新注册账号。</small><button class="primary" :disabled="busy">继续恢复 Session</button></form>
    <form v-if="authLabel" @submit.prevent="submitAuth"><p>TDLib 判断此 Session 需要重新授权。</p><label>{{authLabel}}<input v-model="authValue" :type="connection.auth_state==='authorizationStateWaitPhoneNumber'||connection.auth_state==='authorizationStateWaitEmailAddress'?'text':'password'" autocomplete="off" required></label><button :disabled="busy">提交</button></form>
    <div class="buttons"><button v-if="connection.user_disconnected&&!needsPassword" :disabled="busy" @click="connect">恢复连接</button><button v-else-if="!connection.user_disconnected" :disabled="busy||connection.runtime_state==='Closed'||connection.runtime_state==='Closing'||connection.runtime_state==='LogoutCleanupRequired'" @click="disconnect">断开连接</button><button :disabled="busy||(connection.runtime_state!=='Ready'&&connection.runtime_state!=='LogoutCleanupRequired')" @click="logoutAccount">{{connection.runtime_state==='LogoutCleanupRequired'?'重试完成注销':'注销账号…'}}</button><button @click="addAccount">添加新账号</button></div>
  </template>
</section>
</template>
