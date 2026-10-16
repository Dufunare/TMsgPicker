# TMsgPicker 项目功能、模块与源码导览

> **文档定位**：帮助开发者快速找到“某项功能在哪个文件实现、通过什么接口交互、修改时需要同步检查哪些测试”。依据 2026-10-16 的 `main` 整理。架构原则见 [ARCHITECTURE.md](ARCHITECTURE.md)；终端用户操作见 [README](../README.md)；迭代历史见 [Devlog.md](Devlog.md)。

## 1. 功能地图与当前实现边界

| 功能域 | 当前可操作行为 | 关键实现位置 | 说明 |
|---|---|---|---|
| Telegram 接入 | 多账号登录、验证码/2FA、已有 Session 恢复 | `login.rs`、`services.rs`、`tdlib/gateway.rs`、`LoginWizard.vue` | 账号授权由 TDLib 驱动，应用不代替 Telegram 保存密码 |
| 全局配置 | 自带 API ID/Hash、SOCKS5、代理认证 | `settings.rs`、`useApplicationSettings.ts` | API/代理为全局，不按账号分别设置 |
| 多账号 | 添加、查看切换、主动断开/恢复、注销 | `app.rs`、`accountViews.ts`、`AccountPanel.vue`、`AccountSettings.vue` | Client 常驻、视图切换不重新登录 |
| 聊天定位 | 列表、名称搜索、全部/普通/归档、公开 username | `ChatService`、`ChatPanel.vue`、`useWorkspace.ts` | 依赖有限范围的 TDLib 聊天列表 |
| 扫描历史 | 最近 N Post、日期内最新 N Post、整个日期范围 | `scan/*`、`core.rs`、`QueryPanel.vue` | 有资源限制和显式扫描结果状态 |
| 谓词过滤 | 指标比较、空值、媒体类型存在性、AND/OR/NOT | `core.rs`、`FilterEditor.vue` | 三值逻辑，Post 级判定 |
| 排序 | 浏览/转发/评论/反应/Emoji/日期、升降序 | `core.rs`、`scan/topk.rs` | UI 单字段，后端最多 10 字段 |
| 结果浏览 | 40 条一页、状态/统计、展开媒体组、链接 | `MessageResultsPanel.vue`、`LinkedText.vue` | 当前结果属于所选账号与聊天 |
| 图片/视频预览 | 缩略图、照片自动原图、视频封面、悬停成员指标 | `MediaPreview.vue`、`mediaScheduler.ts`、`services.rs` | 同步 TDLib 下载，受文件限额和缓存限额约束 |
| 原消息链接 | TDLib 权限检查后打开链接 | `useWorkspace.ts`、`ActionService`、`message_link` | 私有消息不保证具有可打开的 URL |
| 转发预检与回执 | Rust 服务已实现转发权限检查、Pending/Unknown 机制 | `services.rs`、`app.rs`、`useWorkspace.ts` | **当前主结果面板未开放完整的批量选择/转发 UI** |
| 外观 | 主题、颜色、密度、字号、玻璃参数、动画 | `appearance.ts`、`AppearanceSettings.vue` | 实时预览，按“应用主题”才保存 |
| 设置导航 | 唯一设置窗、账号目标隔离、独立登录窗 | `SettingsPage.vue`、`settings_target.rs`、`useSettingsNavigation.ts` | 设置操作对象不随主窗口切换而静默改变 |
| 安全退出/清理 | 关闭、注销、Pending 防重发、Debug 全部重置 | `app.rs`、`managed_data.rs`、`reset.rs` | Debug 重置不能当作普通清缓存操作 |

## 2. 仓库结构

```text
TMsgPicker/
├── README.md                      用户上手、构建与操作手册
├── docs/
│   ├── ARCHITECTURE.md             架构设计及现行决策
│   ├── projects.md                 本文件：功能/模块/源码索引
│   └── Devlog.md                   时间线与历史变更
├── src/                            Vue / TS 前端与样式
│   ├── components/
│   ├── media/
│   ├── scan/
│   ├── theme/
│   ├── text/
│   └── window/
├── src-tauri/                      Rust / TDLib / Tauri
│   ├── src/
│   │   ├── scan/
│   │   ├── tdlib/
│   │   └── bin/
│   ├── tests/
│   ├── capabilities/
│   ├── build.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── tests/                          前端 Node / Mock 回归
├── .github/workflows/
│   └── frontend-ui.yml
├── package.json
├── pnpm-lock.yaml
└── vite.config.ts
```

