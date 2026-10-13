# v0.1 实现与验收记录

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
| Native / 授权 | 原生 FFI ABI、唯一接收器、并发路由、取消/超时清理、错误脱敏、Closed 后停止并可重建；手机号/代码/2FA/邮箱 UI；代理响应先于 TDLib 参数 | 真实 API 初始化、SOCKS5 连通、第一次账号登录、已有加密 Session 复用及 GUI 退出 |
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
4. **Session/代理**：沿用已验证 CLI 的本地 appdata 路径和 Base64 bytes 参数。每次启动输入数据库密码，不存前端或仓库。代理在 setTdlibParameters 前等待成功，没有配置时禁用旧代理。
5. **动作**：确认前展示允许/拒绝项，提交时复核；只正常转发，Pending 不冒充送达，关注 TDLib 发送成功/失败更新。
6. **后续项目**：媒体缩略图/播放器、聊天列表加载更多、发送报告持久化、自动重试、指标实时全局重排、MSI/NSIS 分发均未实现。首版媒体类型占位、最多 100 项聊天、单聊天/内存扫描满足当前范围；界面编辑一个排序字段，核心接受最多 10 字段。

本机最短验收流程详见 [README](../README.md) 的“检查与手动验收”。用户自行确认真实发送后再验收转发；本次开发没有替用户执行该动作。

TDLib 方法参数核对来源为 [官方 td_api.tl](https://github.com/tdlib/td/blob/master/td/generate/scheme/td_api.tl)，结合本地已验证 harness 的新式 addProxy 形状；本地运行时仍应以实际 DLL 的行为为准。
