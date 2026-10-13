# TMsgPicker — 产品与架构设计说明（MVP v0.1）

> **文档用途**：作为仓库中正式开发的产品需求、领域语义、架构边界与验收依据。建议存放为 `docs/PROJECT_SPEC.md`。
>
> **状态**：2026-10-13；设计基线 v0.1。本文将「已验证事实」「既定设计」「暂未验证的假设」明确区分。实现者应以当前工作区和对应 TDLib 版本的实际 API 为准。

## 1. 产品定位与边界

**TMsgPicker 是一个面向 Telegram 消息元数据的轻量级桌面筛选、排序、浏览与处理工具（specialized Telegram client）**。它不是官方客户端的完整替代品，也不是 Bot/Telegram Mini App；不需要重新实现聊天、通知、联系人等完整功能。

核心流程：**登录用户账号 → 定位群组/频道 → 选择扫描范围 → 可视化构造查询 → 扫描历史 → 将原始消息规范化为可查询对象 → 筛选/排序 → 轻量渲染 → 对结果执行合法动作**。

当前验证目标是一个实际频道，但**过滤条件必须抽象为通用消息字段及表达式，而不能把该频道的广告规律写死在业务逻辑中**。

### 1.1 MVP 必须具备

- Windows x64 本地桌面程序：**Tauri 2 + Vue 3 + TypeScript + Rust + TDLib JSON C API（`tdjson`）**。当前仓库使用 Vue，而非此前讨论时设想的 React；**继续使用 Vue**。
- 使用普通 Telegram 用户账号和 TDLib Session；支持至少手机号、验证码、2FA 及已授权 Session 复用。其他授权状态展示明确提示/可恢复错误，必要时再扩展。
- **Chat Picker**：列出本账号可见的群组/频道，并支持按已知聊天搜索；公开频道可按 username 解析。第一版实际查询可限制单个聊天，保留扩展多聊天的领域接口。
- **Scope**：最近 N 条原始消息，或日期范围；有合理默认扫描预算及取消功能。
- **Query Builder**：数字比较、字段缺失检查、结构性条件、AND/OR/NOT，按指标排序及结果条数限制。
- **消息/帖子结果列表**：文本、caption、图片/视频/动画/文件基本类型信息；能显示互动计数与空值；其他类型有 fallback。媒体可按需加载，无需实现完整播放器。
- **轻量动作**：跳转原消息；选择符合权限的消息/媒体组并转发到用户可访问的聊天、私人频道或 Saved Messages。操作前确认、检查权限、展示逐条/逐批结果。
- 扫描进度/部分结果/完整结果的明确状态；查询取消与错误反馈。
- 足够的纯单元测试及真实 TDLib 集成手工验收步骤。

### 1.2 v0.1 不做

- Bot 与 Mini App、Web 后端、SaaS/多用户托管；完整 Telegram 客户端功能。
- 全局全文搜索、Elasticsearch、向量索引、Embedding、LLM 分类、海量持久化镜像数据库。
- 完整聊天消息编辑、发帖、群管理、通话、Secret Chat UI、Story、复杂媒体播放。
- 按时间增长率计算热度（需多时点快照）；复杂热度模型和跨频道归一化排名。
- 自动绕过转发限制、复制受保护内容、批量转发的无确认自动化。

## 2. 已验证事实、观察与未知事项

### 2.1 当前本地 Bring-up 已通过（用户在 Windows 实测）

1. MSVC x64 编译出 `tdjson.dll` / `tdjson.lib`，运行时依赖包括 `libssl-3-x64.dll`、`libcrypto-3-x64.dll`、`z.dll` 和系统/MSVC 运行库。
2. Rust 正常 FFI 链接；`td_execute`、`td_create_client_id`、`td_send`、`td_receive` 可用，客户端可正常关闭。
3. TDLib 通过 SOCKS5 代理连通 Telegram；手机号、验证码、2FA 授权成功；再次启动可以复用本地 Session。
4. 通过 `searchPublicChat` 找到频道，使用 `getChatHistory` 读取频道历史；`view_count`、`forward_count`、`reply_info.reply_count`、具体 emoji reaction 计数均已在真实频道得到验证。
5. **单次 `getChatHistory(limit=N)` 可能只返回 1 条甚至少于 N 条**；使用 `from_message_id` 持续向更早的消息分页、去重后，用户已成功累计 50 条。