下列路径均相对于仓库根目录，**大小写按当前 Git 中实际文件名书写**。历史文档里出现过的 `src/base.css`、`docs/IPC.md` 等旧路径不再作为本版本维护的文档入口。

## 3. 前端应用入口、工作区与导航

| 文件 | 详细职责 |
|---|---|
| `src/main.ts` | Vue 应用启动，初始化外观，加载全局 CSS，根据固定 `view` 参数创建主界面/设置/登录入口，并包装窗口外壳 |
| `src/App.vue` | 主窗口组合根组件；创建工作区，通过 `workspaceKey` 向 AccountPanel、ChatPanel、结果和查询子组件提供同一实例 |
| `src/SettingsPage.vue` | 唯一设置窗口；应用/账号/外观三个标签，共享工作区上下文；切换标签保留未保存表单 |
| `src/LoginPage.vue` | 独立登录窗口包装层，挂载 LoginWizard |
| `src/types.ts` | TS 业务 DTO 与 QuerySpec V2、ScanResult、Post、Filter、Account、Settings 类型；和 Rust serde 数据结构保持一致 |
| `src/useWorkspace.ts` | 主工作区状态与 IPC 命令协调：账号、聊天、扫描、取消、原文链接、转发逻辑、设置/登录窗口入口；捕获 Profile/Chat/Run 防旧响应误用 |
| `src/accountViews.ts` | 按 Profile ID 保留聊天列表、Query Builder 草稿、扫描状态等独立视图；`selectChat` 在聊天 ID 改变时清理旧查询；进度按 generation/run/chat 校验 |
| `src/useLogin.ts` | PendingLogin 的手机号、数据库密码、验证码/2FA、邮箱或设备确认；保护临时输入与事件先后顺序 |
| `src/useApplicationSettings.ts` | 非敏感全局配置表单、Hash/代理凭据保存与清除、Debug 完整重置的交互确认 |
| `src/useSettingsNavigation.ts` | 设置窗口初始目标读取与新事件竞争仲裁；将“应用/账号/外观”导航和显式操作账号分开 |
| `src/vite-env.d.ts` | Vite/TypeScript 前端环境声明 |
| `src/theme.json` | Rust 与 Vue 共用的窗口背景/主题 Token 数据 |

**依赖关系**：业务组件通过 `useWorkspaceContext()` 获取工作区操作，不各自创建第二个账号 Runtime。设置窗口是自己的 Vue 界面实例，但账号目标由 Rust 的 `settings_target` 显式指定，不接受“默认把前台账号当作设置目标”的隐式假设。

## 4. 主界面与通用组件

| 文件 | 具体职责及交互 |
|---|---|
| `src/components/WindowShell.vue` | 将原生窗口标题栏与主业务页面组合；统一应用窗口外壳 |
| `src/components/WindowTitlebar.vue` | Windows 窗口按钮、拖动、最小化/最大化/关闭等原生交互 |
| `src/components/WorkspaceLayout.vue` | CSS Grid 三栏、左右可拖动 Splitter、键盘方向键/Home、小窗口查询侧边抽屉 |
| `src/components/AccountPanel.vue` | 左上账号身份/头像/状态、添加账号、切换账号、菜单（断开/恢复/账号设置）、应用设置入口 |
| `src/components/ChatPanel.vue` | 列表与搜索；全部/普通/归档范围、`@username` 公共解析、头像及当前聊天选择 |
| `src/components/QueryPanel.vue` | 查询主表单；Post 范围、日期区间、递归过滤器、排序、输出上限、执行/取消 |
| `src/components/FilterEditor.vue` | 递归 AST 编辑器：比较、空值/存在、内容类型存在/不存在、AND/OR/NOT 子条件 |
| `src/components/AppSelect.vue` | 与主题一致、可键盘操作的选择器，替代大量原生 select |
| `src/components/AppDateTime.vue` | 日期时间输入与主题日历/滚轮控件；界面本地时间最终转换为 Unix 秒 |
| `src/components/MessageResultsPanel.vue` | 当前聊天标题、扫描状态和统计、Post 卡片、媒体组展开、每页 40 条与底部翻页；翻页回滚顶部并遵循动画偏好 |
| `src/components/MediaPreview.vue` | 单成员媒体预览，微型图/缩略图与照片原图渐进加载，悬停展示该消息浏览/转发数，卸载时撤销订阅 |
| `src/components/AvatarPreview.vue` | 账号/聊天头像、首字母/微型图回退，可视区域加载和受限缓存 |
| `src/components/LinkedText.vue` | 安全渲染文本 URL 和 Telegram 隐藏 `textUrl` 实体，避免把原生 HTML 当作可信内容 |
| `src/components/UiIcon.vue` | UI 线条图标的统一入口 |
| `src/components/ConfirmDialog.vue` | 通用操作确认，支持必须输入指定短语的危险操作 |
| `src/components/ApplicationSettings.vue` | API ID/Hash、SOCKS5、全局秘密清除、Debug 重置 |
| `src/components/AccountSettings.vue` | 明确目标账号的身份、Session 密码恢复、授权、断开/恢复、注销和添加账号 |
| `src/components/AppearanceSettings.vue` | 外观主题、强调色、间距、字号、阴影、滚动条、玻璃与动画的预览/保存 |

