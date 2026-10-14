# 第二轮 UI 精简与布局修正

日期：2026-10-13。基于 `f0127a9`，开始时工作区干净。保留 TDLib Gateway、Request Router、Session 目录、DPAPI、Query Engine、Album、分页和转发服务；只修改必要的前端、窗口生命周期和 UI 偏好。

## 已实现

- 左上 User Card 默认外部高度约 76 CSS px；头像使用显示名首字母，展示姓名、username 和简短网络/授权状态。Compact 外部高度约 48 px，保留头像、名称、状态点、添加、切换、更多与独立齿轮。底部小箭头或更多菜单控制收缩，160ms 动画，尊重 reduced motion；`card_compact` 保存在原普通配置内，旧配置缺失字段默认为展开。
- 添加、切换和更多操作使用一致的 SVG 线条图标。更多菜单组织连接/恢复、断开、重命名、确认登出和移除。点击外部或 Escape 关闭菜单。原确认与 Pending 保护继续生效；普通切换仍不调用 logOut。
- 主工作空间没有品牌大标题或 Header。复杂说明和授权输入不在左栏；账号错误以简短提示显示，Tooltip 包含详细错误，并提供 Settings 入口。
- 独立 Rust `WebviewWindowBuilder` 创建 label=settings 的本地页面 `index.html?view=settings`，尺寸 500 × 580，最小 420 × 440，禁用最大化，内部滚动。独立 mutex 避免重复创建；再次点击解除最小化并聚焦已有窗口，不覆盖相同账号的未提交输入。
- Settings 包含「账号与授权」「API 与代理」「安全与界面」。保留 API、SOCKS5、数据库口令、remember/clear、账号管理、普通重置和布局重置。授权状态自动打开/聚焦该窗口的授权界面，主卡片同步显示状态。关闭小窗不提交或存储输入框里的秘密。
- CloseRequested 统一路由只对 main 调用安全退出。Settings 关闭不触及 Gateway、query、generation 或 Pending；主窗口退出仍保留既有会话安全关闭与发送保护，也不会因 Settings 存在而仅关闭主窗口、留下孤立小窗。
- 分界线保留独立 Grid 6px 命中区，不覆盖按钮。默认仅极淡的 1px 线，无粗色条或手柄；Hover、键盘焦点、Drag 显示半透明 1px 强调线。保留 Pointer Capture、文字选择抑制、最小宽度、方向键/Home、窄窗口抽屉；在释放/取消指针后保存比例，键盘调整也保存。
- Chat 顶部搜索、刷新和公开 username 解析固定，只有列表滚动；左侧整栏不再随配置内容滚动。新会话进入 Ready 后自动刷新聊天；前端继续用会话 token 排除旧异步结果。
- 两窗通过非敏感刷新通知更新 Profile/偏好，连接继续使用后端 connection generation。API 设置提交前重新读取最新普通配置，避免覆盖另一窗口刚改的栏宽。秘密读取协议不变，无解密秘密回传或浏览器持久化。

## 主要文件

| 文件 | 变化 |
|---|---|
| `src/components/AccountPanel.vue` / `UiIcon.vue` | 紧凑卡片、图标、下拉选择和账号菜单 |
| `src/SettingsPage.vue` / `src/components/AccountSettings.vue` / `src/main.ts` | 独立设置页面入口、配置/口令/授权分组 |
| `src/components/ChatPanel.vue` | 固定工具栏与独立滚动列表 |
| `src/components/WorkspaceLayout.vue` / `src/workspace.css` / `src/App.vue` | 透明 Splitter、释放后保存、卡片尺寸与统一样式 |
| `src/useWorkspace.ts` / `src/types.ts` | 窗口上下文、跨窗刷新、偏好和自动授权/聊天处理 |
| `src-tauri/src/app.rs` | 固定窗口 IPC、单窗口创建锁、CloseRequested 路由测试 |
| `src-tauri/src/settings.rs` | 向后兼容 card_compact 字段与保留既有配置的测试 |
| `src-tauri/capabilities/settings.json` | Settings 本地 core event capability |
| `README.md` / `docs/IPC.md` | 当前入口、窗口 IPC、事件与生命周期说明 |

## 验证结果

- `pnpm.cmd build`：TypeScript/Vite 通过。
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`：通过。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib --tests`：12 项单元/原生测试及 20 项业务回归通过，共 32 项。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`：通过。
- `pnpm.cmd tauri build --debug --no-bundle`：Windows 开发 exe 构建通过，产物 `src-tauri/target/debug/tmsgpicker.exe`。
- `git diff --check`：通过。

新增回归通过生产使用的窗口关闭路由证明 settings/other 不执行 main 的退出回调，main 会执行；配置测试证明旧文件缺失 card_compact 时按默认读取，保存偏好后保留账号和已调整栏宽。原有 DPAPI、Session、查询、权限预检与 Mock 转发测试继续通过。没有使用真实账号或真实发送。

已尝试 computer-use GUI 工具，但 native pipe 不可用（os error 2）。因此实际 WebView2 窗口创建、重复聚焦、Settings 关闭后持续扫描、拖拽/菜单/动画与 Windows DPI 尚未交互式验证。测试路由与成功构建不能替代这部分 GUI 验收；真实 Telegram 登录与 A/B 恢复也未执行。不发布安装包、不自动提交、推送或合并。

## 本机快速验收

1. `pnpm.cmd tauri dev`，或启动上面的 exe。确认直接进入三栏，左上没有品牌标题，默认卡片约 76px；点底部小箭头变为约 48px，搜索位置正常，重启保留折叠偏好。
2. 点击加号、账号下拉和更多菜单，确认入口可用。新账号口令与授权只在小窗；已有有效 Session 的切换不触发 logOut。卡片名称、username、状态跟随活动会话。
3. 连续点击卡片外的齿轮，确认只有一个 500 × 580 Settings 窗口；最小化后再次点击应恢复聚焦。同账号重复聚焦不清空未提交输入。
4. 在安全与界面重置布局，在主窗口拖两个边界、用方向键调整并重启验证比例；默认边界应很淡，Hover 只出现细线，拖动不选择文本。检查极限宽度、窄窗口抽屉及 100%/150%/200% DPI。
5. 自行连接账号，开始足够长的历史扫描；打开并关闭 Settings 的系统 X，再检查主窗口仍在、连接 Ready、扫描继续。重新打开小窗后检查授权状态同步；主窗口系统 X 仍应安全退出整个应用。
6. 检查 Settings 中 API/代理保存、秘密清除与数据库 remember/clear；API/代理修改仍须先断开。打开 Settings 后在主窗拖宽度，再保存 API 设置，重启应保留最新宽度。旧 Profile、凭据和 Session 不应被自动重置。
7. 查询最近 N/日期、AND/OR/NOT、Emoji、排序、Album、取消与原文；准备转发并检查预检/确认仍可用。本轮无需真实发送；不要把自动化回归当成真实 Telegram 验收。
