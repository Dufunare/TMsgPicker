# TMsgPicker 架构与设计决策

> **基线**：v0.1，依据 2026-10-16 的 `main` 源码整理。本文描述**当前实现**，不是尚待实现的产品规格。日常安装/操作参见仓库 [README](../README.md)，文件与模块细节参见 [projects.md](projects.md)，已经替换的历史方案参见 [Devlog.md](Devlog.md)。

## 1. 目标、边界与关键决策

TMsgPicker 是 **本地优先的专用 Telegram 历史消息客户端**。目标是读取授权用户可访问的聊天、按 Post 单位组合条件筛选和排序、浏览消息及媒体；不是完整 Telegram IM 客户端，也不是 Bot/Mini App，不运营账号代理服务器。

| 决策 | 采用方案 | 原因与取舍 |
|---|---|---|
| Telegram 接入 | TDLib JSON C API，用户账号授权 | 复用 Telegram 的协议/Session 实现，避免重写客户端；引入原生 DLL、Windows 构建依赖 |
| 应用结构 | Tauri 2 + Vue 3/TypeScript + Rust | UI 与 TDLib 业务隔离，界面只通过受控 IPC 访问原生能力 |
| API 凭据 | **BYO API**，用户自行申请 ID/Hash，程序不内置开发者凭据 | 避免在不可受控的公开二进制中传播共享 API Hash |
| 查询单位 | **LogicalPost**：单消息或整个媒体组 | 避免 Album 被底层消息计数和筛选误拆分 |
| 账号模型 | 多个独立 TDLib Client/Session，统一 AccountRegistry | 查看账号与会话生命周期解耦，各账号独立处理查询和网络状态 |
| TDLib 接收 | 进程内**一个共享 `td_receive` 循环** | 遵守 JSON C API 接收语义；按 Client ID/请求 tag 正确路由 |
| 查询范围 | QuerySpec V2：最近 N Post、日期内最近 N Post 或整个时间段 | Post 数在过滤前确定，资源防护额度独立 |
| 大量检索 | 流式组装、增量过滤、Top-K 有界排名 | 限制结果常驻内存和反复排序成本；最终排序与完整排序共享比较器 |
| 界面状态 | 各账号单独保存视图；切换**聊天**则使旧查询失效 | 避免显示旧聊天结果或把已失效结果用于动作 |
| 外观与栏宽 | 外观全局保存；栏宽仅当前窗口内存 | 避免无意义持久化，减少 UI 偏好与 Session 耦合 |
| 发送安全 | 预检、用户确认、发送回执／Pending／Unknown | 不能把“请求提交成功”误报为真正送达 |
| 媒体 | TDLib 文件校验、受限 Base64 IPC、账号级缓存与并发调度 | 较易控制路径权限，但大图仍会产生 IPC/内存成本 |
| 发布 | Windows x64，MIT 许可证 + 固定 TDLib 源码 + NSIS/Portable ZIP + Draft Release CI | 支持可审计的原生依赖构建与自动打包；干净机实测、正式公开发布、签名仍需人工验收 |

## 2. 分层与数据流

```text
┌────────────────────────────────────────────────────────────┐
│ Vue 3 / TypeScript（Tauri WebView）                         │
│ App / Login / Settings · accountViews · QueryPanel          │
│ MessageResultsPanel · 主题 · 媒体调度与前端缓存             │
└─────────────────────────────┬──────────────────────────────┘
                              │ Tauri invoke / events
┌─────────────────────────────▼──────────────────────────────┐
│ Rust app.rs：AppState、命令入口、账号与运行状态              │
│ 输入验证 · run/epoch 隔离 · 生命周期锁 · 窗口导航             │
├───────────────┬────────────────────────┬───────────────────┤
│ settings.rs   │ services.rs / core.rs  │ scan/*            │
│ 配置/注册表   │ 授权、聊天、动作和领域 │ 历史游标 → Post   │
│ DPAPI / 事务  │ 消息归一化/规则/排序   │ → 过滤/Top-K      │
└───────────────┴───────────────┬────────┴───────────────────┘
                                │ TelegramGateway trait
┌───────────────────────────────▼────────────────────────────┐
│ tdlib/gateway.rs：多 Client、共享 Receiver 与 Pending Router│
│ @client_id → ClientSlot → @extra → 对应请求                  │
├───────────────────────────────┬────────────────────────────┤
│ tdlib/ffi.rs（tdjson.dll）    │ TDLib 加密数据库和文件缓存 │
└───────────────────────────────┴────────────────────────────┘
                                │ Telegram 网络
```

