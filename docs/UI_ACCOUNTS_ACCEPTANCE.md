# 三栏工作空间与多账号迭代验收

日期：2026-10-13。实现完成，Windows 开发版构建通过；真实 Telegram 账号与交互式 GUI 验收仍需下列手工步骤。

## 实现与设计决策

- 三栏占满主窗口，无独立顶部 Header。Account & Chats 在左，消息结果与转发在中，Query Builder 在右。账号状态默认紧凑、授权表单自动显示，完整配置通过原生 HTML dialog 打开；错误按账号、聊天、结果区域展示。
- CSS Grid + Pointer Events Splitter，Pointer Capture、取消捕获、禁止拖动选字、ARIA separator 与方向键/Home。左右最小 240/300 CSS px，中间最小 400，按实际视口约束宽度。小于 1060 CSS px 时右栏变为抽屉。比例通过 Rust 设置保存，提供重置布局。
- 全局 API/代理普通配置与 DPAPI 密文同处版本化 settings.json，但敏感输入不进入普通配置字段。读取 IPC 不解密、不返回秘密。密码字段默认隐藏，留空保留保存值；清除按钮删除保存值。
- 内部 Profile ID 在授权前生成，Ready 后 getMe 更新 Telegram 用户 ID、姓名和 username。重复 Telegram 用户给出提示，Session 目录保持独立；不自动合并或移动数据库。
- Lifecycle mutex 串行会话操作。切换先取消扫描、失效结果，再 close、等待 Closed、join 原生接收线程，随后才创建新 Gateway。关闭失败保留 Gateway 和关闭阻塞状态，用户可重试断开；不绕过原有 ACTIVE 守卫。
- Generation 隔离 TDLib update 和 getMe 保存，查询在活动 run 锁内检查 generation 和 run ID，前端过滤 run ID/会话 token。提交中或 Pending 转发阻止切换、断开和窗口退出，避免把取消历史查询当成取消发送。
- `logOut` 只用于用户确认的显式登出；它自己关闭 TDLib，等待 Closed 时不额外发 close。普通断开和切换只用 close。
- QuerySpec、Predicate、分页、Album、Null/Unknown、字段来源、排序与转发权限逻辑保持原实现。只修正一项旧测试的停滞 Mock：提供足够重试页，与已有稳定性修复一致，未修改 Query Engine。

## 关键文件

| 文件 | 变更 |
|---|---|
| `src-tauri/src/settings.rs` | 设置模型、Profile、DPAPI FFI、原子替换、Legacy 导入、目录安全与离线测试 |
| `src-tauri/src/app.rs` | 类型化设置/账号 IPC、会话串行化、epoch、Pending 跟踪、安全退出和状态测试 |
| `src-tauri/src/tdlib/gateway.rs` | 抽取等待 Closed/join，用于 logOut；原有接收线程和初始化路由保留 |
| `src-tauri/src/lib.rs` | 导出设置模块，使无 desktop feature 时也能测试 |
| `src-tauri/tests/core_regression.rs` | 旧停滞测试补足重试页；其他变更仅 rustfmt |
| `src/useWorkspace.ts` | 集中管理账号、设置、查询、会话清理、预检与发送事件 |
| `src/components/WorkspaceLayout.vue` | Grid、Splitter、键盘与窄窗口抽屉 |
| `src/components/AccountPanel.vue` / `AccountSettings.vue` / `ConfirmDialog.vue` | 账号、授权、设置与确认工作流 |
| `src/components/ChatPanel.vue` / `MessageResultsPanel.vue` / `QueryPanel.vue` | 拆分原 UI，保留真实 IPC、FilterEditor 和业务功能 |
| `src/App.vue` / `src/types.ts` / `src/workspace.css` / `src/main.ts` | 工作空间组装、DTO 与统一桌面样式 |
| `README.md` / `docs/IPC.md` / 本文 | 运行、安全、接口与验收说明 |

新增命令：`app_settings`、`save_settings`、`save_layout`、`reset_settings`、`clear_secret`、`add_account`、`rename_account`、`remove_account`、`logout_account`。`connect` 改为 Profile 激活 DTO；connection/forward-update 增加 generation，connection 增加 profile_id；新增 accounts-changed/account-notice。完整参数见 [IPC](IPC.md)。QuerySpec 和 ScanResult DTO 未改变。

## 本地目录与旧 Session

```text
%LOCALAPPDATA%\com.funare.tmsgpicker\
  settings.json                 # settings、profiles、legacy_imported、DPAPI 密文字节
  settings.json.tmp             # 原子保存过程临时文件，成功后替换正式文件
  tdlib\
    db\                         # Legacy，继续原路径
    files\                      # Legacy，继续原路径
    accounts\
      p-<内部生成 ID>\
        db\
        files\
```

首次打开设置检测旧 db 目录，不需要判断数据库内容有效性，不移动任何文件。幂等标记避免重复导入，包括移除 Legacy Profile 后也不会再次自动导入。用户必须提供现有数据库口令；不会生成新口令覆盖旧 Session。取消记住和清除凭据都不删除 Session。普通重置保留 Profile、秘密与 Session。

