# TMsgPicker

基于 **Tauri 2 + Vue 3 + TypeScript + Rust 2024 + TDLib JSON C API** 的 Windows x64 Telegram 消息筛选工具。设计基线为 [产品说明](docs/TMsgPicker_PROJECT_SPEC.md)，实现与验收记录见 [v0.1 交付记录](docs/MVP_ACCEPTANCE.md)。

当前功能：账号授权与 Session 复用、可配置 SOCKS5、聊天列表/搜索/公开 username 解析、最近 N 条或日期范围扫描、嵌套 AND/OR/NOT 条件、互动指标/Emoji/结构/空值筛选、稳定排序、Album 结果卡片、取消与进度、权限检查后的原消息链接、预检并确认的手动转发。

## 本地启动

需要 Windows x64、MSVC C++ 工具链、WebView2、Rust 和 Node/pnpm，以及已编译好的 TDLib 文件：`tdjson.lib`、`tdjson.dll`、`libssl-3-x64.dll`、`libcrypto-3-x64.dll`、`z.dll`。无需重新编译 TDLib。

默认检测仓库 `Release/`，找不到导入库再检测 `Release/Release/`；也可指定外部目录：

```powershell
$env:TMSGPICKER_TDLIB_DIR = 'D:\Native\TDLib'
pnpm.cmd install --frozen-lockfile
pnpm.cmd tauri dev
```

`build.rs` 检查全部文件，将 DLL 暂存到 Cargo profile 目录和测试程序所在 `deps/`。缺失文件会提供路径和配置提示。`pnpm.cmd` 可避免 PowerShell 执行策略阻止 `pnpm.ps1`。

主窗口为左侧 Account & Chats、中间 Messages & Results、右侧 Query Builder。两条分界线保留 6px 透明命中区，可拖动或聚焦后按方向键调整、Home 重置。栏宽仅在当前窗口内生效，每次启动恢复固定默认比例（左 23%、右 29%），不写入配置。小于 1060 CSS px 时查询栏变成可打开的侧边抽屉。用户卡片固定使用紧凑布局，不再提供 Compact 设置。

没有正式账号时只显示“尚未添加 Telegram 账号”和添加按钮，保留外侧应用设置齿轮；不自动创建占位 Profile。正式账号卡片显示 getMe 返回的姓名、username 与网络状态。更多菜单只提供“断开连接 / 恢复连接”（互斥）和“账号设置”。查看账号仅改变 ActiveProfileId，不写 UI 偏好，也不改变后台 Client 或查询。

设置窗口与登录窗口默认均为 500 × 580，重复点击聚焦已有窗口：

- **添加 Telegram 账号**：所有添加入口打开唯一 LoginWindow，创建 PendingLogin 与隔离 Session。提交手机号、数据库密码和首次登录的“记住此设备”后等待验证码，不重复要求输入手机号。验证码与 Telegram 2FA 密码在同一表单中填写；启用 2FA 时可一起提交，未启用时密码留空。后端先验证验证码，仅在实际进入 WaitPassword 后验证密码；密码错误可在同一表单中只重试密码。邮箱及设备确认仍按实际授权状态处理。Ready 后 getMe 去重，身份和可选 DPAPI 口令在同一事务中正式登记，主界面选中新账号。取消使用 close，等待 Closed 后仅清理本次新建的临时目录。旧版本未完成登录／Legacy 在此窗口单独恢复，取消恢复保留原数据。
- **应用设置子页面**：外侧齿轮打开统一设置窗口并定位应用子页面，保留 API ID / Hash、SOCKS5、全局敏感凭据清除及 Debug 工具。配置优先于 `TMSGPICKER_API_ID` / `TMSGPICKER_API_HASH` 环境变量，秘密输入留空保留保存值。保存时安全关闭全部 Client 后重建运行中的账号，主动断开账号继续保持断开。
- **账号设置子页面**：更多菜单复用同一个设置窗口，定位账号子页面并选择该账号。页面内可选择其他账号，显示只读 Telegram 身份、状态、断开／恢复、注销及添加新账号。缺少口令时输入原数据库密码继续恢复。不提供改名、独立解锁菜单、记住设备、API、代理或布局设置。注销后刷新账号列表，空列表显示添加引导；设置窗口保持打开。主窗口切换不自动改变设置操作对象。
- **外观设置子页面**：与应用设置、账号设置同级，独立预览与保存主题、强调色、密度及高级外观选项。主题支持跟随系统、浅色、深色和液态玻璃；液态玻璃采用与标题栏衔接的浅色渐变背景、磨砂半透明面板和边缘高光，不支持背景模糊时回退为实色面板。各页面按关联功能分组为卡片；切换子页面保留未保存的输入。

三个设置子页面通过顶部按钮切换，保留未保存的应用输入；显式选择其他账号时清空原账号的临时口令与授权输入。窗口关闭不改变客户端生命周期。