**依赖方向**：Vue 不提交任意 TDLib 原始 JSON；所有业务请求通过后端已注册的命令及 DTO。`src/types.ts` 为前端序列化协议，Rust 的 `core.rs`、`services.rs`、`settings.rs` 是领域与配置类型来源。Tauri 顶层命令参数使用 camelCase，业务 DTO 内部字段多为 snake_case。

**线程原则**：网络访问、扫描、媒体读取和可能等待锁的大型查询结果释放放在后台执行；WebView 主线程主要处理响应式状态和视图渲染。后台执行并不等于没有阻塞：单账号内原生同步媒体下载仍受生命周期互斥锁保护。

## 3. 领域模型与 Post 语义

### 3.1 数据模型

- **TelegramMessage**：归一化后的底层消息，包含字符串形式的 `chat_id/message_id`、`date`、`media_album_id`、`content_kind`、文本/实体链接、媒体文件 ID 和互动指标。
- **LogicalPost**：一个独立底层消息（`Single`），或同聊天同非零 `media_album_id` 的成员集合（`Album`）。含成员 `members`、代表消息、Post 日期、文本、指标、来源 `provenance`、成员完整性。
- **QuerySpec V2**：`chat_id`、`scope`、`filter`、`sort`、`output_limit`、`version: 2`。
- **ScanResult**：`status/stop_reason`、`coverage`、`stats`、当前返回 Posts。扫描覆盖和单个 Post 完整性**分开**。
- **AccountProfile / AccountRegistry**：只有成功 Ready + `getMe` 后才成为正式 Profile；未完成的授权/旧版待恢复记录在独立区域。
- **AccountRuntime**：每个正式账号自己的 TDLib Client、epoch、取消标志、当前扫描 run、权威结果、Pending 发送和网络状态。

所有 Telegram 大整数 ID 在跨 Tauri IPC 时表示为**十进制字符串**，避免 JavaScript Number 精度损失。时间戳在后端使用 Unix 秒，UI 输入日期转换到时间戳。指标未知用 `null`，**不能等价于 0**。

### 3.2 Album 的主消息与指标

Post 是扫描范围、过滤、排序、展示、输出上限和动作授权的统一单位；底层消息仍独立存在，不会合并 Telegram 的真实消息 ID。

对于一个 Album：

1. 成员按日期/消息 ID 确定顺序并去重。
2. **主消息**：选择最早携带非空 caption/文本的成员，否则选择排序后的首成员。
3. **评论/回复信息与 reactions**：只取该主消息的对应字段；缺失保持未知。
4. **浏览数和转发数**：分别取所有可用成员中的**最大值**，不求和，不把它解释为去重后的独立用户数。
5. `Post.date`：取成员最晚发送时间 `max(member.date)`；通过时间范围筛选的整个 Post 保留所有已读取成员。
6. 原始成员指标保留，媒体小卡片悬停可单独查看；`provenance` 指出聚合字段的来源消息。

这套“主消息”是 TMsgPicker 的确定性政策，不宣称 Telegram 服务端对所有 Album 都采用同样的显示规则。

### 3.3 三值筛选器

Filter AST 支持：

- `compare`：对浏览、转发、评论/回复、reactions 数量和指定 Emoji 数量执行 `eq/ne/gt/ge/lt/le`。
- `is_null` / `is_not_null`：判断指标有没有有效值。
- `contains_content`：按整个 Post 成员集合判断某类媒体**存在/不存在**，不把媒体类型作为数值比较。
- `and` / `or` / `not`：可嵌套的 Kleene 三值逻辑。

`Unknown` 在 `NOT` 后仍然是 Unknown；只有求值为 `True` 才进入候选集合。某 Emoji 不在**已知的** reactions 列表时是 0，整个 reactions 信息不存在时是 Unknown。后端验证类型、数字输入及条件深度（最大 16）、每组成员数（最多 100）；前端编辑器有更保守的嵌套深度限制。

