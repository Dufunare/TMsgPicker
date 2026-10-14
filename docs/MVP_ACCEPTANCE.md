# v0.1 实现与验收记录

## 2026-10-14 P2 稳定性修复

基于用户已合并 UI 微调后的本地 `main`，开工工作区干净。本轮修复四项阻塞／一致性问题，保留三栏、独立 Settings 和用户最新 UI；仅增加 Unknown 历史展示，并移除一个已不使用、导致 TypeScript 构建报错的 closeSettings 绑定。

- Pending 转发每项等待至少 30 秒后，断开／全局设置／退出可由用户明确确认安全关闭，结果为 Unknown。确认令牌绑定 Profile、generation 和消息集合，单次使用、5 分钟有效，新增 Pending 或换代必须重新审阅。取消保留客户端和未保存输入；记录在本进程断开／重建后可查，退出后不持久化。移除／注销仍不绕过 Pending，普通关闭不调用 logOut。
- Gateway 在提交 close／logOut 前进入 Closing；请求登记、发送、关闭使用同一 Client 提交锁，后续请求被拒绝，重复 close 幂等。
- Store 修改先暂存，配置／注册表／DPAPI 密文三文件以回滚日志提交，成功后发布内存。失败恢复旧值，启动先恢复未完成事务；清除代理密码与用户名为同一事务。
- 接收线程 Rust panic／异常返回使全部 Pending 立即失败、逐账号通知错误。重试断开仅恢复旧 Client 的接收和安全关闭，不允许新业务或重开 Session；全部真实 Closed 后才可新建 Client，始终保持单 receive loop。不覆盖原生访问违规／进程 abort 恢复。