数据位于 `%LOCALAPPDATA%\com.funare.tmsgpicker\`：`app-config.json` 只保存应用配置，`accounts.json` 保存正式账号、主账号和分离的待恢复记录，`credential-store.json` 只保存当前 Windows 用户范围的 DPAPI 密文。旧 `settings.json` 保留为迁移备份，旧栏宽／Compact 字段被忽略，账号与凭据不丢失。三个文件仍通过暂存、flush、Windows 原子替换及回滚日志协调提交，失败恢复旧内存与磁盘，损坏或未来版本明确报错。秘密只由 Rust 解密，读取 IPC 不回填 Vue；新数据库口令只在初始化、Ready、getMe、去重和登记成功后按首次登录选择保存。手机号、验证码、2FA 不持久化，不降级明文。

Legacy Session 继续使用 `tdlib/db` 和 `tdlib/files`，未识别身份前作为独立恢复记录，不显示占位账号。新账号使用 `tdlib/accounts/<Profile ID>/db` 与 `files`，运行中的数据库不移动目录。原密码仍按既有 Base64 → TDLib bytes 规则传递（Base64 不等于加密）。未记住设备时，下次启动仍需原数据库密码。

启动为全部正式账号创建独立 TDLib Client，具备保存口令的账号并发恢复。无口令账号保留登记和原 Session，Runtime 等待原密码，不使用空密钥或删除数据库。旧恢复记录也可独立启动并完成 getMe 识别。主账号属于 AccountRegistry：优先合法主账号，升级时一次兼容合法的旧 `last_profile_id`，否则选择最早登记的账号。查看其他账号不改变主账号；主账号注销后自动回退，最后一个注销后返回空状态。整个进程仍只有一个共享 `td_receive` 线程。

普通断开和退出应用只使用 `close` 并等待 Closed，保留 Session、登记和保存口令。网络暂断由 TDLib 自行重连，不等于主动断开。**注销账号**需要明确确认，只对指定账号执行 `logOut`，确认 ok 和 Closed 后检查并清理其私有数据库／文件，再删除登记与口令；Legacy 仅清理自己的 `db`、`files`，保护其他账号。响应未确认、关闭超时或清理失败均保留登记并报错；已确认的退出／关闭进度在本进程内可重试，不重复 logOut。重复 getMe 身份不覆盖原账号，新登录安全关闭并提示取消。

Debug 应用设置提供“重置 TMsgPicker（注销全部账号并清除全部本地数据）”：列出数据类别、两次确认并输入 `RESET TMSGPICKER`，沿用 Pending 发送等待／Unknown 明确确认。先阻止新任务、取消查询并等待转发提交，遍历注册表中全部关联账号，包含主动断开账号；必要时用原 Session 和数据库口令恢复 Client，逐一执行 logOut 并确认真实 Closed。有效授权的 Pending Client 也须注销，未授权 Client 可 close。任何失败都会保留失败账号的 Session／凭据和登记，显示逐账号结果并允许重试，已完成注销的账号不会重复执行。只有全部完成后才严格清理固定应用专属目录的配置、凭据、全部 Session、Legacy 与缓存；Windows 使用不跟随 Reparse Point 的锁定句柄防止越界。成功退出，重启显示无账号首次启动界面；删除失败明确报告。Release 通过 Rust `cfg(debug_assertions)` 排除危险命令注册和实现。普通退出、断开和账号切换仍不发起 logOut。自动化只使用 Mock TDLib 和独立临时合成数据。

所有窗口共享 `src/theme.json` 背景 Token。Windows 11 原生标题栏使用 DWM caption、border、text 属性，COLORREF 正确按 BGR 字节转换；旧系统保留原生降级，拖动与系统窗口按钮继续由系统处理。

SOCKS5 服务器填写主机（如 `127.0.0.1`），端口使用实际 SOCKS5/Mixed 监听端口；未设置代理认证时用户名和密码留空。TDLib 不自动跟随 Windows 的 HTTP 代理。

每个 Client 的初始化顺序：先发送 `addProxy(enable=true)` 或 `disableProxy`，随后立即发送 `setTdlibParameters`，两条都发出后再等待响应。**不能先等代理响应再提交参数**：代理请求在缺少参数时排队。先等待参数响应，参数失败清理其余 Pending，不被代理阻塞 45 秒。代理设置成功不表示网络已连通，授权状态与连接状态独立显示。新账号在 LoginWindow 授权；现有 Session 确实失效时在绑定账号设置重新授权，不为后台账号自动弹窗。

## 查询语义

- 所有 Telegram ID 通过字符串 DTO 传给 TypeScript，包括 Album 和 custom emoji int64。日期使用 UTC Unix 秒，界面日期范围输入按本地时间转换；区间为 `[开始,结束)`。
- 最近 N 条指原始消息；结果上限指筛选后的逻辑帖子。短页不会终止扫描；排除重复游标、持续向旧消息推进。空页结束，无进展为 Partial，网络/限流错误为 Error，取消为 Cancelled，保留已取得候选。
- 单字段未知值显示 `—`，与 0 不同。数字比较未知值返回 Unknown，NOT(Unknown) 仍为 Unknown，只选择 True。反应对象存在但缺少某个 Emoji 视为 0；整个反应对象缺失视为未知。
- Album 按 `(chat_id, media_album_id != 0)` 分组，跨页合并。每个指标取日期/ID 最早且该字段有效的成员，并记录来源；不累加浏览、转发、评论或反应。
- TDLib 消息没有媒体组总成员数，本版对 **所有 Album 保守标记成员完整性未知**，含 Album 的查询最终为 Partial，不标 Exact。这不会阻止浏览和手动选择已扫描成员，但转发仅覆盖明确列出的成员。Single 查询完整扫描 Scope 后可为 Complete/Exact；Exact 只针对当前 Scope 和读取时指标快照。
- 升/降序均为空值最后；同指标按 date、数值 chat ID、数值 message ID 确定顺序。排序最多支持 10 个字段，UI 首版编辑一个字段。
- 「排除疑似推广」是可编辑的 `NOT (reply_info IS NULL AND post_kind = single)` 查询预设，可能误判，仅适合用户观察到该结构规律的频道。

## 消息动作

打开原文先查 `getMessageProperties.can_get_link`，再请求 TDLib `getMessageLink`；Album 使用 `for_album=true`。不自行生成私密消息链接。

选中帖子 → 选择目标或解析 Saved Messages → 准备转发 → 查看逐条权限预检 → 明确确认。后端重新验证源消息转发能力和目标成员/发帖/媒体权限。按源聊天分组，成员 ID 去重后严格升序，每批最多 100，使用正常转发，不复制绕过保护。Album 的动作范围是卡片中已扫描的所有成员。

结果逐条区分 Rejected、Failed、Pending、Sent。返回 `null` 或缺项计为 Failed；`messageSendingStatePending` 仅为 Pending，`updateMessageSendSucceeded` / `updateMessageSendFailed` 更新最终状态。断开、退出或修改全局设置时，若发送至少 30 秒仍无回执，可由用户明确确认安全关闭，并将这些发送标为 **Unknown（结果未知）**。取消不关闭账号；确认只使用 close，不注销。Unknown 可能已送达，请先在 Telegram 核对，避免重复发送。记录按账号保留，在本进程断开／重建后仍可查看，退出后不持久化；不会自动重发。

Client 在发送 close 或 logOut 前即停止接受新业务请求，避免后台任务在关闭期间继续提交。共享接收线程异常退出会立即使所有 Pending 请求失败并显示账号错误；重试断开可启动仅用于观察旧 Client 关闭的接收器，仍不接受新业务或重开 Session，全部收到真实 Closed 后才允许重连。Rust 异常退出处理不能恢复原生 DLL 访问违规或进程强制终止。

## 检查与手动验收

```powershell
# 离线核心 / mock 测试：不需要 TDLib DLL 或 Telegram 凭据
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
# Windows 原生接收线程生命周期与离线回归；不登录，不发送消息
cargo test --manifest-path src-tauri/Cargo.toml --lib --tests
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm.cmd build
pnpm.cmd test # Node 22.18+ / 24，Vue 内存渲染器；不访问 Telegram
```

现有 `src-tauri/src/bin/tdlib_smoke.rs` / `tdlib_auth.rs` 保持可用。`tdlib_auth` 是原有手动工具，代理设置及打印方式仍采用原来的 bring-up 代码；日常使用新的桌面授权服务，不将旧 CLI 的输出当作可分享诊断。

此前三栏与多账号记录见 [第一轮验收](docs/UI_ACCOUNTS_ACCEPTANCE.md) 和 [第二轮 UI 验收](docs/UI_REFINEMENT_ACCEPTANCE.md)；当前账号生命周期、统一设置／登录交互与测试结果见 [本轮交付记录](docs/ACCOUNT_LIFECYCLE_ACCEPTANCE.md)。本机运行：`pnpm.cmd tauri dev`，或构建 `pnpm.cmd tauri build --debug --no-bundle` 后启动 `src-tauri/target/debug/tmsgpicker.exe`（同目录需 staging 的 DLL）。

## 开发接口与部署限制

命令、事件和 DTO 见 [IPC 接口](docs/IPC.md)，数据结构、状态机、调用流程及本轮手动验收见 [Runtime 架构与验收](docs/RUNTIME_ARCHITECTURE.md)。所有账号业务 IPC 显式携带 Profile ID；共享接收线程先按 `@client_id` 找到 Client 专属 Router，再按 `@extra` 匹配响应。后台线程执行网络与扫描，UI 仅用 `invoke`／事件。

当前只完成开发期 native staging，尚未制作/验收 MSI、NSIS 安装包。后续部署需打包相同版本的 TDLib/OpenSSL/zlib DLL，验证 MSVC 运行库、WebView2、更新机制与许可证。`pnpm tauri build` 的安装包分发不属于已验证项。

其他限制：聊天首版每次加载最多 100 项；媒体采用类型占位，不下载缩略图/播放视频；单聊天内存查询，最大原始扫描预算 100000、展示上限 1000；未实现自动重试或实时全局排名。真实网络登录、代理/Session 复用、私密链接和真实转发仍需上述本机手工验收。