排序由 `compare_posts()` 提供共同比较规则：升/降序，**null 始终最后**，再使用日期及数值 Chat/Message ID 确保确定性。同样逻辑供全量排序和流式 Top-K 使用。Rust 最多支持 10 个排序项，v0.1 UI 暴露单个排序字段。

## 4. TDLib Runtime 与账号生命周期

### 4.1 Client、Receiver 与请求隔离

`AppState` 的 `TdlibRuntimeManager` 维护 `Profile ID → Arc<AccountRuntime>`。每个 Runtime 拥有自己的 Client/Session/epoch/查询/发送记录；一个进程级共享接收器处理 TDLib JSON C API。

- 每个 Client 请求附 `@extra` tag，并通过 ClientSlot 的 Router 匹配结果。
- TDLib 响应先由 `@client_id` 找到正确的 ClientSlot，再用 `@extra` 找到对应 Pending 请求；两个账号使用相同 tag 也不会串线。
- Receiver 从 C 指针读取消息后先复制到 Rust owned String，再做 JSON 解析与路由；Client Closed 清理对应请求，不清理其他 Client。
- Client 关闭/重建会更新 epoch；旧请求、迟到事件和已经释放的查询结果不能写到新 Client。
- `TelegramGateway`、`SessionLifecycle` 为业务与原生 Client 之间的可模拟测试边界。

TDLib 初始化顺序特别重要：发送 `addProxy`（或 `disableProxy`）与 `setTdlibParameters` 时，**先把请求都提交，再等待结果**；否则初始化前代理操作可能排队，出现人为超时。`request_batch` 也用于聊天详情批量请求。

### 4.2 三种不能混为一谈的操作

| 操作 | Runtime 行为 | Session/账号数据 |
|---|---|---|
| 查看/切换账号 | 前端选择不同 Profile；后端仅验证身份 | 不关闭、不重新登录、不改变主账号 |
| 主动断开或正常退出 | 对应 Client `close` → 等真实 `Closed` | 保留数据库、账号登记、保存的本地数据库密码 |
| 注销账号 | 用户明确确认；执行 `logOut`、验证应答并等待真实 `Closed`，然后清理本账号目录与登记 | **删除该账号本地 Session**；失败保留恢复所需数据 |

启动为注册表中的正式账号准备独立 Runtime；有 DPAPI 保存密码的账号可以恢复 Session，未保存密码的账号进入 `NeedsDatabasePassphrase`，等待**原数据库密码**，不使用空密码/新密码“修复”既有数据库。旧 Legacy 与尚未完成的登录保留为待恢复记录，不生成占位正式账号。

### 4.3 关闭、发送与 Unknown 回执

普通请求提交和 `close/logOut` 使用同一 Client 提交锁：进入 Closing 后拒绝新业务请求；重复 close 幂等，已请求 logOut 后不追加 close 冒充完成。原生 Receiver 故障会令相关 Pending 请求失败，但不能伪造 Closed；需要继续观察同一旧 Client 的真实关闭。

发送与关闭存在竞态：`forward_selected` 预检通过并不保证消息已被送达。发送结果区分 `Rejected`、`Failed`、`Pending`、`Sent`；如果 Pending 至少等待 30 秒仍无明确回执，用户可以确认安全关闭，并将未确定项记为 **Unknown**。Unknown **可能已经送达**；不自动补发。确认令牌绑定账号、generation 和所审阅的 Pending 集合，单次使用、有效期受限。正常关闭不执行 logOut。

这一动作管线目前保留在 Rust 与前端工作区逻辑中，但主结果面板**没有完整公开的批量转发控件**；不能把“后端支持”写成“界面已提供”。

## 5. 查询扫描引擎（QuerySpec V2）

### 5.1 流水线

```text
TDLib getChatHistory / getChatMessageByDate
    ↓
scan/history.rs       有界分页、单聊天数值 ID 去重、游标推进
    ↓
scan/accumulator.rs   跨页 Post/Album 累积
    ↓
scan/boundary.rs      根据成员边界证据判断完整性
    ↓
scan/scope.rs         按 Post 而非底层消息判断范围与停止
    ↓
scan/topk.rs          每个完成的 Post 过滤一次，维护有界 Top-K
    ↓
scan/mod.rs           协调进度/终止、预算、取消及 ScanResult
    ↓
Rust AppState 当前 run → Tauri query-progress → Vue 结果卡片
```

