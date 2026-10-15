# Tauri IPC — 常驻多账号 Runtime（2026-10-14）

生产接口仅接受业务 DTO，不允许前端提交任意 TDLib JSON。类型见 `src/types.ts`，Rust 定义见 `core.rs` / `services.rs` / `app.rs`。所有 ID 为十进制字符串，所有日期为 UTC Unix 秒；指标缺失序列化为 null。

## 命令

Tauri 顶层参数用 camelCase，DTO 内部字段用 snake_case。命令错误返回短错误字符串；TDLib 错误仅显示错误码，避免把请求内容或账号数据放进日志/UI 错误文本。

| 命令 | 参数 | 返回 |
|---|---|---|
| `connection_status` | `{profileId}` | `{profile_id,generation,runtime_state,auth_state,connection_state,detail,user_disconnected}` |
| `account_statuses` | 无 | 全部 Profile 的 Connection 快照；不含 Client ID |
| `select_profile` | `{profileId}` | 只验证正式 Profile，选择在前端当前窗口内生效；不写配置、不改变主账号，不创建、初始化、关闭或注销 Client |
| `connect` | `{config:{profile_id,passphrase:string|null,remember_database:boolean}}` | 解锁指定账号的未初始化 Client；如已 Closed 则重建此 Client；不影响其他 Profile。所有 Client 共用后端全局配置 |
| `auth_submit` | `{profileId,value}` | 按指定账号实际授权状态提交手机号／代码／密码／邮箱 |
| `disconnect` | `{profileId,closeConfirmation?:string|null}` | 只取消此账号查询、close 并等 Closed；超时 Pending 需提交经用户确认的令牌；不注销、不删除目录 |
| `close_guard` | `{profileId:string|null}` | `{pending_count,retry_after_ms,token:string|null}`；null Profile 表示全部账号，检查 Pending 并生成确认挑战，不关闭 Client |
| `unknown_sends` | 无 | `UnknownSend[]`，本进程内各账号经确认关闭的未确认发送记录 |
| `exit_application` | `{closeConfirmation?:string|null}` | 安全关闭全部账号后退出；仍有 Pending 时向主窗口请求确认，关闭失败保留进程 |
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
| `open_settings` | `{profileId:string|null}` | 打开／聚焦唯一 settings 窗口。null 定位应用子页面，正式 ID 定位账号子页面并选择该账号。后端验证正式账号，保存进程内导航目标并向该窗口发送 settings-navigate；不接受 URL 或路径 |
| `settings_target` | 无 | `{tab:"application"\|"account",profile_id:string|null}`；设置窗口先监听导航，再读取最新目标，补偿创建／监听间隙；不写入配置或注册表 |
| `appearance_state` | 无 | `AppearanceSnapshot={appearance,revision,editor}`；返回进程中有效外观（有预览则优先预览），不写盘。新窗口先监听再读取，较新 revision 优先 |
| `preview_appearance` | `{appearance,editor}` | 仅 settings 窗口；校验完整外观及当前 editor，更新内存并同步所有窗口与原生标题栏，不写入配置 |
| `save_appearance` | `{appearance,editor}` | 仅 settings 窗口；“应用主题”调用，成功写入配置后清除预览并广播快照；写入失败保留预览及原保存值 |
| `open_login` | 无 | 打开／聚焦唯一 `login` 窗口；不创建正式账号，也不跳转到设置 |
| `begin_login` | `{config:{phone,passphrase,remember_database,recovery_profile_id:string|null}}` | Profile ID；验证全局 API，分配唯一 ID 和最终隔离目录，创建 PendingLogin / Client 并初始化。只有 Ready 和 getMe 去重成功才正式登记。phone 仅保留在内存到实际 WaitPhoneNumber；recovery ID 仅允许已有待恢复记录，必须复用原目录和原密码 |
| `login_status` | 无 | `(Connection & {phone_submitting:boolean})\|null`；当前 PendingLogin 快照，先监听再读取；自动提交手机号期间仅展示等待，拒绝时允许修正 |
| `login_auth_submit` | `{profileId,generation,authState,code,password}` | 仅当前 PendingLogin Client，校验账号、generation 和实际授权状态。验证码与可选 2FA 密码一次提交，后端按 checkAuthenticationCode → 观察实际状态 → 仅 WaitPassword 时 checkAuthenticationPassword 顺序执行。无 2FA 不发送密码；验证码错误不发送密码；WaitPassword 重试只验证密码。输入仅用于本次内存请求，不持久化 |
| `retry_login_identity` | 无 | Ready 后重试 getMe / 去重 / 原子登记；错误通过 `login-error` 告知，保持 PendingLogin |
| `cancel_login` | 无 | close → Closed → 清理本次新建的独立 Session 和待登录记录；取消旧记录恢复只关闭 Client、保留原数据。绝不 logOut |
| `app_settings` | 无 | `SettingsSnapshot`，仅非敏感字段及 has_saved_* |
| `save_settings` | `{edit:{settings,api_hash:string|null,proxy_password:string|null},closeConfirmation?:string|null}` | Snapshot；秘密 null 保持，安全关闭全部 Client 后统一保存并并发重建运行中的账号，主动断开账号保持断开；不写栏宽、Compact、当前查看账号 |
| `reset_settings` | `{closeConfirmation?:string|null}` | 兼容普通配置重置，未在 UI 提供；保留注册表、主账号、凭据和 Session，安全重建 |
| `clear_secret` | `{key:"api"\|"proxy"\|"db:<profile_id>",closeConfirmation?:string|null}` | Snapshot；全局秘密清除会安全重建全部 Client，proxy 同一持久化事务清空密码与用户名；清除 db 只取消对应保存副本，当前解锁会话不关闭 |
| `logout_account` | `{profileId,confirmed:true}` | 仅指定账号：确认 logOut 的 ok 和 Closed 后清理其私有 Session，再原子删除登记及口令。失败保留登记；本进程保存确认／关闭进度以便重试，不重复 logOut。主账号自动回退，最后一个账号删除后为空 |
| `debug_reset` | `{phrase:"RESET TMSGPICKER",closeConfirmation?:string|null}` | **仅 Debug 注册及编译**。UI 两次确认、逐项说明清理类别；先拒绝新任务、取消查询并等待正在提交的转发，沿用 Pending → Unknown 审阅保护。遍历注册表，包含主动断开账号，必要时用原 Session／口令恢复；全部关联账号 logOut 并确认 Closed，授权有效的 Pending Client 也 logOut，未授权 Pending Client 可 close。仅全部成功后严格清理固定 `%LOCALAPPDATA%/com.funare.tmsgpicker` 全部数据（含 Legacy），完成后退出，重启进入首次启动状态。不接受删除路径。返回 ResetReport；部分失败保留失败账号的登记／Session／凭据并允许重试，已完成注销不重复执行 |