### 2.2 特定频道中的观察（不是普遍协议保证）

- 部分广告/推广帖符合 `reply_info == null && media_album_id == 0`。这只是一条**频道专属、可配置、可能误判的启发式规则**。
- 同一个媒体组中，在该频道观察到通常最早的成员携带 comments/reactions，其余成员这些字段为 null；需要以真实 `media_album_id` 验证分组，不能用时间接近作为唯一依据，也不能假定所有频道均遵守该规律。
- 频道的多个媒体组成员可能各自携带略有差异的 views/forwards。**不要将它们直接相加**。

### 2.3 仍需明确验证

- 媒体组在不同频道、跨页/跨 Scope 边界时，成员指标的归属和完整性。
- 非公开聊天、一般 supergroup、论坛 topic 等不同来源的字段可用性与消息链接能力。
- 转发到 Saved Messages、私人频道与受保护频道的权限及结果行为。
- 大范围扫描效率、短页/空页/网络故障/会话断线后的恢复策略。

### 2.4 本地/远端文件差异（很重要）

GitHub `main` 已有 `src-tauri/src/tdlib/{ffi.rs,runtime.rs,mod.rs}` 和带 DLL staging 的 `src-tauri/build.rs`；**但本地多轮测试代码可能尚未提交**。远端 `build.rs` 可见的库目录为 `<repo>/Release`，用户此前描述本地为 `<repo>/Release/Release`。Codex 必须先检查**运行时的工作区**，避免覆盖已通过的本地测试和修改；检测/配置 DLL 路径，不要猜测；绝不向 Git 提交 DLL 或会话数据。

## 3. 推荐架构

```text
Vue 3 / TypeScript (thin UI)
  ├─ Auth / Connection Status
  ├─ Chat Picker
  ├─ Query Builder + Saved Presets (later)
  ├─ Scan Progress / Result List / Simple Renderer
  └─ Actions (Open / Forward)
            │ Tauri invoke + events, typed DTO
            ▼
Rust Application Layer
  ├─ AuthService / ChatService
  ├─ ScanService + QueryPlanner
  ├─ QueryEngine (pure domain logic)
  ├─ PostAssembler (Album grouping)
  └─ MessageActionService
            │
            ▼
TelegramGateway trait / abstraction
            │
      TdlibGateway implementation
         ├─ tdjson FFI (C ABI)
         ├─ Request Router (@extra -> pending request)
         ├─ Dedicated Receive Loop (response/update order)
         ├─ Authorization / Proxy / Session
         └─ History / Message / File / Forward methods
            │
      tdjson.dll → Telegram

TDLib internal local DB → user application data directory
MVP query results → in-memory (optional small preferences/preset store)
```

**层次契约**：Vue 不访问裸 TDLib JSON；`QueryEngine` 不直接依赖网络/TDLib；`TdlibGateway` 不决定“热门帖/广告帖”的业务含义。系统不需要单独部署一个 HTTP 后端。

### 3.1 TDLib Runtime 不变量