**特别注意**：`useWorkspace.ts` 已存在选择、转发预览、执行转发和后端响应处理函数；`MessageResultsPanel.vue` 当前并没有渲染相应的完整选择/批量发送控件。这是**后端能力与产品 UI 的区别**，不要在文档或演示中说界面已经具备一个未显示的按钮。

## 5. 前端基础服务与样式

| 文件 | 职责 |
|---|---|
| `src/media/mediaRequests.ts` | 统一媒体请求入口，封装受控 Tauri IPC、缓存读取与按账号清理 |
| `src/media/mediaScheduler.ts` | 任务优先级、同文件多订阅者去重、AbortSignal 独立取消、全局/账号并发、缩略图和原图双 LRU + TTL |
| `src/media/avatarCache.ts` | 头像独立缓存、受限并发、内存预算、generation 换代清理 |
| `src/scan/scanUi.ts` | ScanStatus、StopReason、PostCompleteness 的显示名称与不完整 Post 警示判断 |
| `src/text/linkedText.ts` | 自动识别 URL、处理实体偏移、清理外链边界与安全文本渲染辅助 |
| `src/theme/appearance.ts` | 外观类型、默认值、合法性验证、CSS 变量应用、跟随系统、跨窗口同步 |
| `src/theme/appearanceClient.ts` | 外观 IPC 预览/保存串行化，revision/editor 防止异步过期状态覆盖 |
| `src/theme/scrollMotion.ts` | 结果翻页滚动：启用动画时平滑滚动，禁用动画或系统减少动态效果时即时跳转 |
| `src/window/windowControls.ts` | 窗口操作 Tauri 适配，业务组件不直接实现底层窗口控制 |
| `src/workspace.css` | 主窗口布局、三栏尺寸与滚动容器、结果卡片和分页位置 |
| `src/theme/tokens.css` | 颜色、圆角、语义变量与基础设计 Token |
| `src/theme/components.css` | 按钮、输入、卡片、指标、媒体、通用控件样式 |
| `src/theme/settings.css` | 设置/登录窗口布局和卡片样式 |
| `src/theme/appearance-settings.css` | 外观编辑页面样式 |
| `src/theme/selection-motion.css` | 分段选择器/标签过渡动画，尊重动画开关 |
| `src/theme/glass.css` | 液态玻璃主题透明度、模糊与回退 |
| `src/theme/window-chrome.css` | 原生标题栏与窗口边界的网页侧样式 |

样式约定：业务样式尽量使用 `--surface`、`--text`、`--border`、`--accent`、`--radius` 等语义 Token；不要在组件里重复定义全局控件体系。减少动态效果的判断由根部 `data-motion` 与 `prefers-reduced-motion` 一起决定。

## 6. Rust/Tauri 服务与配置

### 6.1 进程入口、协议与业务