移除 IPC：`add_account`、`rename_account`、`remove_account`、`save_layout`、`save_ui_preferences`、`close_settings`。辅助窗口通过原生关闭按钮关闭；登录窗口先执行安全取消。

`Settings = {schema_version:1,api_id,proxy_enabled,proxy_server,proxy_port,proxy_username}`。旧 `last_profile_id` 只用于升级读取；栏宽和 Compact 忽略且不再写出。

`SettingsSnapshot = {settings,profiles,primary_profile_id,interrupted_profiles,debug_tools,has_saved_api_hash,has_saved_proxy_password}`。
正式 profiles 的 Telegram User ID 必须非空；Profile 字段包括 `profile_id,telegram_user_id,display_name,username,session_directory_id,is_legacy_profile,created_at,last_used_at,has_saved_database_secret,logout_pending,logout_completed`，名称和 username 始终根据最新 getMe 更新。仅在有注销证据且收到真实 Closed 后持久化 logout_completed；重启后只进入注销清理恢复状态，不重建其 Client。旧 logout_pending 仅证明请求曾被接受，不作为完成证据。待登录／升级未识别记录仅出现在 interrupted_profiles，供 LoginWindow 恢复，不进入普通账号列表。accounts.json 分离保存正式 profiles、primary_profile_id 和 pending_profiles。Session ID 由后端生成，业务命令不接受 Client ID 或文件路径。