`Scope::Recent {count:N}` 和 `Scope::Dates {from,to,limit_posts:Some(N)|None}` **先选定扫描范围，再执行过滤**；`output_limit` 最后决定保留的最优 K 个 Post，与原始消息数无关。`Dates` 的时间区间为 `[from,to)`，`None` 是尝试遍历整个区间，而非无限制读取所有历史。

日期范围可尝试通过 `getChatMessageByDate(to - 1)` 附近的历史窗口定位；**只有观察到足够的上界证据才信任该起点**，否则退回从聊天最新历史开始向前扫描。此优化并非对任意乱序时间戳的数学证明。

### 5.2 两个独立的完成语义

| 层次 | 值 | 说明 |
|---|---|---|
| **ScanStatus** | Running / Complete / BudgetExceeded / Interrupted / Error | 当前查询是否满足指定扫描范围；可因用户取消、游标停滞、资源耗尽而不完整 |
| **StopReason** | PostLimitReached、DateLowerBoundReached、HistoryExhausted、UserCancelled、CursorStalled、ResourceLimitReached、GatewayFailure | 为什么停止或中断 |
| **ScanCoverage** | requested_posts、examined_posts、scope_complete、date_window_exhausted | 区分“已达到最新 N 个 Post”和“真正扫描完整个日期窗口” |
| **PostCompleteness** | Verified / Assumed / Truncated / Unknown | 某一个 Post 所包含的成员是否有足够证据 |

单消息一般为 `Verified`；Album 即使连续历史与边界都被观察到也通常是 `Assumed`，因为 TDLib 没有公开预期成员总数。`Truncated` 需要已知遗漏证据；中断或不明边界通常为 `Unknown`。扫描 `Complete` 可以同时包含 `Assumed` Post。

### 5.3 资源预算、增量计算与快照

默认内部限制（`scan/mod.rs`）：

| 项目 | 默认 |
|---|---:|
| 原始 Telegram 消息 | 100000 条 |
| 历史分页 | 2500 页 |
| 扫描持续时间 | 300 秒 |
| 开放 Album 边界附加消息额度 | 64 条 |
| QuerySpec 输出上限 | 1000 Post |
| 扫描中一次进度事件的候选 Post | 最多 40 个 |

扫描过程中使用有界 `TopPosts` 堆，优先处理/过滤完成的 Post，不在每次进度时重新对全部历史排序。约每 500ms 发布进度快照；最终输出可以包含全部 K 个已排名 Post。状态统计区分已读取原始消息、范围内 Post、匹配数、实际返回数。预算耗尽不会伪装成 Complete。

账号 Runtime 保留当前 run 的权威结果供后端校验；聊天切换使前端结果立即失效并发送 `release_query`，后台取消匹配的 run 且释放旧排名。`run_id + profile_id + generation + 当前 chat_id` 共同防止跨账号/跨聊天污染；过期事件不覆盖新页面。

## 6. 媒体读取、缓存与 UI 性能

媒体文件经 `ChatService::media_data` 使用 TDLib `downloadFile` 获取，只允许读取**当前账号 Session 内**经 canonicalize 和目录边界验证的文件；不会向 WebView 暴露用户可传入的任意本地文件路径。读取前检测大小，并识别允许的内容魔数；当前通过 Base64 Data URL 传输。

- 文件安全上限：缩略图约 2 MiB，原图约 24 MiB。
- 展开 Album 时先显示微型图/缩略图，自动请求照片原图。视频封面并不等同于完整视频下载/播放。
- `mediaScheduler.ts` 限制同时执行的下载：**全局最多 2、每账号最多 1**；原图请求排在普通缩略图/头像任务前；同文件多订阅者共享请求、可单独撤销待处理请求。
- 缓存按 Profile、generation、file ID 与尺寸变体隔离。前端缩略图 LRU 最多 128 项、12 MiB 字符预算，原图 LRU 最多 8 项、64 MiB 字符预算；还有 TTL、单项大小限制；头像另有自己的预算。退出/会话换代清除应用级缓存。
- 已提交给 TDLib 的**同步下载不能靠前端 abort 立即终止**；主动收起可以取消等待者/尚未开始的任务，下载结束后依照有效性决定是否缓存。
- 结果面板通过每页 40 张 Post 卡片控制挂载 DOM，翻页回到中间栏顶部，动画服从“启用界面动画”和系统减少动态效果选项。
- Vue 将不可变的扫描快照作为 `markRaw` 使用，减少不必要的深层响应式代理。

