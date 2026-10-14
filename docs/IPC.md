# Tauri IPC — 常驻多账号 Runtime（2026-10-14）

生产接口仅接受业务 DTO，不允许前端提交任意 TDLib JSON。类型见 `src/types.ts`，Rust 定义见 `core.rs` / `services.rs` / `app.rs`。所有 ID 为十进制字符串，所有日期为 UTC Unix 秒；指标缺失序列化为 null。

## 命令

Tauri 顶层参数用 camelCase，DTO 内部字段用 snake_case。命令错误返回短错误字符串；TDLib 错误仅显示错误码，避免把请求内容或账号数据放进日志/UI 错误文本。

| 命令 | 参数 | 返回 |
|---|---|---|
| `connection_status` | `{profileId}` | `{profile_id,generation,runtime_state,auth_state,connection_state,detail}` |
| `account_statuses` | 无 | 全部 Profile 的 Connection 快照；不含 Client ID |
| `select_profile` | `{profileId}` | 只保存上次查看的 Profile；不创建、初始化、关闭或注销 Client |
| `connect` | `{config:{profile_id,passphrase:string|null,remember_database:boolean}}` | 解锁指定账号的未初始化 Client；如已 Closed 则重建此 Client；不影响其他 Profile。所有 Client 共用后端全局配置 |
| `auth_submit` | `{profileId,value}` | 按指定账号实际授权状态提交手机号／代码／密码／邮箱 |
| `disconnect` | `{profileId}` | 只取消此账号查询、close 并等 Closed；不注销、不删除目录 |
| `chats` | `{profileId,search}` | `Chat[]`；空字符串加载列表，非空搜索已知聊天 |
| `public_chat` | `{profileId,username}` | 公开 Chat，支持 @ 前缀 |
| `saved_messages` | `{profileId}` | 此账号 getMe + createPrivateChat 解析出的 Chat |
| `start_query` | `{profileId,spec,runId}` | 启动此账号后台扫描；新 run 只取消同账号旧 run |
| `cancel_query` | `{profileId,runId}` | 只取消此账号对应 run；请求每约 50ms 检查取消 |
| `message_link` | `{profileId,key:{chat_id,message_id},album}` | TDLib 返回且通过协议检查的 URL |
| `preview_forward` | `{profileId,target,postKeys,runId}` | `ForwardReport`，只预检不发送 |
| `forward_selected` | `{profileId,target,postKeys,runId,confirmed:true}` | `ForwardReport`；只验证此账号的查询结果、选择及权限 |

## 设置与账号命令

| 命令 | 顶层参数 | 返回/行为 |
|---|---|---|
| `open_settings` | `{profileId:string|null}` | 创建唯一 settings WebviewWindow，或聚焦已有窗口并发送 settings-focus-profile；后端验证 Profile，不接受 URL/路径 |
| `close_settings` | 无 | 仅关闭 settings，不触发会话关闭 |
| `save_ui_preferences` | `{cardCompact:boolean}` | 仅保存折叠偏好，保留 API、Profile、秘密与布局 |
| `app_settings` | 无 | `SettingsSnapshot`，仅非敏感字段及 has_saved_* |
| `save_settings` | `{edit:{settings,api_hash:string|null,proxy_password:string|null}}` | Snapshot；秘密 null 保持；安全关闭全部 Client 后统一保存并并发重建。保留最新布局／折叠／上次查看偏好。各账号失败通过 connection 事件报告 |
| `save_layout` | `{left:number,right:number}` | 保存列宽比例，连接期间允许 |
| `reset_settings` | 无 | Snapshot；安全关闭／重建全部账号，仅重置普通配置，保留凭据／Profile／Session |
| `clear_secret` | `{key:"api"\|"proxy"\|"db:<profile_id>"}` | Snapshot；全局秘密清除会安全重建全部 Client，proxy 同时清空用户名；清除 db 只取消对应保存副本，当前解锁会话不关闭 |
| `add_account` | `{name}` | 内部 Profile ID；持久化独立 Profile，创建等待数据库口令的 Client；不接受前端目录 |
| `rename_account` | `{profileId,name}` | Snapshot，本地别名 |
| `remove_account` | `{profileId,confirmed,deleteData,deleteConfirmed}` | Snapshot；先安全关闭指定账号，默认保留 Session；删除数据要求二次确认和路径归属检查，禁止删除 Legacy |
| `logout_account` | `{profileId,confirmed:true}` | 指定已授权账号，UI 和后端均要求确认；logOut 后等 TDLib 自行 Closed，不追加 close；其他账号继续运行 |