`ResetReport = {completed,accounts:[{profile_id,display_name,completed,error:string|null}],error:string|null}`。任一账号或最终目录清理失败时 completed=false，设置窗口显示逐账号结果、错误及重试说明。TDLib 成功注销的账号已经不可恢复，即使其他账号失败；请求 ok 不是完成。网络超时保留同一 Client 并继续观察迟到的回执／LoggingOut 和 Closed，不用 close 冒充注销。普通关闭、断开和切换不发起 logOut。

秘密和配置优先级：全局设置／DPAPI 秘密优先，缺少 API 时使用环境变量。原数据库口令手工输入优先于保存值；缺失时 Runtime 保持 NeedsDatabasePassphrase，账号设置输入原密码继续恢复原 Session，不使用空密钥或删除数据库。新口令仅在初始化、Ready、getMe、去重和登记的同一事务中按首次登录选择保存／清除；无正在运行的登录向导时，升级恢复保留原凭据。保存状态 IPC 不解密、不返回秘密。手机号、验证码、2FA 不保存。

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
| `connection` | `{profile_id,generation,runtime_state,auth_state,connection_state,detail,user_disconnected}`；不广播手机号／验证码／邮箱／密码。user_disconnected 仅在用户断开成功后设置，网络更新不改变它 |
| `query-progress` | `{profile_id,generation,result:ScanResult}`；ScanResult 保持 `{run_id,status,scanned,pages,exact,detail,posts}`，status 为 Scanning／Partial／Complete／Cancelled／Error |
| `forward-update` | `{profile_id,generation,target,old_message_id,state:"Sent"|"Failed"|"Unknown"}`；对应发送临时 ID |
| `close-confirmation-required` | 仅发给 main，负载为 CloseChallenge；退出前 Pending 未确认，主窗口提示等待／确认，不自动关闭 |
| `workspace-settings-changed` | `{source:"main"\|"settings"}`；前端发送不含秘密的刷新通知，另一窗口重新通过 app_settings 读取非敏感状态；不作为会话授权依据 |
| `login-completed` | Profile ID，仅发送 main；刷新注册表、选择新账号、加载聊天 |
| `login-error` | 脱敏错误；保持 PendingLogin，以便重试或安全取消 |
| `settings-navigate` | 仅发给 settings，`{tab:"application"\|"account",profile_id:string|null}`；定位子页面及账号，不触发 Client 生命周期或修改主账号；新事件优先于尚未返回的初始目标读取 |
| `appearance-changed` | `AppearanceSnapshot`；所有网页及原生窗口同步。设置窗口销毁时丢弃未应用预览，广播已保存外观并使旧 editor 失效；应用退出时不为回滚重新打开存储。网页串行并合并快速预览请求，拒绝旧 revision，防止迟到快照覆盖最新草稿 |
| `accounts-changed` | 无敏感负载；前端重新读取 app_settings，刷新 getMe 更新后的 Profile |
| `account-notice` | `{profile_id,detail}`；重复 Telegram 身份或元数据保存失败的提示 |
| `app-error` | 安全关闭失败时的短错误；保持进程，允许重试 |

前端先注册监听，再读取 account_statuses 捕获启动期间已经发生的状态变化。所有账号事件写入对应 Profile 缓存；query-progress 必须同时匹配此 Profile 的 generation 和 run_id。后台执行相同验证。新查询只取消同账号旧 run；切换视图不取消任何查询。

`LogicalPost` 包括 key、kind、members、representative、date、text、metrics、provenance、partial。members 是规范化的 TelegramMessage，包含字符串联合 ID、content_kind、caption/text、media_album_id 和快照 metrics。provenance 为字段到 source_message_id 的映射。

ReactionKey 序列化为 `{kind:"emoji"|"custom_emoji"|"paid"|"unknown",value?}`。所有非 Unicode 反应保留类型和计数。反应对象缺失为 null，已知为空为 `[]`。reaction_total 汇总同一个指标来源内的 reaction 计数，绝不跨 Album 成员累加。

`ForwardReport = {items:[{source:{chat_id,message_id},state,detail,sent_id}]}`。预检为 Allowed/Rejected；执行为 Rejected/Failed/Pending/Sent，用户确认关闭后可更新为 Unknown。Pending 和 Unknown 均不代表送达或失败。`UnknownSend = {profile_id,generation,target,old_message_id,state:"Unknown"}`。Unknown 历史独立于查询结果，按 Profile、generation、目标及临时 ID 隔离，断开／重建后仍可查看；同一旧 Client 的明确回执可更新它，新 Client 重用临时 ID 不覆盖它。记录仅保留本进程，退出后不持久化、不自动重发。

