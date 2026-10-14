# 账号交互与设置职责交付记录

2026-10-14，基于当前本地代码。开工 Git 工作区干净；未推送、合并、停止用户程序或访问真实 Session。

## 交付行为

- 无正式账号时不创建默认 Profile，不显示头像、账号选择或更多菜单；左栏只保留添加账号引导与应用设置齿轮。
- AccountRegistry 的 profiles 仅有 Ready + getMe 成功的账号；pending_profiles 单独保存中断恢复元数据，手机号、验证码、2FA 不写盘。旧未识别 Profile 和 Legacy 保留目录／凭据并移入恢复区。
- 主账号保存于注册表；合法主账号优先，缺失时一次兼容合法旧 last_profile_id，否则按最早登记顺序回退。查看账号不写盘，不改变主账号，不调用 Client 生命周期。启动仍创建所有正式账号 Runtime；无保存口令的账号等待原数据库密码。
- 所有添加按钮调用同一 open_login 服务；唯一 LoginWindow 根据真实 TDLib 状态推进。Ready 后 getMe 去重，身份与首次登录选择的 DPAPI 口令原子提交；成功 main 切换账号并加载聊天，Client 继续运行。取消只 close，等待 Closed 后清理本次新建目录；取消旧记录恢复保留原数据。
- 更多菜单仅断开／恢复（互斥）与账号设置。主动断开标志独立于网络状态；普通网络断线仍由 TDLib 重连。
- ApplicationSettings 只含 API、SOCKS5、全局凭据清除和 Debug 工具。AccountSettings 固定绑定 Profile ID，只管理只读身份、状态、断开／恢复／注销／添加账号；原 Session 需要密码或确实失效时在此继续恢复／授权。设置关闭不关闭 Client。
- 注销只作用于指定账号：二次确认、logOut 明确回执及真实 Closed、持久化 logout_completed、专属 Session 清理、删除登记／口令。拒绝、超时或清理失败保留登记；超时继续观察同一 Client 的迟到回执／LoggingOut 和真实 Closed，不重复发 logOut 或追加 close。完成标志仅在 Closed 后保存，旧 logout_pending 不作为完成证据；重启和全局重建不会重新打开已完成注销的 Session。
- 移除改名、移除本地 Profile、独立解锁菜单、Compact 设置和布局持久化 IPC。拖动、方向键与 Home 保留；每次主窗口启动使用左 23% / 右 29%。旧字段忽略，账号／秘密不丢失。
- Debug Reset 现定义为“注销全部 Telegram Session + 清除全部本地数据 + 恢复首次启动状态”。两次确认并输入 RESET TMSGPICKER，先审阅清理类别及 Pending 发送，禁止新任务、取消查询、等待转发提交；注册表全部关联账号（含主动断开者）都须 logOut／真实 Closed。必要时恢复原 Session／口令，不能用 close 代替；已授权 Pending Client 也注销，未授权者可 close。部分失败保留恢复所需数据、显示逐账号结果、记录已完成注销供重试。全部完成后才检查并清理固定应用目录，包含 Legacy、DPAPI 凭据及缓存；成功退出后重启，失败不得提示成功。Release 从编译中排除危险命令及实现。
- theme.json 由 Vue 与 Rust 共用；主窗口、两个设置类型及 LoginWindow 使用同一背景，Windows 11 DWM 原生 caption / border / text 使用正确 COLORREF，保留旧系统原生降级。

## 修改范围

| 范围 | 文件 |
|---|---|
| 主界面与窗口入口 | src/App.vue、src/main.ts、src/SettingsPage.vue、src/components/AccountPanel.vue、src/components/ChatPanel.vue |
| 独立登录 | src/LoginPage.vue、src/components/LoginWizard.vue、src/useLogin.ts、src-tauri/src/login.rs |
| 设置与确认 | src/components/ApplicationSettings.vue、src/components/AccountSettings.vue、src/useApplicationSettings.ts、src/components/ConfirmDialog.vue |
| 视图与布局 | src/useWorkspace.ts、src/accountViews.ts、src/types.ts、src/components/WorkspaceLayout.vue、src/workspace.css、src/base.css |
| 账号生命周期与持久化 | src-tauri/src/app.rs、src-tauri/src/settings.rs、src-tauri/src/services.rs、src-tauri/src/tdlib/gateway.rs、src-tauri/src/lib.rs |
| 严格目录清理与窗口主题 | src-tauri/src/managed_data.rs、src-tauri/src/window_theme.rs、src/theme.json、src-tauri/capabilities/settings.json |
| 回归测试 | tests/account_ui.test.ts、tests/workspace.test.ts、src-tauri/tests/core_regression.rs，以及 Rust 模块内测试 |
| 文档 | README.md、docs/IPC.md、docs/RUNTIME_ARCHITECTURE.md、本文件 |

## 自动化验证

本次重置修订测试只使用 Vue 内存渲染／SSR、Mock TDLib、合成接收事件／凭据及独立临时目录。所有 native TDLib 测试均跳过，不执行真实 logOut，不删除用户真实本地数据。

| 命令 | 结果 |
|---|---|
| pnpm.cmd test | 15 项通过 |
| pnpm.cmd build | TypeScript 与 Vite 构建通过 |
| cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib --tests | 23 项核心回归通过；重置及目录单元测试另见下一行 |
| cargo test --manifest-path src-tauri/Cargo.toml --lib -- --skip native_ | 42 个单元测试通过，跳过 2 个 native 测试 |
| cargo check --manifest-path src-tauri/Cargo.toml --all-targets | 通过 |
| cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings | 通过 |
| cargo check --manifest-path src-tauri/Cargo.toml --release --lib | 通过；验证 Debug Reset 的编译条件排除 |
| 完整可执行文件构建 | 现有 Debug exe 被占用，本次未替换运行程序；使用库测试完成验证 |

关键回归覆盖：多账号（含 Legacy、主动断开账号）全部 logOut／Closed 后才全量清理；授权有效的 Pending Client 注销；网络失败保留恢复所需 Session／凭据、重试跳过已完成账号；缺口令和 Closed 超时阻止清理；迟到回执按 Client 隔离且不重复注销／追加 close；旧请求接受标志不作为完成证据；文件占用／删除失败明确返回未完成，释放后可重试；清理后的首次启动空状态；Junction／越界路径拒绝；两次确认及短语匹配、Release 入口排除、逐账号错误展示，以及原有 Pending → Unknown 和视图隔离保护。

## 尚需人工验证

未启动该构建连接用户真实数据。Windows WebView2 下的窗口居中、重复点击聚焦、意外关闭、三栏拖动、DWM 标题栏颜色／边框、不同 Windows 版本降级和系统按钮需本机 GUI 验收。真实手机号、验证码、2FA／邮箱授权、SOCKS5 连通性、有效 Session 重用、实际 getMe 去重和发送回执仍需在用户可控账号／目标上验证。

真实注销与完整重置均为破坏性人工操作；本轮未执行。自动化只验证 Mock 生命周期及临时合成目录。正常退出旧程序后，可通过 pnpm.cmd tauri dev 启动修订代码，或重新构建后启动新的 exe；不要同时让两个应用实例打开同一 Session。MSI / NSIS 安装包未制作。
