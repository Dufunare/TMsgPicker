# TMsgPicker 开发日志（Devlog）

> **定位**：只记录重要的功能里程碑、架构变动、被取代的决策、测试边界和待办事项。不再承担现行产品说明、IPC 参考、源码目录或用户操作手册的职责。  
> **当前基线**：截至 **2026-10-16** 的 `main`。现行文档以 [README](../README.md)、[ARCHITECTURE.md](ARCHITECTURE.md)、[projects.md](projects.md) 为准；本日志中的旧行为**不代表现在仍然有效**。

## 2026-10-13：v0.1 最初实现与 Windows Bring-up

**工作重点**

- 选定 `Tauri 2 + Vue 3/TypeScript + Rust + TDLib JSON C API`，围绕 Windows x64 的本地消息查询工具而非完整 Telegram 客户端开发。
- 打通 `tdjson.dll`、MSVC `tdjson.lib` 与 OpenSSL/zlib 动态库的本地开发构建；保留 `tdlib_smoke` / `tdlib_auth` 诊断入口。
- 初步实现 TelegramMessage 归一化、媒体组组织、嵌套过滤 AST、null/Unknown 语义、历史游标分页、结果排序与消息动作服务。
- 完成早期账号登录、SOCKS5、Session 与多栏 UI，使用 Mock 进行核心、服务及生命周期测试。

**当时的约束**：设计与自动化覆盖了大量边界，但并未等价于真实账号、真实网络及 Windows WebView GUI 的完整验收。早期文档对单活动 Client、部分字段、原始消息预算等表达在后续迭代已被替换。

**历史入口**：早期 `docs/TMsgPicker_PROJECT_SPEC.md`、`docs/MVP_ACCEPTANCE.md` 可通过 Git 历史查看，不再作为当前产品规范。

## 2026-10-14：多账号 Runtime、登录与持久化稳定化

**架构与安全**

- 从旧的“查看账号改变当前唯一 Client”走向**AccountRegistry + 每账号独立 AccountRuntime / TDLib Client**，同时保留进程级唯一 `td_receive` 循环。
- 为账号增加独立 epoch/generation、网络/授权状态、扫描取消和 Pending 发送归属；切换前台账号不再关闭其他 Client。
- 按“已完成 Ready + `getMe`”认定正式账号；未完成登录和 Legacy Session 独立记录、按需恢复。
- 全局配置、账号登记与 DPAPI 密文分开存放，并通过协调写入/回滚恢复保证跨文件一致性。
- 断开连接定义为 `close` 并保留 Session；注销定义为 `logOut` → 确认应答/真实 Closed → 定向清理；延迟回执、重复账号与关闭故障按状态处理。
- 建立 Pending/Unknown 发送保护；危险的“完整重置”限于 Debug 构建，并为全部账号的注销和文件所有权验证设置阶段性约束。

**界面**

- 明确区分唯一登录窗口、唯一设置窗口与主窗口；设置窗口收敛为应用/账号/外观独立子页面。
- 首次登录手机号、验证码与可选 2FA 的状态驱动流程；第一次设置本地数据库密码并可选择是否用 DPAPI 记住。
- 左栏不创建占位账号；主账号与正在查看的账号分别处理；旧的本地 Profile 改名/移除、布局持久化和 Compact 卡片开关逐步取消。

**证据与边界**：当时保存过 Mock 单测、Rust 检查、前端构建和部分 Windows 原生调试构建记录。它们不是所有真实 Telegram 账号、代理、注销/发送路径都已手动通过的声明。

**历史入口**：`docs/RUNTIME_ARCHITECTURE.md`、`docs/ACCOUNT_LIFECYCLE_ACCEPTANCE.md`、`docs/UI_ACCOUNTS_ACCEPTANCE.md` 与 `docs/UI_REFINEMENT_ACCEPTANCE.md` 的原始版本可从 Git 历史获取。注意其中有互相先后取代的 UI 行为。

## 2026-10-15：结果展示、交互与外观完善

**聊天与消息**

- 聊天列表区分**全部 / 普通 / 归档**；TDLib `getChats(chatListMain/chatListArchive)` 成为列表成员归属判断依据；加入迟到请求防污染与去重。
- 结果卡片优先呈现文字、可点击链接、互动统计和可展开的媒体组；增加聊天/账号头像及微型图回退。
- 引入安全 URL/`textUrl` 处理，修复汉字与标点边界、Telegram 隐藏链接解析。
- 持续完善缩略图、原图、视频封面和媒体打开交互，同时设置下载大小与会话隔离限制。
- 逐步明确“以 Post 查询而不是对媒体组每条消息分别查询”的语义，调整 Album 指标规则与内容类型存在/不存在谓词。

**外观与窗口**

