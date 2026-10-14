# Tauri IPC — 三栏与多账号迭代

生产接口仅接受业务 DTO，不允许前端提交任意 TDLib JSON。类型见 `src/types.ts`，Rust 定义见 `core.rs` / `services.rs` / `app.rs`。所有 ID 为十进制字符串，所有日期为 UTC Unix 秒；指标缺失序列化为 null。

## 命令

Tauri 顶层参数用 camelCase，DTO 内部字段用 snake_case。命令错误返回短错误字符串；TDLib 错误仅显示错误码，避免把请求内容或账号数据放进日志/UI 错误文本。

| 命令 | 参数 | 返回 |
|---|---|---|
| `connection_status` | 无 | `{profile_id,generation,auth_state,connection_state,detail}` |
| `connect` | `{config:{profile_id,passphrase:string|null,remember_database:boolean}}` | 安全关闭旧 Gateway、等待 Closed 和线程 join 后激活指定 Profile；从后端设置及 DPAPI 凭据初始化，仍按序先提交代理、参数再等待 |
| `auth_submit` | `{value}` | 按当前授权状态提交手机号/代码/密码/邮箱 |
| `disconnect` | 无 | 取消查询，close 并等待 authorizationStateClosed |
| `chats` | `{search}` | `Chat[]`；空字符串加载列表，非空搜索已知聊天 |
| `public_chat` | `{username}` | 公开 Chat，支持输入 @ 前缀 |
| `saved_messages` | 无 | getMe + createPrivateChat 解析出的 Chat |
| `start_query` | `{spec,runId}` | 立即启动后台扫描；结果由事件发送 |
| `cancel_query` | `{runId}` | 仅取消对应活动 run；等待中的请求最多约 50ms 检查取消 |
| `message_link` | `{key:{chat_id,message_id},album}` | TDLib 返回且通过协议检查的 URL |
| `preview_forward` | `{target,postKeys,runId}` | `ForwardReport`，只预检不发送 |
| `forward_selected` | `{target,postKeys,runId,confirmed:true}` | `ForwardReport`；校验选择属于最近查询结果，再重新检查权限 |

## 设置与账号命令

| 命令 | 顶层参数 | 返回/行为 |
|---|---|---|
| `open_settings` | `{profileId:string|null}` | 创建唯一 settings WebviewWindow，或聚焦已有窗口并发送 settings-focus-profile；后端验证 Profile，不接受 URL/路径 |
| `close_settings` | 无 | 仅关闭 settings，不触发会话关闭 |
| `save_ui_preferences` | `{cardCompact:boolean}` | 仅保存折叠偏好，保留 API、Profile、秘密与布局 |
| `app_settings` | 无 | `SettingsSnapshot`，仅非敏感字段及 has_saved_* |
| `save_settings` | `{edit:{settings,api_hash:string|null,proxy_password:string|null}}` | Snapshot；秘密 null 表示保持，非空值更新 DPAPI 密文；Gateway 运行时拒绝 |
| `save_layout` | `{left:number,right:number}` | 保存列宽比例，连接期间允许 |
| `reset_settings` | 无 | Snapshot；须先断开，仅重置普通配置 |
| `clear_secret` | `{key:"api"\|"proxy"\|"db:<profile_id>"}` | Snapshot；删除 DPAPI 密文，proxy 同时清空用户名；运行中的连接不受影响 |
| `add_account` | `{name}` | 内部 Profile ID；独立路径，不接受前端目录 |
| `rename_account` | `{profileId,name}` | Snapshot，本地别名 |
| `remove_account` | `{profileId,confirmed,deleteData,deleteConfirmed}` | Snapshot；活动账号须先断开，默认保留数据，删除数据要求第二次确认并验证归属；拒绝删除 Legacy 数据 |
| `logout_account` | `{profileId,confirmed:true}` | 仅当前 Ready 账号；logOut 后等待 Closed 和线程结束，普通断开不调用此命令 |

`Settings = {schema_version:1,api_id,proxy_enabled,proxy_server,proxy_port,proxy_username,last_profile_id,left_ratio,right_ratio,card_compact}`。

`SettingsSnapshot = {settings,profiles,has_saved_api_hash,has_saved_proxy_password}`。
Profile 字段为 `profile_id,telegram_user_id:string|null,display_name,username:string|null,is_legacy_profile,last_used_at,has_saved_database_secret`。首次授权前 Telegram ID 为空；Ready 后 getMe 更新身份。username/Telegram ID 只作为元数据，不构造 Session 路径。