`Settings = {schema_version:1,api_id,proxy_enabled,proxy_server,proxy_port,proxy_username,last_profile_id,left_ratio,right_ratio,card_compact}`。

`SettingsSnapshot = {settings,profiles,has_saved_api_hash,has_saved_proxy_password}`。
Profile 字段为 `profile_id,telegram_user_id:string|null,display_name,username:string|null,session_directory_id,is_legacy_profile,created_at,last_used_at,has_saved_database_secret`。首次授权前 Telegram ID 为空；Ready 后 getMe 更新身份。重复身份不写入冲突 Profile，安全断开并提示显式移除；原账号不受影响。Session 标识由后端产生，任何业务命令都不接受 Client ID 或文件路径。

秘密和配置优先级：全局设置／DPAPI 秘密优先，缺少 API 设置时使用环境变量。数据库口令手工输入优先于此 Profile 的保存值；只有初始化成功后按 remember_database 保存／清除副本。UI 取消记住会立即调用 clear_secret 清除副本。保存状态 IPC 不解密、不返回秘密。DPAPI 解密失败只影响对应账号的解锁，手工输入可覆盖失败的保存副本。验证码／2FA 不保存。

`TelegramGateway.request_batch` 在等待前按序提交所有请求，返回值保持提交顺序；先等待最后的参数请求，以便参数错误立即返回并清理其余 pending。TDLib 将初始化前的 addProxy/disableProxy 排队，因此不可用单条阻塞 request 先等待代理确认。代理请求成功仅表示设置被接受，不表示网络连接成功。

## 查询 DTO

```json
{
  "version": 1,
  "chat_id": "-1001",
  "scope": {"kind":"recent","count":50},
  "filter": {
    "kind":"and",
    "items":[
      {"kind":"compare","field":{"kind":"views"},"op":"ge","value":100},
      {"kind":"is_not_null","field":{"kind":"reply_info"}}
    ]
  },
  "sort":[{"field":{"kind":"views"},"descending":true}],
  "output_limit":20
}
```

日期 Scope：`{kind:"dates",from,to,budget}`；区间 `[from,to)`，budget 包括为定位范围而扫描的更新消息。

字段：views、forwards、comments、reply_info、reaction_total、`{kind:"reaction",emoji:"👍"}`、post_kind、media_album_id、content_kind、date。post_kind/content_kind/media_album_id 是字符串，其余是整数；`reply_info` 表示存在的回复信息对应计数，可用 `IS NULL` 检查存在性。post_kind 值仅 single/album。

比较：eq/ne/gt/ge/lt/le；空值检查：`{kind:"is_null"|"is_not_null",field}`；组合：`{kind:"and"|"or",items:[...]}`、`{kind:"not",item:...}`。服务端限制嵌套深度 16、每组条件 100、排序字段 10，校验字段和值类型。空 AND 为 True，空 OR 为 False。

## 事件

| 事件 | 负载 / 行为 |
|---|---|
| `connection` | `{profile_id,generation,runtime_state,auth_state,connection_state,detail}`；不广播原始手机号／验证码／邮箱／密码提示 |
| `query-progress` | `{profile_id,generation,result:ScanResult}`；ScanResult 保持 `{run_id,status,scanned,pages,exact,detail,posts}`，status 为 Scanning／Partial／Complete／Cancelled／Error |
| `forward-update` | `{profile_id,generation,target,old_message_id,state:"Sent"|"Failed"}`；对应发送临时 ID |
| `workspace-settings-changed` | `{source:"main"\|"settings"}`；前端发送不含秘密的刷新通知，另一窗口重新通过 app_settings 读取非敏感状态；不作为会话授权依据 |
| `settings-focus-profile` | `string|null`；后端仅向 settings 窗口发送要显示的 Profile；相同账号仅聚焦，不清空输入 |
| `accounts-changed` | 无敏感负载；前端重新读取 app_settings，刷新 getMe 更新后的 Profile |
| `account-notice` | `{profile_id,detail}`；重复 Telegram 身份或元数据保存失败的提示 |
| `app-error` | 安全关闭失败时的短错误；保持进程，允许重试 |