- 使用现有现代 JSON ABI：`td_create_client_id`、`td_send`、`td_receive`、`td_execute`；不要改回 legacy `td_json_client_*`。
- **单一专用接收线程**调用 `td_receive`；所有响应及更新按接收顺序解析/分发；`@client_id` 区分 client，`@extra` 关联异步请求。请求超时、取消、错误要清理 pending map。
- FFI 返回的 C 字符串必须在下次可能使其失效的 TDLib 调用之前复制到 Rust-owned 内存；如存在 `td_execute` 并发，必须妥善串行化或约束其生命周期。
- GUI 线程不阻塞在 TDLib receive、网络 I/O 或大量筛选操作上；Tauri commands 暴露异步/后台任务接口并通过 event 推送进度。
- 授权状态机按 `updateAuthorizationState` 驱动，支持 session 重用；`close` 后等待 `authorizationStateClosed`，不要任意退出破坏 TDLib DB 状态。
- SOCKS5 代理可配置（用户当前可用本地代理；**端口不能在生产代码中写死**），在需要网络动作前启用；遇到错误可诊断/重试，但不静默反复请求验证码。

### 3.2 薄网关接口建议

```text
TelegramGateway
  authorization_state / submit_auth_step / configure_proxy
  list_or_load_chats / search_known_chats / resolve_public_chat
  get_chat_history(chat_id, from_message_id, limit <= 100)
  get_message / get_message_properties / get_message_link
  download_or_resolve_media_on_demand
  forward_messages(target_chat, source_chat, message_ids)
  shutdown
```

由测试替身 `MockTelegramGateway` 支撑纯核心层测试；不要在单元测试中连接真实 Telegram，也不要发出真实转发。

## 4. 领域模型与数据来源

MVP 应保留 **RawMessage / NormalizedMessage / LogicalPost** 三个概念，避免过早把所有消息当成“帖子”。

### 4.1 TelegramMessage（归一化后的原始消息）

- `MessageKey { chat_id, message_id }`：必须联合唯一；TDLib `int53` 可用 Rust `i64`，传给 TypeScript DTO 建议使用 **字符串 ID**（避免未来混合精度和跨类型问题）。
- `date`（UTC Unix 秒）、`content_kind`、`text/caption`、轻量媒体引用与 fallback 信息。
- `media_album_id: Option<String>`：TDLib int64 的 JSON 表示为**字符串**；`"0"` 规范化为 `None`（不属于媒体组）。
- `interaction: Option<InteractionMetrics>`，其中 `views: Option<u32>`、`forwards: Option<u32>`、`reply_info: Option<ReplyInfo>`、`reactions: Option<...>`。原始 `null`/字段缺失含义必须保留，不能静默变成 0。
- `ReactionKey = Emoji(String) | CustomEmoji(String) | Paid`；`reaction_count` 应记录 `total_count`，不要因为展示只支持 Unicode Emoji 而丢弃其他类型。
- `forward_info`、`is_channel_post` 等可按需保留；对实际暂未支持的 content type 必须能够安全 fallback。

### 4.2 LogicalPost（用户界面和默认筛选单位）

```text
LogicalPost {
  key: PostKey,                   // 单条 message key 或 chat_id + album_id
  kind: Single | Album,
  members: Vec<MessageKey>,
  representative_message_id,
  metrics: ResolvedMetrics,
  metrics_provenance: { field -> source_message_id },
  completeness: Complete | Partial,
  preview: ...
}
```

**媒体组归并**：同一聊天下拥有相同非零 `media_album_id` 的消息构成候选 Album，不能跨聊天合并；单条消息单独成 Post。默认 Query Unit 是 LogicalPost，未来可添加 raw-message 调试视图。

**指标归属**：优先选有对应非空字段的代表成员（当前测试频道多数是最早的一条）；在字段级记录来源，避免假设一个成员必然拥有全部字段。views/forwards 在多成员都有值时采用**明确且确定的代表策略**（MVP 可取时间/ID 最早且值有效的成员），不得求和；评论和 Reaction 不得累加多个看似重复的媒体组指标。此为应用层显示/筛选约定，不是 Telegram 官方保证的 Album 总指标。

**跨页与边界**：一个 Album 可能被 `getChatHistory` 页边界或 ScanRange 边界切开。组装器应支持增量收集、去重和边界补齐；未能证实完整时标记 `Partial`，不能偷偷宣称已完整扫描所有帖子。MVP 可在扫描范围结束后再统一归并并显示边界风险。