| 文件 | 详细职责 |
|---|---|
| `src-tauri/src/main.rs` | Windows 桌面程序入口，Release 关闭额外控制台，调用 `tmsgpicker_lib::run()` |
| `src-tauri/src/lib.rs` | 库模块注册与 desktop feature 门控，暴露 Mock 可测核心 |
| `src-tauri/src/app.rs` | 核心命令/事件注册、`AppState`、账号 Runtime Map、窗口创建/退出、run/epoch 生命周期、后台任务、转发回执、全局重建/注销 |
| `src-tauri/src/gateway.rs` | `TelegramGateway`、`SessionLifecycle` trait 与共同错误类型；切断纯业务对具体 TDLib Client 的依赖 |
| `src-tauri/src/tdlib/ffi.rs` | TDLib JSON C API 的 `extern` 定义和 FFI 链接入口 |
| `src-tauri/src/tdlib/gateway.rs` | 一个共享 `td_receive` 线程、多个 ClientSlot、`@client_id/@extra` 响应路由、请求批量提交、Router Pending、close/logOut 提交态 |
| `src-tauri/src/tdlib/runtime.rs` | TDLib 客户端与 Session 目录布局、初始化/路径处理的原生适配 |
| `src-tauri/src/tdlib/mod.rs` | `tdlib` 子模块组织 |
| `src-tauri/src/services.rs` | `AuthService`、`ChatService`、`ActionService`：TDLib 参数/代理初始化、授权、聊天列表、媒体文件验证、消息链接、转发预检/执行 |
| `src-tauri/src/core.rs` | `TelegramMessage` 归一化、Post 组装/指标、Filter AST 与三值逻辑、QuerySpec V2、比较器及扫描 DTO |
| `src-tauri/src/formatted_text.rs` | Telegram 文本格式化实体/隐藏文本链接解析及安全文本处理 |
| `src-tauri/src/settings.rs` | `ApplicationConfig`、`AccountRegistry`、`AccountCredentialStore`/Store、DPAPI、安全事务、迁移与非敏感快照 |
| `src-tauri/src/login.rs` | 新账号/恢复账号的 `PendingLogin` 及授权状态推进，成功 `getMe` 登记，失败与取消清理 |
| `src-tauri/src/settings_target.rs` | 设置导航目标及其合法性，绑定明确账号，防止跨窗口意外误操作 |
| `src-tauri/src/appearance_session.rs` | 全局外观预览会话及编辑版本、保存/回滚协调 |
| `src-tauri/src/window_theme.rs` | Windows DWM 原生标题栏颜色及主题同步，旧系统降级 |
| `src-tauri/src/managed_data.rs` | 应用专属目录所有权检查、Windows 重解析点/符号链接防护、句柄级安全删除 |
| `src-tauri/src/reset.rs` | Debug 完整重置的多账号注销协调与阶段性完成判断（Mock 可验证） |

`app.rs` 是跨模块协调层，不宜把领域过滤规则直接写入 IPC handler；`core.rs` 和 `scan/*` 尽量保留可脱离 Tauri/TDLib 运行的纯逻辑。

### 6.2 扫描子模块

| 文件 | 详细职责 / 不变量 |
|---|---|
| `src-tauri/src/scan/mod.rs` | 扫描协调器、内部预算、进度快照、终止状态、结果统计；扫描中最多输出 40 个候选 Post |
| `src-tauri/src/scan/history.rs` | `getChatHistory` 有界页读取、严格向旧 ID 推进、单聊天数值 ID 去重、可回退的日期定位 |
| `src-tauri/src/scan/accumulator.rs` | 流式 Post 累积，跨页媒体组连续合并，遇下一个组或历史尽头再确定边界 |
| `src-tauri/src/scan/boundary.rs` | 依据上下边界及明确缺失证据评估 `Verified/Assumed/Truncated/Unknown` |
| `src-tauri/src/scan/scope.rs` | 最近 N / 日期最新 N / 日期全部的**Post**计数与停止决策 |
| `src-tauri/src/scan/topk.rs` | 过滤一次、维护有界排名堆和匹配数量；对 UI 提供前 K 名快照 |

添加新排序字段时，要保证 `core.rs::field_value()` 与 `compare_posts()` 的数值/null 语义不分叉；不能只改前端选项，忘记 Top-K 比较器与测试。

### 6.3 开发工具