**权衡**：同步 TDLib 文件下载和 Base64 IPC 仍可能产生额外等待、内存复制与磁盘文件；原生文件缓存归 TDLib 管理，不能任意删除运行中的 Session 文件。当前还没有经过真实 WebView 大规模压测的固定 FPS/RSS 保证。

## 7. 本地持久化与配置事务

本地根目录：`%LOCALAPPDATA%\com.funare.tmsgpicker\`。

```text
app-config.json           全局 API ID、SOCKS5 非敏感字段、外观
accounts.json             AccountRegistry，正式/待恢复账号、主账号
credential-store.json     当前 Windows 用户范围 DPAPI 加密的秘密
tdlib/accounts/<Profile ID>/{db,files}/  每个账号独立 Session
tdlib/{db,files}/         旧版本 Legacy 数据（按需保留）
```

`settings.rs` 将配置、注册表、DPAPI 凭据分开，并通过暂存、写入 flush、Windows 文件替换和回滚日志协调一致性。旧 `settings.json` 等数据仅按明确迁移路径处理；损坏或未知未来版本不能静默清空。读取 IPC 快照不解密 Hash/代理密码/数据库密码。手机号、验证码、邮箱验证码、Telegram 2FA 仅参与当次授权，不写入应用配置。

`managed_data.rs` 为破坏性操作建立**固定所有权路径**与 Windows Reparse Point/符号链接/越界防护。注销只清理已验证属于目标账号的文件；Debug 全部重置受 `cfg(debug_assertions)` 限制，必须首先成功处理各账号注销及真实 Closed，不能由前端任意指定删除目录。

## 8. Tauri IPC 和窗口边界

### 8.1 IPC 的最小权限

生产 UI 只能调用预先注册的命令，**不能透传任意 TDLib JSON 请求、任意文件路径或 Client ID**。按职责分组：

| 类别 | 代表命令 / 事件 | 保护条件 |
|---|---|---|
| 账号状态 | `account_statuses`、`connection_status`、`connection` | 指定 Profile，事件带 generation |
| 登录 | `open_login`、`begin_login`、`login_auth_submit`、`cancel_login` | PendingLogin 身份与授权状态校验，Ready + getMe 才登记 |
| 全局设置 | `app_settings`、`save_settings`、`clear_secret` | DPAPI 秘密不回填；全局配置更新安全关闭/重建 Client |
| 外观与导航 | `open_settings`、`settings_target`、`preview_appearance`、`save_appearance`、`appearance-changed` | 设置窗口作用域、revision / editor 防迟到覆盖 |
| 聊天与扫描 | `chats`、`public_chat`、`start_query`、`cancel_query`、`release_query`、`query-progress` | Profile + chat + run + epoch 校验 |
| 原文与发送 | `message_link`、`preview_forward`、`forward_selected`、`forward-update` | TDLib 权限检查、Post Key/run 和用户确认、Pending 回执 |
| 生命周期 | `disconnect`、`logout_account`、`close_guard`、`exit_application` | 真正 Closed、Pending/Unknown 保护 |
| Debug | `debug_reset` | Debug-only、固定短语/两次确认、所有权目录校验 |

其中 `selection()` 只允许来自**当前有效后端查询结果**的 Post Key，并对 `Unknown/Truncated` Post 设置额外确认要求；即使前端构造恶意参数，也不能用任意历史 ID 调用批量转发。

### 8.2 窗口与主题

入口按 `window.location.search` 的固定 `view` 区分主窗口、设置、登录；设置和登录各限制为**唯一辅助窗口**，重复打开会聚焦已有窗口。登录窗口关闭时先安全取消 PendingLogin；设置窗口关闭不触碰 Session。

三栏由 CSS Grid、可拖动/键盘操作 Splitter 组成；宽度不持久化，小窗口右侧查询变抽屉。`SettingsPage.vue` 的**应用 / 账号 / 外观**子页面共用一个设置窗口，各自职责明确；当前操作的账号 ID 独立于主窗口的查看账号，避免误对其他账号执行注销。

`AppearanceSession` 和 `appearanceClient.ts` 使用预览 revision 及 editor 来处理跨窗口竞态。更改主题先广播预览，只有**应用主题**才写配置；关闭未保存的设置恢复之前的主题。`src/theme.json` 共享背景 Token，`window_theme.rs` 在 Windows 能支持的系统上同步 DWM 原生标题栏，不能支持时降级。

## 9. 测试策略与剩余边界

- **纯领域/Mock**：`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib --tests` 验证 Post、三值过滤、日期、扫描游标、Top-K、账号/发送生命周期模拟。
- **前端/TS**：`pnpm.cmd test`（Node 内存渲染器与模拟 IPC），`pnpm.cmd build`（类型检查/Vite）。
- **Windows 原生**：带默认 desktop feature 的 Cargo 测试/检查及本地 `pnpm.cmd tauri dev`；原生路径依赖 TDLib DLL，测试场景与实际账号登录需区分。
- **GitHub CI**：前端构建/测试与 Windows 离线 Rust 回归；当前 CI 不是正式发行构建和真实 Telegram 端到端自动化。

**明确未解决或未完整覆盖的范围**：跨平台桌面分发、正式安装器及运行库封装、账号 API 凭据的默认分发（刻意不提供）、全部聊天实时索引、超大结果的原生分页提取、TDLib 磁盘缓存治理、Windows GUI 实测 FPS/内存曲线、批量转发图形入口，以及崩溃/强杀后尚未最终确认发送的跨进程回执恢复。

下一步开发新功能时，优先保持三条不变量：**Post 是查询单位；账号+generation+run 所有权不能跨界；关闭/注销和未知发送必须以真实证据而非猜测驱动。**

## 10. v0.1 Windows 原生依赖与 Release 决策

**来源固定**：TDLib 1.8.68，固定至 2026-10-12 16:43 UTC 提交 `f381f4b35ebfd52a086ec3af5352ed77bbdbde01`；配套 vcpkg commit `e456309491fd875487bf02b8a0dca76c1b1b00cc`（OpenSSL 3.6.5#1、zlib 1.3.2#2、构建工具 gperf 3.3#1）。源码和 native 输出放在 Git 忽略的 `native/`，项目代码不直接 vendoring TDLib，`scripts/native-versions.json` 是唯一版本锁定数据源。历史手动构建 DLL 的来源并未自动验证。

**部署模型**：`tdjson.lib` 用于 MSVC 编译链接；最终 `tdjson.dll`、OpenSSL/zlib 的**真实 PE 导入名称** DLL 需位于 `tmsgpicker.exe` 同目录。普通 `src-tauri/build.rs` 只负责 Cargo 开发期链接/复制；Release 脚本独立生成 Tauri 配置 overlay，将已验证哈希的 DLL 和版权声明映射到 NSIS 安装目标目录。Portable ZIP 也包含运行 DLL 和 MSVC CRT；WebView2 通过安装器下载引导或由用户系统提供。

**CI 安全界限**：Pull Request 只静态解析 PowerShell 和执行已有离线检查；手动 workflow_dispatch 可构建发行资产但不创建公开 Release。只有匹配 Tauri 版本的 `vX.Y.Z` tag 才启动 Draft/Prerelease 生成，随后必须由人完成干净 Windows x64 安装、升级/卸载、Session 完整性和版权审查。发行构建**没有 Telegram 开发者 API Hash 变量注入**，用户始终自带 API ID/Hash。

**可复现级别**：锁定 TDLib/vcpkg 源码提交与 port 版本并记录构建文件 SHA-256；目前没有完全锁定 MSVC/CMake/Windows runner 补丁版本，因此不声称产物可逐字节重复。许可证、签名与发布审核详见 [RELEASE.md](RELEASE.md)，构建步骤详见 [BUILD_WINDOWS.md](BUILD_WINDOWS.md)。