### 4.3 缺失值的严格语义

- `reply_info = null`：回复信息不可用/未返回，不等价于评论数 0。
- `reply_info.reply_count = 0`：回复信息存在，确认为 0。
- `reactions = null` 不等价于所有 emoji count 为 0；**reactions 对象存在但不含某个 emoji**，该 emoji 可按 0 处理。
- `interaction_info = null` 时 views/forwards 也视为未知，不要由 JSON null 默认读成零。
- `IS NULL` / `IS NOT NULL` 是一等查询谓词；任意数字比较遇到未知值不得意外匹配（推荐三值逻辑 `True/False/Unknown`，最终只选择 `True`；NOT(Unknown) 仍为 Unknown）。

## 5. QuerySpec：通用筛选器

### 5.1 四个独立部分

```text
QuerySpec v1
  scope: ScanScope
  unit: LogicalPost (MVP default)
  filter: FilterExpr AST
  sort: SortSpec[]
  output: { limit }
```

- **Scope**：`chat_ids`（MVP 单个）、`last_n_raw_messages` 或 `[from_utc, to_utc)`（半开日期区间），并设扫描预算、手动取消。Scope 决定**要获取哪些数据**，不等于结果数。
- **Filter**：在已取得并归并的数据上求值。
- **Sort**：稳定排序；相同值按 date、chat_id、message_id 等固定 tie-break；升降序时都将未知值放最后。
- **Output**：最终最多展示几条结果；不改变 Scope 的扫描范围。

### 5.2 FilterExpr（建议 AST）

```rust
// Specification sketch; implement as serializable domain types.
enum FilterExpr {
    Compare { field: Field, op: CompareOp, value: Scalar },
    IsNull(Field),
    IsNotNull(Field),
    And(Vec<FilterExpr>),
    Or(Vec<FilterExpr>),
    Not(Box<FilterExpr>),
}
```

首批字段：`views`, `forwards`, `comments`, `reaction_total`, `reaction(emoji)`, `content_kind`, `date`, `reply_info`, `post_kind` / `media_album_id`。数字运算符：`= != > >= < <=`；适当提供日期/数值区间（可映射为 `AND(GE, LT/LE)`）。字段与运算符必须按类型校验，不能把任意用户字符串直接拼成代码/SQL。

**频道专属、通用引擎示例**：

```json
{
  "name": "排除疑似推广（仅当前测试频道有效）",
  "filter": {
    "not": {
      "and": [
        { "is_null": "reply_info" },
        { "compare": { "field": "post_kind", "op": "eq", "value": "single" } }
      ]
    }
  }
}
```

等价的人类可读表达式：

```text
NOT (reply_info IS NULL AND post_kind = Single)
```

原始消息层的等价观察：`NOT (reply_info IS NULL AND media_album_id = 0)`。**这只是可配置的查询 Preset，不应实现为 `is_ad()` 之类的全局推断**。

### 5.3 查询正确性、进度与排序

- `getChatHistory` 每次 `limit <= 100`，结果可能明显小于 limit；**不得因返回短页就判断历史结束**。
- 维护游标和 `seen MessageKey`；使用上一批最旧消息 ID 查询更早历史，排除重复、验证游标向更旧方向前进，保护无进展/循环；错误、真正的终止、用户取消须有不同状态。
- 只有扫描 Scope 完成并完成分组/过滤后，才能将按评论数等全局指标排序的 TopK 标记为 **Exact/Complete**；扫描中可流式展示 `Partial` 候选，不可当成最终排名。
- `last_n_raw_messages` 的 N 指原始消息数量，不是归并后的帖子数；对日期 Scope 应明确边界、日期 UTC 语义和结束状态。
- 动态指标是读取时快照；后续可基于 `updateMessageInteractionInfo` 更新，但 v0.1 不承诺实时全局排名。
- 查询取消后旧 `run_id` 的晚到事件不得覆盖新查询结果。

