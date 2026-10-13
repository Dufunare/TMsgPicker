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

界面输入 API ID/hash，或通过 `TMSGPICKER_API_ID` / `TMSGPICKER_API_HASH` 环境变量提供；界面留空时使用环境变量。凭据请自行在本机设置，勿粘贴到报告或提交文件。

首次启动选定一个非空数据库密码。后续必须输入相同密码。数据库和 Session 使用 `%LOCALAPPDATA%\com.funare.tmsgpicker\tdlib\db`，文件目录为同级 `files`，与现有 CLI auth 工具一致。密码经 Base64 编码传入 TDLib 的 bytes 字段，**Base64 本身不是加密**；由 TDLib 使用该密钥加密数据库。密码不持久化到源码、前端 storage 或配置文件，因此每次启动需重新输入数据库密码。

SOCKS5 的地址、端口和可选用户名/密码在界面配置。代理响应成功后才提交 `setTdlibParameters`；不启用代理时显式 `disableProxy`，避免 Session 中旧代理仍生效。修改代理/API 配置前先安全断开。手机号、验证码、2FA 和邮箱授权由界面提交；额外授权步骤显示明确提示，不自动重试验证码。

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

结果逐条区分 Rejected、Failed、Pending、Sent。返回 `null` 或缺项计为 Failed；`messageSendingStatePending` 仅为 Pending，`updateMessageSendSucceeded` / `updateMessageSendFailed` 更新最终状态。关闭前仍 Pending 的项目不能视为送达；本版不持久化发送结果。

## 检查与手动验收

```powershell
# 离线核心 / mock 测试：不需要 TDLib DLL 或 Telegram 凭据
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
# Windows 原生接收线程生命周期与离线回归；不登录，不发送消息
cargo test --manifest-path src-tauri/Cargo.toml --lib --tests
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm.cmd build
```

现有 `src-tauri/src/bin/tdlib_smoke.rs` / `tdlib_auth.rs` 保持可用。`tdlib_auth` 是原有手动工具，代理设置及打印方式仍采用原来的 bring-up 代码；日常使用新的桌面授权服务，不将旧 CLI 的输出当作可分享诊断。

最短手工验收：

1. `pnpm.cmd tauri dev`；输入本机 API 配置、现有数据库密码和 SOCKS5 配置，完成必要授权。安全断开再连接，确认无需再次输入登录码。
2. 刷新聊天，或解析一个已知公开 username；选聊天，扫描最近 50 条；核对分页进度、Album 合并、未知值与 0。切换评论/Emoji 条件和排序；结果上限设 1，确认仍扫描 50 条。
3. 选择较大范围，取消后立即运行新查询，确认旧 run 的事件不覆盖新结果；验证日期范围与预算耗尽的 Partial 状态。
4. 点击原文；在用户自行同意真实发送后，选一条合规消息，目标 Saved Messages，预检并确认转发，核对 Sent/Failed。再验证受保护内容被拒绝。**自动化测试不会执行真实转发。**
5. 关闭窗口，确认正常退出；异常关闭提示出现时保留程序并重试安全断开，不删除 Session 文件。

## 开发接口与部署限制

命令、事件和 DTO 见 [IPC 接口](docs/IPC.md)。Gateway 请求由唯一 dedicated receive 线程按 `@client_id` / `@extra` 路由，后台线程执行网络与扫描，UI 仅用 `invoke`/事件；没有 HTTP 服务或生产 mock 数据。

当前只完成开发期 native staging，尚未制作/验收 MSI、NSIS 安装包。后续部署需打包相同版本的 TDLib/OpenSSL/zlib DLL，验证 MSVC 运行库、WebView2、更新机制与许可证。`pnpm tauri build` 的安装包分发不属于已验证项。

其他限制：聊天首版每次加载最多 100 项；媒体采用类型占位，不下载缩略图/播放视频；单聊天内存查询，最大原始扫描预算 100000、展示上限 1000；未实现自动重试或实时全局排名。真实网络登录、代理/Session 复用、私密链接和真实转发仍需上述本机手工验收。