前端先注册监听，再读取 account_statuses 捕获启动期间已经发生的状态变化。所有账号事件写入对应 Profile 缓存；query-progress 必须同时匹配此 Profile 的 generation 和 run_id。后台执行相同验证。新查询只取消同账号旧 run；切换视图不取消任何查询。

`LogicalPost` 包括 key、kind、members、representative、date、text、metrics、provenance、partial。members 是规范化的 TelegramMessage，包含字符串联合 ID、content_kind、caption/text、media_album_id 和快照 metrics。provenance 为字段到 source_message_id 的映射。

ReactionKey 序列化为 `{kind:"emoji"|"custom_emoji"|"paid"|"unknown",value?}`。所有非 Unicode 反应保留类型和计数。反应对象缺失为 null，已知为空为 `[]`。reaction_total 汇总同一个指标来源内的 reaction 计数，绝不跨 Album 成员累加。

`ForwardReport = {items:[{source:{chat_id,message_id},state,detail,sent_id}]}`。预检为 Allowed/Rejected；执行为 Rejected/Failed/Pending/Sent。Pending 不代表送达，forward-update 按 Profile ID、generation、target、临时 sent_id 匹配；进程不持久化动作记录。

## 会话并发约束

AppState 持全局生命周期 RwLock，普通业务持读锁，各账号独立 lifecycle Mutex，不跨账号串行业务。全局重建／退出持写锁。每个 Runtime 独立 epoch、扫描取消标志、结果与 Pending 转发。关闭失败保留此 Gateway 和失败原因，不在原目录强开新 Client。

整个进程只有一个 SharedReceiver。先验证 @client_id 对应存活 ClientSlot，再通过专属 Router 的 @extra 匹配 Pending；两级键等价于 (client_id, request_tag)。Closed 只清理该 Client 的 Pending，全部 Client Closed 后才停止并 join 共享接收线程。重建获得新 Client ID／generation，旧 Client 消息和旧 generation 回调安全丢弃。

Pending 临时消息 ID 按账号隔离，收到 Sent／Failed 后解除；指定账号有 Pending 时拒绝其断开／移除／注销，全局设置和退出须等待全部 Pending。切换视图始终允许，不调用生命周期 IPC。应用退出先拒绝新任务，全部提交 close 后并发等待 Closed；正常退出不调用 logOut。详情见 [Runtime 架构](RUNTIME_ARCHITECTURE.md)。

## 多窗口生命周期（第二轮 UI）

主窗口 label 为 `main`，独立设置窗口为 `settings`，URL 为本地 `index.html?view=settings`。Settings capability 仅为该窗口提供 core event 访问；窗口创建/关闭通过固定目标的 Rust 命令，不开放任意原始 TDLib 或解密秘密接口。Settings 默认 500 × 580，最小 420 × 440，可调整和内部滚动，重复创建由独立 mutex 串行，已有窗口解除最小化并聚焦。

`on_window_event` 的 CloseRequested 通过 `route_window_close` 只对 main 执行 prevent_close + begin_safe_exit；settings 关闭正常销毁该小窗，不调用 close_session，不修改 generation、查询或发送状态。main 关闭即使没有 Gateway，也退出整个应用，避免遗留 Settings。原有 RunEvent 安全退出与 Pending 发送保护保留。

`card_compact` 是 schema 1 的向后兼容普通字段，旧配置缺失时为 false；不自动重置账号、凭据或栏宽。Settings 保存 API/代理前重新读取最新普通配置，避免用打开窗口时的旧快照覆盖主窗口后来调整的布局或折叠偏好。