## 6. UI/交互要求（轻量、可用优先）

单窗口三段式即可：

```text
┌──────────────┬───────────────────────────┬─────────────────────────────┐
│ Chat Picker  │ Query Builder             │ Results                     │
│ Search      │ Scope                     │ Scan progress / status      │
│ Channels    │ AND / OR / NOT conditions │ Sort / summary              │
│ Groups      │ Views / Comments / ...    │ Message cards / album cards │
│             │ Sort + result limit       │ Open / Forward / select     │
└──────────────┴───────────────────────────┴─────────────────────────────┘
```

- 鉴权前展示授权/网络状态；代理与账号异常有可理解的提示。
- 查询表单以**通用类型化字段列表**驱动，初版简单规则组和 NOT 切换即可；AST 支持嵌套以便扩展。
- 默认以 Post 显示，卡片包含标题/频道、日期、文本/caption、媒体类型及简易缩略图、views/forwards/comments、按 emoji 展开的 reactions；未知值显示 `—`，不能冒充 `0`。
- 以明确视觉状态区分 `Scanning / Partial / Complete / Cancelled / Error`；允许重复运行、取消；参数变动不导致自动的无限重新扫描。
- Album 卡片折叠展示成员数及指标代表来源，不重复生成一串看似独立的帖子。
- 非支持类型展示文件类型、基本占位与“在 Telegram 打开”，不应导致整个列表崩溃。
- 不需要 Electron、单独 Node 后端或复杂 UI 组件库。

## 7. 结果管理与转发

- 打开原文：优先使用 TDLib `getMessageProperties` 检查 `can_get_link`，再 `getMessageLink`；媒体组使用 `for_album=true`；链接失败可提示或使用应用支持的其他正常导航方式，**不能自行伪造可访问的私密消息 URL**。
- 转发：临时查询 `getMessageProperties.can_be_forwarded`，尊重内容保护、目标聊天权限；`forwardMessages` 同批**必须源聊天一致、message_ids 严格递增、单批最多 100**；多来源/大批量分组分批。
- 用户必须主动选择目标并确认；先显示预计条数、被拒绝/未知权限项；每批返回中可能有 `null`，必须正确报告部分成功和失败，不可展示虚假的“全部成功”。
- Album 的转发应定义为成员消息的操作；在权限允许时按升序转发全部选定成员，不允许丢失成员或偷偷重传受保护内容。
- Saved Messages 视作用户可访问的目标（需通过 TDLib 正确解析），不硬编码内部 chat ID。
- v0.1 不执行自动批量转发，不实现绕过 Telegram 转发/保存限制的机制。

## 8. 本地数据、安全与可移植性

- TDLib 自有数据库和 Session 应放在用户应用数据目录（Windows 可为 `%LOCALAPPDATA%/com.funare.tmsgpicker/tdlib/`），**不在仓库、`target/`、`Release/` 或前端 localStorage**。
- `api_id`、`api_hash`、Telegram 验证码、2FA、TDLib 数据库加密密钥和授权消息不得进日志、仓库、截图或前端持久化。Bring-up 曾使用环境变量及手工 passphrase，可作为开发启动配置；正式 UI 应以安全输入+受控本地存储/Windows 凭据机制演进。禁止硬编码开发者账号凭据。
- Windows 目前正常动态链接 `tdjson.lib` / `tdjson.dll`，另需 OpenSSL/zlib DLL。保留已成功的原生 FFI 路线；不要未经验证替换成旧版 Rust wrapper、HTTP Bot API 或完整 MTProto 实现。
- 仅提交 Rust/TS 源码、文档和非敏感 fixture。`Release/` 原生二进制、`*.dll/*.lib`（如约定忽略）、TDLib DB、密钥都不提交；不要破坏已有非 TDLib 相关项目文件。
- 原生库目录不能凭空假定：检查实际本地 `<repo>/Release` 或 `<repo>/Release/Release`；优先支持环境变量例如 `TMSGPICKER_TDLIB_DIR`，有明确缺失诊断；云端 Codex 可能没有原生 DLL，纯核心测试应尽可能独立于 TDLib 启动。
- 网络代理需可配置（例如 SOCKS5 host/port），不能假定 Windows 浏览器代理会自动影响 TDLib；连接失败不能反复刷巨量 verbose 日志（默认 Error/Warning）。