| 验证 | 结果 |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` | 33 项通过：12 项持久化／账号 + 21 项业务回归 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib --tests --target-dir src-tauri/target/runtime-check` | 49 项通过：28 项 unit／原生 Runtime + 21 项业务回归 |
| `pnpm.cmd test` | 6 项通过，Vue 内存渲染器及模拟 IPC |
| `pnpm.cmd build`（Tauri beforeBuildCommand） | TypeScript 与 Vite 生产构建通过 |
| `pnpm.cmd tauri build --debug --no-bundle`，设置 `CARGO_TARGET_DIR` | Windows 桌面构建通过，`src-tauri/target/runtime-check/debug/tmsgpicker.exe`，DLL 已 staging |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `cargo check --manifest-path src-tauri/Cargo.toml --all-targets --target-dir src-tauri/target/runtime-check` | 通过，包含原 CLI 工具 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --target-dir src-tauri/target/runtime-check -- -D warnings` | 通过，无警告 |
| `git diff --check` | 通过 |

49 项 Rust 与 6 项前端共 55 项独立测试；离线 33 项为 Rust 的子集。新增覆盖真实并发提交／close 边界、多个 Client 的 Pending 故障传播、故障通知回调 panic、原生接收回调 panic 后仅关闭恢复、超时确认及重放／换代／新 Pending 拒绝、整组持久化各阶段失败及实际 IO 失败、未完成日志恢复、代理凭据清除事务、Unknown 账号隔离与重建保留、取消／等待时不关闭及保留编辑中的秘密。

测试仅使用 fixture、临时目录、mock 和未授权原生 Client；没有真实 Telegram 凭据、真实消息发送或真实 Session 读取。没有删除现有数据库／Session／凭据，没有结束用户运行的应用，没有提交、推送或合并。真实断网长 Pending、Windows WebView2 确认窗口和已登录 Session 的关闭／恢复仍需按 [补充手动验收](RUNTIME_ARCHITECTURE.md#2026-10-14-p2-修复补充验收) 检查；未制作安装包。

## 2026-10-14 常驻多账号 Runtime 重构

本轮以本地 `review/stability-final-pass-20261013` 分支和干净工作区为基础，保留最新 UI。此前文档中的单活动账号／串行 Session 切换为历史行为，现行结构、状态机、最终数据模型、调用流程和手动验收以 [Runtime 架构与验收](RUNTIME_ARCHITECTURE.md) 为准，接口见 [IPC](IPC.md)。

已实现：唯一 ApplicationConfigService、持久化 AccountRegistry、Profile 级 DPAPI 数据库口令、非破坏旧设置迁移、Legacy 原目录复用、进程共享接收器、Client 专属 Router、全部注册账号启动、未解锁 Client 保留、账号级初始化失败、重复身份冲突处理、并发安全关闭、全局配置统一重建、显式 Profile IPC 和事件、按账号缓存前端状态。切换视图不调用任何 TDLib 生命周期操作，后台查询继续，普通关闭仅 close，显式确认注销才 logOut。

主要文件：`settings.rs`、`app.rs`、`tdlib/gateway.rs`、`gateway.rs`、`services.rs`、`useWorkspace.ts`、新增 `accountViews.ts`、账号／设置／聊天组件、`types.ts`、Rust 回归和新增 `tests/workspace.test.ts`。`core.rs` 和两个 bring-up CLI 保持原业务实现。

| 验证 | 结果 |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` | 29 项通过：8 项配置／账号测试 + 21 项业务／生命周期 mock 回归 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib --tests --target-dir src-tauri/target/runtime-check` | 39 项通过：18 项 unit／原生 Runtime + 21 项集成回归；不使用真实 Session 或凭据 |
| `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` | 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | 通过 |
| `pnpm.cmd test` | 3 项前端测试通过，使用 Vue 内存渲染器和模拟 IPC |
| `pnpm.cmd build` | TypeScript／Vite 生产构建通过 |
| `pnpm.cmd tauri build --debug --no-bundle`，设置 CARGO_TARGET_DIR | 完整 Windows 桌面构建通过，产物为 `src-tauri/target/runtime-check/debug/tmsgpicker.exe`，依赖 DLL 已 staging |
| `git diff --check` | 通过 |

新增测试覆盖：多 Client 和唯一 receive loop、相同 tag 的隔离、账号更新顺序、单 Client 超时／关闭／Pending 失败、未知和已关闭 Client 迟到消息、Client 重建和并发关闭、授权／网络／Locked／Error 分离、旧 epoch／run 隔离、空注册表、不保存口令不创建数据库、独立 DPAPI 解密及局部失败、旧配置／Legacy 文件保留、重复身份不覆盖、close 与 logOut 的独立调用序列、A → B → A 只调用 select_profile、后台查询／发送隔离、授权无自动弹窗、全局重建 generation 的独立应用。39 项 Rust 与 3 项前端共 42 项独立测试；离线模式的 29 项属于其中的子集。

原默认输出目录的 tmsgpicker.exe 正在运行，首次 `--lib --tests` 因 Windows 文件占用无法覆盖主程序。使用独立 target 目录完成完整测试与构建，没有结束现有进程或操作真实 appdata。开发过程中未删除现有数据库／Session／凭据，未推送或合并 main。

人工未验证：真实多账号授权、SOCKS5 和网络故障、旧加密 Session 的实际复用、真实 getMe 冲突、真实消息发送及 Pending 更新、WebView2 视觉和窗口交互。请先正常关闭旧程序，再运行新产物，按 [本地手动验收步骤](RUNTIME_ARCHITECTURE.md#自动化验证与人工验收) 验证。没有制作或验证安装包。

日期：2026-10-13。设计依据：[项目规范](TMsgPicker_PROJECT_SPEC.md) 第 9 节，加用户正式任务附件的补充要求。

**状态：A–D 的应用代码已实现，离线核心及 Windows 原生生命周期通过自动化验证；真实账号端到端手工验收尚未执行。不能把编译成功视为真实登录/网络/转发已验收。**

## 工作区保护

开工 `git status --short` 为空；设计文档实际文件名为 `docs/TMsgPicker_PROJECT_SPEC.md`。添加 `PROJECT_SPEC.md` 入口链接，保留原文。检查了原有 Vue/Tauri、FFI、build.rs、smoke/auth 两个 CLI 工具及实际 `Release/` 文件。保留两个 CLI 的原实现；未清理 DLL、数据库或 Session，未提交或 push。

## 主要文件与职责

| 文件 | 职责 |
|---|---|
| `src-tauri/src/tdlib/gateway.rs` | dedicated receive、客户端/请求关联、并发 pending、错误脱敏、45s 请求超时、取消清理、close/Closed、唯一接收器约束 |
| `src-tauri/src/gateway.rs` | TelegramGateway 边界与字符串 ID 输入校验 |
| `src-tauri/src/services.rs` | AuthService、ChatService、链接与转发动作/预检；复用现有 TDLib 参数及数据位置 |
| `src-tauri/src/core.rs` | 归一化、反应、Album 组装、三值谓词、版本化 QuerySpec、排序、分页扫描/状态 |
| `src-tauri/src/app.rs` | Tauri commands、后台任务、脱敏事件、run 隔离、选中结果验证、安全退出 |
| `src/types.ts` / `src/components/FilterEditor.vue` / `src/App.vue` | 类型 DTO、递归 AND/OR/NOT 编辑器、真实 IPC 连接的授权/聊天/查询/卡片/动作界面 |
| `src-tauri/tests/fixtures/messages.json` / `core_regression.rs` | 人工合成数据与 MockTelegramGateway 回归；没有真实 Telegram 数据 |
| `src-tauri/build.rs` / `Cargo.toml` | 可配置开发期 DLL staging；desktop feature 与离线 core 测试入口 |
| `tauri.conf.json` / `.gitignore` | 可用窗口尺寸、CSP、二进制和数据库排除规则 |
| `README.md` / `docs/IPC.md` | 中文运行说明、限制、命令/事件/DTO 文档及手工验收流程 |

## 对照验收矩阵

| 规范范围 | 已实现 / 自动验证 | 仍待本机手工验收 |
|---|---|---|
| Native / 授权 | 原生 FFI ABI、唯一接收器、并发路由、取消/超时清理、错误脱敏、Closed 后停止并可重建；手机号/代码/2FA/邮箱 UI；代理请求先发送，参数不等待代理响应便提交，再等待两者确认 | 真实 API 初始化、SOCKS5 连通、第一次账号登录、已有加密 Session 复用及 GUI 退出 |
| Pagination | 先 1 后 49 累计 50、短页继续、重复游标、空页、无进展 Partial、错误/429、取消、最后一条预算游标包含问题 | 当前账号真实历史的短页/断网表现 |
| ID / Normalization | 所有 ID 为字符串 DTO；int64 字符串 Album/custom emoji；文本/caption、媒体类型占位和未知内容 fallback；null / 0、具体 Emoji/Custom/Paid | 多个真实消息类型的展示 |
| Album | 跨页/跨 chat 正确分组；最早有效成员逐字段来源、计数不累加、成员完整性未知显式 Partial | 真实频道的字段来源和扫描边界核对 |
| Predicate | 比较 6 种操作、IS NULL/NOT NULL、嵌套 AND/OR/NOT、Kleene Unknown、类型校验、可编辑频道专属推广预设 | 用户频道中预设适用性与误判 |
| Query | Scope 与 OutputLimit 独立；日期半开范围/预算；升降序 null 最后、多字段/数字 ID tie-break；Partial/Complete/Cancelled/Error；run ID 隔离测试 | GUI 快速取消后新查询不被旧事件覆盖 |
| Frontend | Vue TypeScript/生产构建；真实 invoke/events；授权、聊天选择/搜索/username、递归条件、执行/取消、错误/空态、LogicalPost 卡片 | WebView2 中实际交互与布局、真实数据显示；未执行自动化浏览器/GUI 测试 |
| Actions | 链接能力 + TDLib 链接、Album 标志；Saved Messages 解析；预检确认/再检查；目标频道/群成员/媒体权限；源 chat 分批升序 ≤100；受保护拒绝；null/省略/异常返回/批次错误；Pending 与发送更新 | 真实私密链接、真实合法转发及发送更新。自动化未发送消息 |
| Security | 新服务不记录凭据或消息日志，不写浏览器 storage；本机 appdata DB，复用既有加密密钥；仅人工 fixture；二进制/数据库排除 | 真实账户手工验收时保持本机凭据与 Session，不分享原始数据或旧 CLI 输出 |

## 自动化验证

已执行：

- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`：18 项 mock/core/service 回归通过；无 DLL 或 Telegram 网络依赖。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib --tests`：4 项路由/原生生命周期/旧 run 隔离测试与 18 项核心回归通过，合计 22 项。原生测试仅创建未配置参数的客户端，读取授权状态与 version，不使用凭据、Session 目录或真实账号。
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`：通过，包含原 CLI 工具。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`：通过；初次发现的折叠 if 警告已修复。
- `pnpm.cmd build`：Vue TypeScript 检查与 Vite 生产构建通过。

- `pnpm.cmd tauri build --debug --no-bundle`：完整 Windows 桌面程序构建通过，产物为 `src-tauri/target/debug/tmsgpicker.exe`，依赖 DLL 已暂存到同目录；没有制作安装包。
- `git diff --check`：通过。

未执行真实账号授权/历史读取/转发；未制作或验证安装包。当前没有可调用的 Browser 控制工具，也未安装独立 Playwright，因此没有宣称 GUI 自动化通过。

## 关键决策与具体限制

1. **Album**：以 chat + 非零 album ID 分组；最早有效字段分别取值并保留来源。没有总成员数证据，所有 Album 的成员完整性保守标 Partial，含 Album 的最终查询不标 Exact；手动转发仅覆盖卡片列出的成员。
2. **分页**：请求 limit ≤100；后续请求为包含游标预留一位。短页继续，空页为历史结束，cursor 不变/向新方向为 Partial，错误或取消保留候选。日期 budget 包括定位区间的更新消息，不被 output_limit 截断。
3. **null**：unknown 不当成 0；只匹配谓词 True。反应对象存在且 Emoji 缺项为已知 0，整个对象缺失为 unknown。
4. **Session/代理**：沿用已验证 CLI 的本地 appdata 路径和 Base64 bytes 参数。每次启动输入数据库密码，不存前端或仓库。先发送代理请求，再提交 setTdlibParameters，两者都发送后等待响应；没有配置时禁用旧代理。原先“先等待代理成功再提交参数”的实现已修正，避免 TDLib 排队造成相互等待。
5. **动作**：确认前展示允许/拒绝项，提交时复核；只正常转发，Pending 不冒充送达，关注 TDLib 发送成功/失败更新。
6. **后续项目**：媒体缩略图/播放器、聊天列表加载更多、发送报告持久化、自动重试、指标实时全局重排、MSI/NSIS 分发均未实现。首版媒体类型占位、最多 100 项聊天、单聊天/内存扫描满足当前范围；界面编辑一个排序字段，核心接受最多 10 字段。

本机最短验收流程详见 [README](../README.md) 的“检查与手动验收”。用户自行确认真实发送后再验收转发；本次开发没有替用户执行该动作。

TDLib 方法参数核对来源为 [官方 td_api.tl](https://github.com/tdlib/td/blob/master/td/generate/scheme/td_api.tl)，结合本地已验证 harness 的新式 addProxy 形状；本地运行时仍应以实际 DLL 的行为为准。

## 初始化超时修复（2026-10-13）

用户实测 addProxy / disableProxy 均超时。原因是原服务使用阻塞 request 等待代理确认，而 TDLib 在参数缺失阶段把代理方法放入 pending_preauthentication_requests；参数请求因此永远不会发送。CLI 原实现先发送代理、继续处理授权状态并发送参数，没有此等待环。

修复为按序提交两条请求的 request_batch，随后先等待参数响应，再等待代理确认；参数错误或取消时通过 PendingRequest 的 Drop 清理其余路由槽位。原生回归使用本地 DLL 重现两种代理 API 的预初始化排队，并提交故意无效的 `api_id=0` 参数，确认立即报告参数错误而非等待代理超时；不提供真实凭据、不打开 Session 或发送消息。Mock 回归强制禁止在 bootstrap 中使用单条阻塞 request，覆盖启用代理、停用代理及失败。

TDLib 状态分类和排队逻辑参见 [官方 Td.cpp](https://github.com/tdlib/td/blob/master/td/telegram/Td.cpp)：preauthentication 与 preinitialization 是不同阶段。真实 Clash 连通性仍须本机配置正确 SOCKS5/Mixed 端口后观察连接状态。

## 三栏与多账号后续迭代

2026-10-13：已完成设置、DPAPI、Profile/Legacy、串行会话切换与三栏 UI。新的交付与验收记录见 [UI_ACCOUNTS_ACCEPTANCE.md](UI_ACCOUNTS_ACCEPTANCE.md)；本页原有 v0.1 结果是历史记录，新的验证状态以该文档为准。