| 文件 | 职责 |
|---|---|
| `src-tauri/src/bin/tdlib_smoke.rs` | 手动 TDLib 原生冒烟工具；属于开发 Bring-up 而不是面向用户的 UI |
| `src-tauri/src/bin/tdlib_auth.rs` | 早期手动授权诊断 CLI；可能涉及敏感输入/日志，勿将输出贴入公开 Issue |
| `src-tauri/build.rs` | 检查 `tdjson.lib` 与运行 DLL（zlib 可为 `z.dll`/`zlib1.dll`），添加 MSVC 链接路径并 staging 到 Cargo profile；**不负责 NSIS 打包** |
| `src-tauri/Cargo.toml` | Rust 包、desktop feature、依赖与 Release 优化配置 |
| `src-tauri/Cargo.lock` | 固定 Rust 依赖解析版本 |
| `src-tauri/tauri.conf.json` | Tauri 产品、窗口尺寸、安全 CSP、前端入口、Bundle 配置（尚非发布验收） |
| `src-tauri/capabilities/default.json` | 主窗口及原生插件权限范围 |
| `src-tauri/capabilities/settings.json` | 设置/登录辅助窗口可用的受限权限 |

## 7. 自动化测试与 CI

### Rust

| 文件 | 重点 |
|---|---|
| `src-tauri/tests/core_regression.rs` | 通用过滤、消息字段、媒体组、扫描和发送动作的 Mock Regression |
| `src-tauri/tests/post_scan_v2.rs` | Post 级范围、跨页 Album、日期边界/日期定位回退、资源预算、协议版本 |
| `src-tauri/tests/performance_regression.rs` | 大量合成消息下 Top-K、排序一致性、批量聊天详情 |
| `src-tauri/tests/link_entities.rs` | URL / Telegram formatted text 实体和 Unicode 边界行为 |
| `src-tauri/tests/fixtures/messages.json` | 人工合成的测试消息，不是实际用户历史 |
| Rust 模块内 `#[cfg(test)]` | Receiver/Client 关闭、Store 事务、DPAPI 模拟、固定目录清理、边界判定、重置进度等 |

### 前端（`tests/`）

| 文件 | 重点 |
|---|---|
| `account_ui.test.ts` | 左侧账号布局、空状态、账号操作入口 |
| `workspace.test.ts` | 工作区与多账号隔离、转发确认、设置对象与旧事件 |
| `chatScanIsolation.test.ts` | 切聊天清理结果、旧 run 拒绝、交互归属 |
| `login_flow.test.ts` | 授权事件顺序、重复手机号表单、验证码与 2FA |
| `debug_reset.test.ts` | Debug 重置确认与失败呈现 |
| `appearance.test.ts` / `appearance_client.test.ts` | 外观校验、预览/保存及跨窗口竞态 |
| `settings_navigation.test.ts` / `settingsMotion.test.ts` | 设置目标绑定、导航与动画 |
| `mediaScheduler.test.ts` | 请求去重/取消、自动原图优先级、有界缓存、账号换代 |
| `postScanUi.test.ts` | 扫描状态与 Post 完整性展示分离 |
| `scrollMotion.test.ts` | 翻页滚动、动画关闭、系统减少动态效果 |
| `linkedText.test.ts` | 外链识别、实体和标点边界 |
| `windowChrome.test.ts` | 窗口控制与受限能力验证 |

`package.json` 定义 `pnpm.cmd test`、`pnpm.cmd build` 与 `pnpm.cmd tauri dev`。`vite.config.ts`、`tsconfig.json`、`tsconfig.node.json` 管理前端构建与类型检查。`.github/workflows/frontend-ui.yml` 在 PR/主分支变更中执行前端测试/构建与 Windows Rust **no-default-features** 领域回归；不连接真实 Telegram，也没有一键构建所有原生 DLL 的发行工作流。

## 8. 典型功能调用路径

### 8.1 从账号登录到聊天查询

```text
LoginWizard → useLogin → begin_login / login_auth_submit
  → app.rs / login.rs → AuthService → TelegramGateway
  → TDLib Ready + getMe → settings.rs 注册正式 Profile
  → accounts-changed / login-completed
  → AccountPanel / ChatPanel → chats / public_chat
  → QueryPanel → QuerySpec V2 → start_query
  → scan/history → accumulator → scope → topk
  → query-progress → accountViews → MessageResultsPanel
```

