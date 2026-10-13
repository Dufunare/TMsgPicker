<script setup lang="ts">
import { useWorkspaceContext } from '../useWorkspace';
const {chatError,ready,busy,chatList,search,selectedChat,loadChats,resolvePublic}=useWorkspaceContext();
</script>
<template>
<aside class="panel chats"><h2>Chats</h2><p v-if="chatError" class="error" role="alert">{{chatError}}</p><form @submit.prevent="loadChats"><input v-model="search" placeholder="已有聊天标题 / 公开 username" aria-label="搜索聊天"><div class="buttons"><button :disabled="!ready || busy">搜索 / 刷新</button><button type="button" :disabled="!ready || busy || !search" @click="resolvePublic">解析 @username</button></div></form><p v-if="!chatList.length" class="muted">登录后刷新聊天列表。可选择频道、群组或私人聊天。</p><button v-for="chat in chatList" :key="chat.id" class="chat" :class="{ chosen: selectedChat?.id===chat.id }" @click="selectedChat=chat"><strong>{{ chat.title }}</strong><small>{{ chat.kind }} · {{ chat.id }}</small></button></aside>
      
</template>