- 统一主题色、圆角、间距与卡片视觉，加入液态玻璃、可调缩略图尺寸、日期时间弹层和分段选择器动效。
- 设置窗口外观预览与原生 Windows DWM 标题栏同步；保留对旧系统的降级行为。
- 消息面板简化后移除了早期的完整批量转发控件，相关后端权限检查与发送状态机保留；用户使用手册必须区分当前可操作 UI 和未开放的后端能力。

**相关历史提交**

- [`93283d2` — 最近 N 逻辑 Post 与媒体组边界修正](https://github.com/Dufunare/TMsgPicker/commit/93283d288)
- [`1019a80` — 保存 Telegram 隐藏文本链接](https://github.com/Dufunare/TMsgPicker/commit/1019a80bf7)
- [`7c1ed92` — 统一窗口外观相关 PR 合并](https://github.com/Dufunare/TMsgPicker/commit/7c1ed925fc)

旧 `docs/UI_THEME_SYSTEM.md`、`docs/WINDOW_CHROME.md` 的有效设计要点已归入现行架构/源码导览；旧文件不继续作为平行规范维护。

## 2026-10-16：Post Scan Engine V2 与性能优化

### 多账号设置的目标安全

- 修正设置窗口在前台切换账号时可能错误绑定当前操作账号的风险。
- 显式捕获设置目标，旧异步结果不能重新定向破坏性操作。

提交：[`5176c827` — PR #8 合并](https://github.com/Dufunare/TMsgPicker/commit/5176c827408ccaee548100c48c1dde9e35a6e1cc)

### Post Scan Engine V2

**不变量重构**

- `QuerySpec` 升级为 **version 2**，最近 N 与日期内最新 N **统一以 Post 计数**；日期还支持尝试扫描整个时间窗口。
- 扫描范围先于过滤，`output_limit` 仅限制最后的保留结果；内部原始消息数/页数/时间分别设安全额度。
- 历史读取器、跨分页 Album 累积器、边界判定器、Post 范围控制器被拆分为 `scan/*` 模块。
- `Post.date = max(member.date)`；浏览/转发取成员最大有效值，评论/反应来自确定的主消息。
- **扫描覆盖**（Complete、Interrupted、BudgetExceeded 等）与**Post 成员完整性**（Verified、Assumed、Truncated、Unknown）分离；Album 缺总成员数证据时不能直接叫 Verified。
- 切换聊天立即使旧查询与结果失效，旧 run/event 无法混入新聊天；未知/截断 Post 的转发权限检查仍在后端执行。

提交：[`e89f355` — PR #10 合并](https://github.com/Dufunare/TMsgPicker/commit/e89f355819ec07240185a421c8c7d02da15c73d3)

### 性能、媒体生命周期与分页体验

- 将重复的全量筛选/排序进度处理改成**增量 Top-K**，每次只保留输出上限范围内的高排名 Post；进度事件最多附 40 个 Post。
- WebView 结果每页 40 张 Post 卡片，扫描快照作为不可变对象减少深层响应式代理。
- 提供带优先级与并发限额的媒体调度器，按账号/generation/文件 ID 去重，支持多组件独立取消和缩略图/原图双 LRU。
- **展开媒体组自动加载照片原图**：先显示微型图/缩略图，原图到达后替换；收起再展开可复用有界缓存，不再逐张点击。
- 聊天详情使用有界 TDLib 请求批次，减少串行往返；切换聊天释放 Rust 侧旧查询结果，较重的查询/释放操作改为后台执行。
- 将上一页/下一页按钮移到结果列表底部；切页后滚回中间栏顶部，界面动画和 OS 减少动态效果决定平滑或立即跳转。
- 明确记录还存在的潜在瓶颈：单账号同步 TDLib 下载、Base64 IPC、最终一次性返回大量 Post、TDLib 磁盘缓存清理及真实环境性能曲线缺失。

**相关提交/合并**

- [`5297979` — Top-K 与响应式结果分页](https://github.com/Dufunare/TMsgPicker/commit/529797984)
- [`f1b1cca` — 媒体自动原图、去重/缓存与后台命令](https://github.com/Dufunare/TMsgPicker/commit/f1b1ccaed8bfa6012c3f216209751366052e2c52)
- [`a447f782` — 底部翻页回顶部及动画偏好](https://github.com/Dufunare/TMsgPicker/commit/a447f7823c48c625844e98c5d5937e210c8bee9f)
- [`766942e2` — 性能分支合入 main（PR #12）](https://github.com/Dufunare/TMsgPicker/commit/766942e2eeb3f203253c7f0d762bde1cf44c987a)

## 被后续版本替换的早期决策

以下仅为历史对照，**不要按旧列操作现版本**。

| 早期方案/描述 | 截至 2026-10-16 的现行方案 |
|---|---|
| “最近 N 条”有时按原始 Telegram 消息理解 | 最近 N **LogicalPost**，Album 是一个 Post |
| 日期 Scope 带原始消息 `budget` | QuerySpec V2：`limit_posts`（Post 数）或 `null`（范围内全部）；原始消息限制是内部资源预算 |
| 任意 Album 一律使扫描整体 `Partial` | ScanStatus 与 PostCompleteness 分离；连续 Album 常为 Assumed |
| 图片原图需要逐张手工点击 | 展开媒体组后自动请求照片原图，失败保留预览 |
| 所有结果卡片一次性挂载 | 每页最多 40 个 Post，分页在列表底部 |
| 单活动 Client、切换账号时关闭旧 Client | 全部正式账号各有 Client；切换仅改变查看视图 |
| 提前注册未完成授权的账号、显示占位 Profile | 正式 Profile 必须 Ready + getMe；PendingLogin/Legacy 独立保留 |
| 账号设置与添加登录入口混在一起 | 独立登录窗口、统一设置窗口的三个子页面 |
| 永久保存栏宽与 Compact 用户卡片 | 栏宽只在当前窗口内存保留；当前账号卡片不提供该开关 |
| 当前 UI 已有完整批量转发按钮 | 后端动作管线存在，但主结果面板暂不提供完整批量发送 UI |

## 当前版本的验证边界与后续问题

**已经建立的自动化基础**：前端 Node/Vue 测试与 TypeScript/Vite 构建、Rust 纯核心/Mock/数据生命周期回归、GitHub Actions 上的 Windows Rust 离线构建/Clippy。Mock 消息、Mock Client 与临时测试目录不应包含真实 Telegram 凭据或历史数据。

**不能仅凭这些自动化测试声称完成**：真实 Windows 多账号并行使用和长期性能测试、全部实际 Telegram 授权类型、特殊代理环境、真实转发/消息发送回执、VC++/WebView2/TDLib DLL 安装部署或 NSIS/MSI/ZIP 的干净系统验收。

**适合后续单独跟踪的方向**：

1. 正式的 Windows Release 构建与 DLL/许可证封装（仍由用户自行提供 API ID/Hash）。
2. 用真实设备压力场景确认大查询、复杂 Album 和原图加载的 FPS、CPU、RSS/缓存上限与取消体验。
3. 考察是否需要将完整最终 ScanResult 改成按 run ID 分页提取，以及减少 Base64 媒体 IPC。
4. 制定 TDLib 官方支持的磁盘文件缓存管理方案，禁止直接删除正在运行的数据库目录。
5. 如果再次开放批量转发 UI，必须与当前后端 run/账号/不完整 Post 和 Pending/Unknown 保护语义一致。

## 日志维护约定

- 新功能、重要修复或破坏性语义变更按日期**追加**，注明重要提交/PR 和验证范围。
- 用户操作步骤写入 [README](../README.md)，当前分层与安全决策写入 [ARCHITECTURE.md](ARCHITECTURE.md)，文件职责写入 [projects.md](projects.md)；本日志只记录**为什么改变、改变了什么、哪些还未验证**。
- 不在 Devlog 中复制全部测试输出、重复大段源码、个人路径、凭据、Session、手机号或真实消息。
- 被本次整理移除的早期规格/验收文件，内容仍存在于 Git 历史，必要时可以按历史提交检索。删除旧的当前文档入口**不等于抹去版本历史**。

## 2026-10-16：v0.1 原生依赖锁定与发布机制准备

- 明确 TDLib 版本：**1.8.68，2026-10-12 16:43 UTC，commit `f381f4b35ebfd52a086ec3af5352ed77bbdbde01`**。选择固定 commit 而非 `master`/浮动 tag，避免后续上游变更改变 ABI 或构建产物。
- 固定 vcpkg 来源和 OpenSSL/zlib/gperf port 修订；通过 PowerShell 构建脚本检出依赖，编译 tdjson，并记录原生 DLL 哈希和实际许可证。已有旧 Release DLL 的来源**没有由此得到证明**。
- 添加 NSIS 安装器与免安装 ZIP staging、PE 依赖审计、MSVC CRT、SHA256SUMS；原生运行库明确位于 EXE 同级，避免“Cargo 能运行而安装包缺 DLL”。
- 增加 GitHub Actions：PR 解析脚本、手动触发构建资产、发布标签创建 **Draft/Prerelease**，不自动向公众发布或做 Telegram 账号真实操作。
- 继续采用 **BYO API**，**不嵌入任何开发者共享 API ID/Hash**，构建过程也不需要 Telegram 凭据。
- 尚未完成：Windows GitHub Hosted Runner 上的真实 TDLib 编译与安装器产物验证；干净 Windows VM 的升级/卸载/Session 不丢失手动测试；项目权利人的 `LICENSE` 选择；代码签名。

当前构建文档：[BUILD_WINDOWS.md](BUILD_WINDOWS.md)；发布验收：[RELEASE.md](RELEASE.md)。后续若改变 TDLib pin 或发布机制，请同时更新这两份文档与 [ARCHITECTURE.md](ARCHITECTURE.md)，不要在 Devlog 中再次复制完整使用手册。