## 9. 验收标准与测试矩阵

| 范围 | 必须验证 |
|---|---|
| Native/授权（已有本地实测） | FFI 链接、TDLib client 生命周期、SOCKS5、初次登录、第二次 Session 复用、正常关闭 |
| Pagination | `limit=50` 先返回 1 条，再通过多次请求累计 50 条；去重、单页短返回、空页/无进展、超时/取消 |
| ID & Normalization | TDLib `int53` 与字符串 `int64` 正确解析；不同 content type；null 与 0、具体 emoji 与 Custom/Paid reaction |
| Album | 同一 album 跨页归并；不同 chat album ID 不混合；single 和 album 正确区分；未完成边界不伪装完整；指标不累加 |
| Predicate | `IS NULL` vs `=0`；`AND / OR / NOT`；未知值、条件合法性、reaction(👍) 比较；单频道疑似推广 Preset |
| Query | Scope 与 OutputLimit 独立；排序 tie-break 稳定；NULL 最后；Partial/Complete 标记准确 |
| Frontend | 用户状态、Chat Picker、条件编辑、开始/取消、结果/错误/部分态、基本消息卡片 |
| Actions | 链接权限、转发权限、受保护内容拒绝、批次大小/升序、部分失败及目标选择确认 |
| Security | 源码与日志没有真实 api_hash、验证码、passphrase、Session、私人消息 fixture |

关键自动化测试应通过 **mock gateway + 经过脱敏的 JSON fixture** 完成；实际 Telegram 网络、登录、转发只做手工集成验收并在交付报告中说明是否真的运行。

## 10. 建议实施阶段

1. **A — 稳定 TDLib Runtime**：整理当前测试代码，保留已通过能力，建立 request router、receive loop、auth/proxy、chat/history gateway、可测试的 mock boundary。
2. **B — 领域与查询核心**：归一化、媒体组组装、过滤 AST、稳定排序、分页扫描、完整/部分状态；充分测试。
3. **C — MVP Vue/Tauri 纵向打通**：Auth/Chat Picker/Query Builder/Results；开始、取消、状态/错误 UI；真实历史消息可见。
4. **D — 结果动作**：打开原消息及合法转发；选择目标、确认、权限/部分失败反馈。
5. **E — 后续优化**：Preset 持久化、增量 Metrics 更新、薄 Cache、更多内容类型/多聊天/专用查询优化。

阶段 A–D 构成 v0.1 用户可用 MVP；阶段 E 属于后续迭代。**不要将现有 smoke test 当作正式 Runtime 直接从 UI 同步调用。**

## 11. 官方接口参考

- TDLib JSON C API / ABI: https://github.com/tdlib/td/blob/master/td/telegram/td_json_client.h
- TDLib Getting Started（授权、聊天列表、历史分页）: https://core.telegram.org/tdlib/getting-started
- `getChatHistory`: https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1get_chat_history.html
- `messageInteractionInfo`: https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1message_interaction_info.html
- `message`: https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1message.html
- `messageProperties`: https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1message_properties.html
- `forwardMessages`: https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1forward_messages.html
- `getMessageLink`: https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1get_message_link.html

**维护约定**：任何与 Telegram 协议行为、指标语义、Album 归属有关的新发现，应先归档为「观察/待验证」；验证后再修改「规范性设计」，并同步测试。不要从单个频道的样本推出平台级必然规律。
