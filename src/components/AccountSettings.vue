<script setup lang="ts">
import {computed} from 'vue';
import {useWorkspaceContext} from '../useWorkspace';
import {accountStatus} from '../accountViews';
const {snapshot,profileId,connection,accountError,notice,busy,passphrase,authLabel,authValue,connect,submitAuth,disconnect,logoutAccount,addAccount}=useWorkspaceContext();
const profile=computed(()=>snapshot.value?.profiles.find(p=>p.profile_id===profileId.value));
const needsPassword=computed(()=>connection.value.runtime_state==='NeedsDatabasePassphrase'||(connection.value.user_disconnected&&!profile.value?.has_saved_database_secret)||connection.value.runtime_state==='Error');
</script>
<template>
<main class="settings-page"><div class="settings-content">
  <h2>账号设置</h2><p v-if="accountError" class="error" role="alert">{{accountError}}</p><p v-if="notice" class="notice">{{notice}}</p>
  <p v-if="!profile" class="muted">此账号已注销，设置窗口已失效。</p>
  <template v-else>
    <h3>{{profile.display_name}}</h3><p v-if="profile.username">@{{profile.username}}</p><p class="muted">Telegram User ID：{{profile.telegram_user_id}}<br>{{accountStatus(connection)}} · {{connection.detail}}</p>
    <form v-if="needsPassword" @submit.prevent="connect"><label>原有本地数据库密码<input v-model="passphrase" type="password" autocomplete="off" required></label><button class="primary" :disabled="busy">继续恢复 Session</button></form>
    <form v-if="authLabel" @submit.prevent="submitAuth"><p>TDLib 判断此 Session 需要重新授权。</p><label>{{authLabel}}<input v-model="authValue" :type="connection.auth_state==='authorizationStateWaitPhoneNumber'||connection.auth_state==='authorizationStateWaitEmailAddress'?'text':'password'" autocomplete="off" required></label><button :disabled="busy">提交</button></form>
    <div class="buttons"><button v-if="connection.user_disconnected&&!needsPassword" :disabled="busy" @click="connect">恢复连接</button><button v-else-if="!connection.user_disconnected" :disabled="busy||connection.runtime_state==='Closed'||connection.runtime_state==='Closing'||connection.runtime_state==='LogoutCleanupRequired'" @click="disconnect">断开连接</button><button :disabled="busy||(connection.runtime_state!=='Ready'&&connection.runtime_state!=='LogoutCleanupRequired')" @click="logoutAccount">{{connection.runtime_state==='LogoutCleanupRequired'?'重试完成注销':'注销账号…'}}</button><button @click="addAccount">添加新账号</button></div>
  </template>
</div></main>
</template>
