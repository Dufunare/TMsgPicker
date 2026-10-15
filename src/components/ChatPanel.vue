<script setup lang="ts">
import { useWorkspaceContext } from '../useWorkspace';
import UiIcon from './UiIcon.vue';
import AvatarPreview from './AvatarPreview.vue';
const {chatError,ready,busy,chatList,search,chatScope,selectedChat,loadChats,resolvePublic,snapshot,addAccount,profileId,connection}=useWorkspaceContext();
</script>
<template>
<aside class="chats">
  <form class="chat-toolbar" @submit.prevent="loadChats"><div class="chat-search"><input v-model="search" placeholder="聊天 / @username" aria-label="搜索聊天"><button class="icon-button" title="搜索 / 刷新聊天" aria-label="搜索或刷新聊天" :disabled="!ready||busy"><UiIcon name="refresh"/></button><button class="icon-button" type="button" title="解析公开 username" aria-label="解析公开 username" :disabled="!ready||busy||!search" @click="resolvePublic"><UiIcon name="link"/></button></div><p v-if="chatError" class="error" role="alert">{{chatError}}</p></form>
  <div class="chat-scope" role="group" aria-label="聊天列表"><button type="button" v-for="option in [{id:'all',label:'全部'},{id:'main',label:'普通'},{id:'archive',label:'归档'}]" :key="option.id" :aria-pressed="chatScope===option.id" :disabled="!ready||busy" @click="chatScope=option.id as typeof chatScope;loadChats()">{{option.label}}</button></div><div class="chat-list"><div v-if="snapshot&&!snapshot.profiles.length" class="chat-empty"><p>尚未添加 Telegram 账号</p><button @click="addAccount">添加账号</button></div><p v-else-if="!chatList.length" class="muted chat-empty">{{ready?'暂无聊天':'账号未连接'}}</p><button v-for="chat in chatList" :key="chat.id" class="chat" :class="{chosen:selectedChat?.id===chat.id}" :aria-pressed="selectedChat?.id===chat.id" @click="selectedChat=chat"><AvatarPreview :profile-id="profileId" :generation="connection.generation" :file-id="chat.photo_file_id??undefined" :name="chat.title" size="sm"/><span class="chat-main"><strong>{{chat.title}}</strong><small>{{chat.kind}}</small></span><span v-if="chat.archived" class="chat-archive-tag">归档</span></button></div>
</aside>
</template>