秘密和配置优先级：后端保存设置/DPAPI 秘密优先，缺少 API 设置时使用环境变量。数据库口令手工输入优先于对应 Profile 保存的口令；`remember_database=false` 清除保存值，不改变传入的口令。保存状态 IPC 不解密，不返回真实秘密。DPAPI 解密失败明确报错，用户可手工覆盖数据库口令或清除/重新设置全局凭据。验证码/2FA 不保存。

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
| `connection` | 包含 profile_id 和递增 generation，以及授权状态类型、连接状态类型和通用提示；不广播原始手机号/验证码/邮箱/密码提示 |
| `query-progress` | `ScanResult = {run_id,status,scanned,pages,exact,detail,posts}`；status 为 Scanning/Partial/Complete/Cancelled/Error |
| `forward-update` | `{generation,target,old_message_id,state:"Sent"|"Failed"}`；对应发送临时 ID |
| `workspace-settings-changed` | `{source:"main"\|"settings"}`；前端发送不含秘密的刷新通知，另一窗口重新通过 app_settings 读取非敏感状态；不作为会话授权依据 |
| `settings-focus-profile` | `string|null`；后端仅向 settings 窗口发送要显示的 Profile；相同账号仅聚焦，不清空输入 |
| `accounts-changed` | 无敏感负载；前端重新读取 app_settings，刷新 getMe 更新后的 Profile |
| `account-notice` | 重复 Telegram 身份或元数据保存失败的通用提示 |
| `app-error` | 安全关闭失败时的短错误；保持进程，允许重试 |

前端应先注册监听再启动命令；只应用等于当前 run_id 的扫描事件，后台同样检查活动 run。开始新查询自动取消旧 run，旧请求的迟到响应不会覆盖新结果。

`LogicalPost` 包括 key、kind、members、representative、date、text、metrics、provenance、partial。members 是规范化的 TelegramMessage，包含字符串联合 ID、content_kind、caption/text、media_album_id 和快照 metrics。provenance 为字段到 source_message_id 的映射。

ReactionKey 序列化为 `{kind:"emoji"|"custom_emoji"|"paid"|"unknown",value?}`。所有非 Unicode 反应保留类型和计数。反应对象缺失为 null，已知为空为 `[]`。reaction_total 汇总同一个指标来源内的 reaction 计数，绝不跨 Album 成员累加。

`ForwardReport = {items:[{source:{chat_id,message_id},state,detail,sent_id}]}`。预检为 Allowed/Rejected；执行为 Rejected/Failed/Pending/Sent。Pending 不代表送达，监听 forward-update 并按 target + 临时 sent_id 匹配；进程不持久化动作记录。

## 会话并发约束

Lifecycle mutex 串行连接、授权、关闭、查询启动、预检/转发与依赖会话的读取。扫描继续使用独立取消标记和 run ID；切换前取消并失效结果，关闭失败保持 Gateway 且禁止业务调用，不启动第二个客户端。唯一 native ACTIVE 守卫继续生效。接收回调捕获 generation，过期回调不写状态，连接/转发事件带 generation；查询事件沿用原 ScanResult DTO，以 generation + run ID 在后端过滤，以当前 run ID 在前端过滤。前端聊天异步响应也检查会话 token。

已提交发送与扫描取消分开：后台维护 Pending 临时消息 ID，收到 Sent/Failed 更新后解除；有提交中或 Pending 发送时拒绝切换、断开和窗口退出，用户等待最终状态后重试。不会把 Pending 误报为 Sent。退出应用与切换抢占同一 lifecycle 锁，冲突时提示重试。

## 多窗口生命周期（第二轮 UI）

主窗口 label 为 `main`，独立设置窗口为 `settings`，URL 为本地 `index.html?view=settings`。Settings capability 仅为该窗口提供 core event 访问；窗口创建/关闭通过固定目标的 Rust 命令，不开放任意原始 TDLib 或解密秘密接口。Settings 默认 500 × 580，最小 420 × 440，可调整和内部滚动，重复创建由独立 mutex 串行，已有窗口解除最小化并聚焦。

`on_window_event` 的 CloseRequested 通过 `route_window_close` 只对 main 执行 prevent_close + begin_safe_exit；settings 关闭正常销毁该小窗，不调用 close_session，不修改 generation、查询或发送状态。main 关闭即使没有 Gateway，也退出整个应用，避免遗留 Settings。原有 RunEvent 安全退出与 Pending 发送保护保留。

`card_compact` 是 schema 1 的向后兼容普通字段，旧配置缺失时为 false；不自动重置账号、凭据或栏宽。Settings 保存 API/代理前重新读取最新普通配置，避免用打开窗口时的旧快照覆盖主窗口后来调整的布局或折叠偏好。
