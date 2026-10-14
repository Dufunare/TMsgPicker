<script setup lang="ts">
import { useWorkspaceContext } from '../useWorkspace';
import UiIcon from './UiIcon.vue';
const {chatError,ready,busy,chatList,search,selectedChat,loadChats,resolvePublic,snapshot,addAccount}=useWorkspaceContext();
</script>
<template>
<aside class="chats">
  <form class="chat-toolbar" @submit.prevent="loadChats"><div class="chat-search"><input v-model="search" placeholder="聊天 / @username" aria-label="搜索聊天"><button class="icon-button" title="搜索 / 刷新聊天" aria-label="搜索或刷新聊天" :disabled="!ready||busy"><UiIcon name="refresh"/></button><button class="icon-button" type="button" title="解析公开 username" aria-label="解析公开 username" :disabled="!ready||busy||!search" @click="resolvePublic"><UiIcon name="link"/></button></div><p v-if="chatError" class="error" role="alert">{{chatError}}</p></form>
  <div class="chat-list"><div v-if="snapshot&&!snapshot.profiles.length" class="chat-empty"><p>尚未添加 Telegram 账号</p><button @click="addAccount">添加账号</button></div><p v-else-if="!chatList.length" class="muted chat-empty">{{ready?'暂无聊天':'账号未连接'}}</p><button v-for="chat in chatList" :key="chat.id" class="chat" :class="{chosen:selectedChat?.id===chat.id}" :aria-pressed="selectedChat?.id===chat.id" @click="selectedChat=chat"><strong>{{chat.title}}</strong><small>{{chat.kind}} · {{chat.id}}</small></button></div>
</aside>
</template>