Windows DPAPI 使用当前用户范围与 UI_FORBIDDEN；保护失败明确报错，不支持明文降级。配置优先级为保存值优先，缺少 API 值才使用环境变量；数据库手工口令优先于保存口令。DPAPI 只保护保存副本，TDLib 初始化继续使用同一 passphrase 的 Base64 bytes。方案依据 [Microsoft CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)。登出会销毁该 TDLib 授权的本地数据，见 [TDLib logOut](https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1log_out.html)，因此界面单独确认。

## 自动化验证

- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`：4 项设置测试 + 20 项既有回归通过。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib --tests`：10 项单元/原生测试 + 20 项回归通过。
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`：通过。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`：通过。
- `pnpm.cmd build`：TypeScript 与 Vite 生产构建通过。
- `pnpm.cmd tauri build --debug --no-bundle`：Windows x64 开发版构建通过；可执行文件 `src-tauri/target/debug/tmsgpicker.exe`，所需 DLL staging 到同目录。
- `git diff --check`：通过。

设置测试覆盖保存/重新加载/重命名/重置、DPAPI 合成凭据往返与清除、损坏密文恢复、普通序列化不含秘密、未知字段兼容、损坏/未来文件拒绝覆盖、Profile 独立路径、Legacy 幂等、重复身份检测、安全删除新 Profile/拒绝删除 Legacy。状态测试覆盖扫描失效、关闭失败保持阻塞和 epoch、过期 run/epoch、不把清空查询状态当成清除 Pending。原生测试覆盖 ACTIVE 单实例、初始化队列、关闭/重建。

测试仅使用合成数据和专用临时目录，不读取用户现有数据库、不提供真实 Telegram 凭据、不发送消息。未构建 MSI/NSIS 安装包。

## 未验证与后续范围

computer-use 已尝试，但本机 native pipe 不可用（os error 2），浏览器工具列出 0 个浏览器；无法取得实际桌面截图或进行拖拽/高 DPI 自动验证。真实 Telegram 登录、A → B → A Session 恢复、Legacy 数据库口令兼容、代理网络、历史读取和真实转发尚未手工执行。构建通过不等于这些行为已验收。

后续可优化：安装包及 DLL 许可证分发、头像/媒体预览、更大量聊天分页、长期发送记录/网络异常后 Pending 恢复、多应用进程设置锁、删除后的回收/恢复工具、查询预设持久化。当前 Pending 长时间无法确认时保留进程并阻止切换/退出；没有自动丢弃 Pending 或宣称发送成功的入口。移除 Profile 但保留文件后，文件由用户保管，本轮不提供重新导入这些新 Profile 孤立目录的功能。

## 本机手工验收步骤

1. 启动 `pnpm.cmd tauri dev` 或上面的 exe。确认无独立 Header，三栏顺序正确；拖两条分界线到极限，分别聚焦按方向键和 Home。重启检查比例恢复，点重置布局检查默认值；将窗口缩至最小尺寸并开/关右侧查询抽屉。Windows 100%/150%/200% 缩放各检查一次。
2. 设置中填写自己的 API 和实际代理配置，保存；退出重启检查普通值与「已保存」状态，真实秘密不回填。查看 settings.json 确认没有可用的明文 Hash/密码。分别更新与清除秘密，禁用代理；连接期间尝试保存应提示先断开。普通重置不得改变 Session 目录。
3. 若有旧 `tdlib/db`，选择 Legacy，输入原来的数据库口令连接；确认进入 Ready 后补齐身份，普通断开再连接无需登录码。重启不重复创建 Legacy；不要为旧 Session 设置不同密码。
4. 添加 A 和 B，设置各自数据库口令并按需勾选记住，分别完成手机号/验证码/2FA。确认独立 accounts 目录。切换 A → B → A：有效 Session 无需验证码，记住口令后自动解锁，未记住时先安全断开并提示输入；聊天/结果/预检/目标不跨账号保留。
5. 查询最近 50 条真实历史，核对短页累计、Album 来源、Views/Forwards/Comments/Reactions、Null 与 0；切换 AND/OR/NOT 和 Emoji、日期 Scope、升降序与 Output Limit=1。取消大扫描后立即重查，再在扫描时切换账号，确认旧进度不覆盖新状态。
6. 打开原文，Saved Messages 解析和权限预检。只有用户自己明确同意真实发送后，手动确认一条合规转发，检查 Pending/Sent/Failed；Pending 未确认时切换、断开、关闭窗口应被阻止。自动化测试不执行本步骤的真实发送。
7. 对自己选择的测试账号，验证「断开」保留授权；单独确认「退出 Telegram 登录」后需重新授权。移除本地 Profile 默认保留文件；仅对可丢弃的新 Profile 测试二次确认删除。Legacy 不提供删除数据选项。不要在真实重要 Session 上试删除。
8. 清除所选账号保存口令，检查数据库仍在，下一次手工输入原口令可打开。合成损坏密文的恢复已由测试覆盖；无需修改真实凭据文件来制造故障。

## 后续 UI 精简

第二轮将紧凑卡片、独立 Settings Window、授权入口与透明 Splitter 接入，当前说明和 GUI 待验收项目见 [UI_REFINEMENT_ACCEPTANCE.md](UI_REFINEMENT_ACCEPTANCE.md)。本页是第一轮历史交付记录。
