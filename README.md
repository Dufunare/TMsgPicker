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

主窗口为左侧 Account & Chats、中间 Messages & Results、右侧 Query Builder。两条分界线保留 6px 透明命中区，默认只有极淡细线，Hover/Focus/Drag 显示半透明 1px 提示；可拖动，也可聚焦后按左右方向键调整、Home 重置；Settings「安全与界面」中提供重置布局。小于 1060 CSS px 时查询栏变成可打开的侧边抽屉。

左上用户卡片显示姓名、username 与状态，右侧图标提供添加、切换和更多操作；卡片底部小箭头可收缩到 Compact 模式并记住偏好。卡片外侧齿轮打开独立的 500 × 580 Settings 小窗；重复点击聚焦已有窗口，关闭小窗不会断开 TDLib。API/代理、数据库口令和手机号/验证码/2FA 授权均在小窗中处理，不占用聊天列表。保存配置优先于 `TMSGPICKER_API_ID` / `TMSGPICKER_API_HASH` 环境变量，未配置时使用环境变量；密码输入框留空保留已保存值，明确的清除按钮删除保存值。保存 API/代理配置会安全关闭并重建全部账号；各账号的失败原因独立显示。已解锁账号可用本进程内口令恢复，未解锁账号仍需口令。配置重置只重置普通设置，保留账号、秘密与 Session；同样重建全部账号。

数据位于 `%LOCALAPPDATA%\com.funare.tmsgpicker\`：`app-config.json` 保存唯一应用配置，`accounts.json` 保存账号注册表，`credential-store.json` 只保存当前 Windows 用户范围的 DPAPI 密文。旧 `settings.json` 非破坏迁移后保留为原始备份，后续不再读取它。修改先暂存，三个文件经 flush、Windows 原子替换及回滚日志协调提交，成功后才更新内存；写入失败恢复旧状态，启动前恢复未完成事务；损坏或未来版本明确报错。回滚日志仅保存原文件字节，秘密仍为 DPAPI 密文。API Hash、代理密码和可选数据库口令仅由 Rust 解密，读取 IPC 只返回保存标志，不回填 Vue。数据库口令按 Profile ID 隔离，仅初始化成功后保存；验证码和 2FA 不持久化。保护失败需清除对应保存凭据并重新输入，不降级为明文。

首次升级检测 `tdlib/db` 后幂等导入 Legacy Profile，继续使用原来的 `tdlib/db` 和 `tdlib/files`，不迁移、覆盖或删除。数据库密码必须使用原有 passphrase；仍按原规则 Base64 编码为 TDLib bytes（Base64 不等于加密）。新账号使用 `tdlib/accounts/<内部 Profile ID>/db` 与 `files`。默认手工输入口令；只有用户勾选「记住此设备」才保存 DPAPI 密文，取消勾选立即清除保存口令而保留数据库。

启动时为全部已登记 Profile 创建独立 TDLib Client，并发初始化具备保存口令的账号。没有保存口令的 Client 保持“需要解锁”，不提交数据库参数，不生成新密钥。注册表为空时显示“添加账号”，不创建匿名 Client。整个桌面进程只有一个共享 `td_receive` 线程。

「添加账号」创建独立 Profile 和 Client；选择账号只切换前端视图，全部客户端继续运行，后台查询继续，切回恢复该账号的聊天、查询条件、结果与选择。普通断开和退出应用使用 `close`；只有经确认的「退出 Telegram 登录」调用 `logOut`，随后等待 TDLib 自行关闭。移除 Profile 默认保留 Session 文件；本地数据删除需二次确认、安全关闭及目录归属／重解析点检查，Legacy 目录不可由此删除。重复 `getMe` 身份不写入冲突 Profile，冲突账号安全断开并提示显式移除；已有账号与目录保持完整。

SOCKS5 服务器填写主机（如 `127.0.0.1`），端口使用实际 SOCKS5/Mixed 监听端口；未设置代理认证时用户名和密码留空。TDLib 不自动跟随 Windows 的 HTTP 代理。

每个 Client 的初始化顺序：先发送 `addProxy(enable=true)` 或 `disableProxy`，随后立即发送 `setTdlibParameters`，两条都发出后再等待响应。**不能先等代理响应再提交参数**：代理请求在缺少参数时排队。先等待参数响应，参数失败清理其余 Pending，不被代理阻塞 45 秒。代理设置成功不表示网络已连通，授权状态与连接状态独立显示。用户选中需要处理的账号后手动打开 Settings 完成授权，不会为多个后台账号自动弹窗。

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

三栏与多账号交付记录见 [第一轮验收](docs/UI_ACCOUNTS_ACCEPTANCE.md)，当前紧凑卡片、独立窗口与隐形 Splitter 的验证及操作步骤见 [第二轮 UI 验收](docs/UI_REFINEMENT_ACCEPTANCE.md)。本机运行：`pnpm.cmd tauri dev`，或构建 `pnpm.cmd tauri build --debug --no-bundle` 后启动 `src-tauri/target/debug/tmsgpicker.exe`（同目录需 staging 的 DLL）。

## 开发接口与部署限制

命令、事件和 DTO 见 [IPC 接口](docs/IPC.md)，数据结构、状态机、调用流程及本轮手动验收见 [Runtime 架构与验收](docs/RUNTIME_ARCHITECTURE.md)。所有账号业务 IPC 显式携带 Profile ID；共享接收线程先按 `@client_id` 找到 Client 专属 Router，再按 `@extra` 匹配响应。后台线程执行网络与扫描，UI 仅用 `invoke`／事件。

当前只完成开发期 native staging，尚未制作/验收 MSI、NSIS 安装包。后续部署需打包相同版本的 TDLib/OpenSSL/zlib DLL，验证 MSVC 运行库、WebView2、更新机制与许可证。`pnpm tauri build` 的安装包分发不属于已验证项。

其他限制：聊天首版每次加载最多 100 项；媒体采用类型占位，不下载缩略图/播放视频；单聊天内存查询，最大原始扫描预算 100000、展示上限 1000；未实现自动重试或实时全局排名。真实网络登录、代理/Session 复用、私密链接和真实转发仍需上述本机手工验收。