## 会话并发约束

AppState 持全局生命周期 RwLock，普通业务持读锁，各账号独立 lifecycle Mutex，不跨账号串行业务。全局重建／退出持写锁。每个 Runtime 独立 epoch、扫描取消标志、结果与 Pending 转发。关闭失败保留此 Gateway 和失败原因，不在原目录强开新 Client。

整个进程只有一个 SharedReceiver。先验证 @client_id 对应存活 ClientSlot，再通过专属 Router 的 @extra 匹配 Pending；两级键等价于 (client_id, request_tag)。Closed 只清理该 Client 的 Pending，全部 Client Closed 后才停止并 join 共享接收线程。重建获得新 Client ID／generation，旧 Client 消息和旧 generation 回调安全丢弃。

Pending 临时消息 ID 按账号隔离，收到 Sent／Failed 后解除。断开、全局设置和退出先调用 close_guard：没有 Pending 无需令牌；有 Pending 时，必须每项等待至少 30 秒，随后由用户明确确认，将尚无回执的发送标为 Unknown，再执行 close。令牌绑定所审阅的 Profile、generation 和临时消息集合，单次使用、5 分钟有效；期间新增 Pending、Runtime 重建或仍在提交转发均拒绝覆盖。取消确认不改变 Client、配置或输入框。移除／注销仍须等待 Pending 最终回执，不能借注销解除阻塞。切换视图始终允许，不调用生命周期 IPC。

请求提交与 close／logOut 共用每个 Client 的提交锁。在发送终止请求前进入 Closing，拒绝后续业务请求，重复 close 不重复发送，logOut 后不盲目追加 close。共享接收器异常退出立即使所有 Router 的 Pending 失败、通知各 Profile Error；不伪造 Closed。重试断开只重启同一旧 Client Map 的接收器以观察真实 Closed，旧 Router 保持封闭，全部旧 Client 安全关闭后才能新建 Client。整个过程仍只有一个 receive loop。

配置／注册表／DPAPI 密文修改在私有暂存对象上完成，三个文件全部持久化成功后才提交内存。多文件替换使用回滚日志；失败恢复旧文件，启动前也先恢复未完成事务。恢复失败明确报错，拒绝继续写入；不静默重置配置。应用退出先拒绝新任务，全部提交 close 后并发等待 Closed；正常退出不调用 logOut。详情见 [Runtime 架构](RUNTIME_ARCHITECTURE.md)。

## 设置与登录辅助窗口生命周期

主窗口为 main；设置为唯一 settings（view=settings），内含应用／账号两个可切换子页面，账号页面可选择已登记账号；登录为唯一 login（view=login）。导航目标仅属于进程内状态，不持久化。窗口 URL 由 Rust 固定生成，创建由 mutex 串行，重复点击解除最小化并聚焦。辅助窗口默认 500 × 580、最小 420 × 440，共享 theme.json 背景及 Windows 原生 DWM 样式。Settings capability 仅覆盖 settings 和 login，提供 core event 访问，不开放任意原生 TDLib 或秘密读取。

设置子页面切换保留未保存的应用输入，显式切换账号时清空原账号临时口令／授权输入；主窗口切换不自动重定向设置。注销后刷新账号列表和添加引导，保持设置窗口。

main CloseRequested 执行安全退出；设置窗口关闭正常销毁，不改变账号、查询或发送状态。LoginWindow CloseRequested 先阻止默认关闭，完成 cancel_pending_login 再 destroy；Destroyed 异常路径也尝试清理，失败广播错误，不退出整个应用。getMe 登记与取消由同一账号生命周期锁串行，已晋升正式账号不会被取消误删。主窗口安全退出关闭全部正式和 Pending Client，保留原有并发关闭、共享 Receiver 和 Pending 发送保护。

三栏比例只保留在主窗口内存。拖动、方向键、Home 仍生效；不发送布局 IPC，不恢复旧比例，不跨窗口同步比例。旧 Compact 字段忽略，不再提供对应 UI 或持久化。