**异步更新的归属**：Profile ID、epoch、run ID、Chat ID 都是必要条件；不能为了简化 UI 把它们合并成一个“全局扫描状态”。切换聊天会释放旧查询，切换账号不重建其他账号的客户端。

### 8.2 展开媒体组

```text
MessageResultsPanel → MediaPreview
  → cachedMedia / mediaScheduler（按账号限流 + 同文件合并）
  → photo_thumbnail Tauri command
  → ChatService::media_data → TDLib downloadFile
  → 账号专属文件路径验证 → Base64 Data URL
  → 先微型图/缩略图，后自动原图；组件销毁撤销订阅
```

不要从前端任意读取 TDLib 文件路径，也不要在临时组件中绕开 `mediaScheduler` 直接发大量同步下载。

### 8.3 设置与退出

```text
ApplicationSettings / AccountSettings / AppearanceSettings
 → useApplicationSettings / useWorkspace / appearanceClient
 → app.rs：配置事务、明确账号操作、主题预览、生命周期锁
 → settings.rs / appearance_session.rs / login.rs / TDLib Session
 → 有真实回执和正确所有权后才能标记完成或清理数据
```

**扩展时的检查顺序**：修改 DTO → Rust 校验与服务 → 前端状态/组件 → Mock 测试 → 离线 CI → Windows 原生实机验收。涉及 Session、发送、账号删除或 API 秘密的更改，应先保证失败情况下保留数据及可恢复状态。

## 9. 不属于当前文件结构的功能

目前仓库没有独立的云后端、账号代理服务器、自动更新服务、统一搜索数据库、跨平台发布脚本、自动构建 TDLib 的脚本，也没有已经验收的 NSIS/MSI 制品。`tauri.conf.json` 开启 `bundle` 不代表打包完成了所有 TDLib/OpenSSL/zlib 依赖。

仓库的旧开发规格、阶段验收报告已收敛为三份维持中的文档。需要查具体历史设计、当时的修改过程或某次 CI 证据，可查看 [Devlog.md](Devlog.md) 中相应日期/提交，再通过 Git 历史打开旧文件版本；不要在当前维护入口中保留多份相互矛盾的现行规范。

## 10. Windows 原生依赖与 Release 脚本索引

| 路径 | 职责 |
|---|---|
| `scripts/native-versions.json` | **TDLib 1.8.68（2026-10-12）**、固定 TDLib SHA、vcpkg SHA 及 OpenSSL/zlib/gperf port 修订号 |
| `scripts/build-tdlib.ps1` | 从完整 SHA 检出 TDLib/vcpkg，检查 port 版本，调用 vcpkg 和 VS2022 CMake，生成 `tdjson.lib/dll`、依赖 DLL、SHA-256 manifest |
| `scripts/stage-native.ps1` | 检查固定构建产物哈希，复制实际版权声明，生成 Tauri Release-only `tauri.release.generated.json` |
| `tests/releaseStaging.ps1` | Mock 原生依赖文件、manifest 和许可证，离线验证 Release staging/Tauri overlay 及文件哈希拒绝逻辑（不加载真实 TDLib） |
| `scripts/audit-pe-imports.ps1` | 用 MSVC dumpbin 验证免安装目录 EXE/DLL 的 PE 导入依赖是否由分发文件或系统库提供 |
| `scripts/package-release.ps1` | 检查 NSIS 包内容、组装含运行时和许可声明的 Portable ZIP、SHA256SUMS |
| `.github/workflows/release.yml` | PR 静态校验、workflow_dispatch 构建资产、`vX.Y.Z` 标签构建后创建 Draft Prerelease |
| `docs/BUILD_WINDOWS.md` | 从空白开发机开始的 Windows/TDLib 依赖构建操作及故障处理 |
| `docs/RELEASE.md` | CI/CD、打包边界、BYO API、安全、许可证及真实干净机验收流程 |

`native/` 和 `src-tauri/tauri.release.generated.json` 是**生成内容**，不进入 Git 历史。普通用户无需编译 TDLib；官方二进制只携带运行 DLL，不含 `tdjson.lib`、CMake、gperf 或源码。发布流程仍需要真实 Windows 安装/升级与 TDLib 会话验收，不能把脚本和 CI 的存在直接写成公开 Release 已经完成。
