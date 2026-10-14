# 应用配置与常驻多账号 Runtime

2026-10-14。替代此前“一个活动 Gateway、切换时关闭旧账号”的运行模型。开工分支 `review/stability-final-pass-20261013`，工作区干净；实现基于该本地分支，未拉取旧 main 覆盖，也未推送／合并。三栏布局、独立 Settings、Splitter、业务算法保持原实现。

## 模块和所有权

```text
Application / AppState
  ├─ Store（持久化服务协调入口，Mutex 保护）
  │   ├─ ApplicationConfigService → 唯一全局配置
  │   ├─ AccountRegistry → 持久化 Profile / Legacy 导入标记
  │   └─ AccountCredentialStore → 全局秘密与 db:<profile_id> 的 DPAPI 密文
  └─ TdlibRuntimeManager → HashMap<Profile ID, Arc<AccountRuntime>>
      ├─ Profile A → Client A / 查询 / 转发 / 授权 / 网络状态
      └─ Profile B → Client B / 查询 / 转发 / 授权 / 网络状态

进程级 SharedReceiver
  └─ Client ID → ClientSlot {专属 Router, Closed 条件变量, 更新回调}
      └─ Router：request tag → Pending response
```

`settings.rs` 定义配置、账号、凭据和非破坏迁移；`app.rs` 管理 Profile Runtime、生命周期和显式账号 IPC；`tdlib/gateway.rs` 实现共享接收器和 Client 专属请求视图；`gateway.rs` 的 `TelegramGateway` 保留业务边界，新增 `SessionLifecycle` 可通过 mock 验证关闭／注销语义。`services.rs` 保留授权／聊天／动作算法，新增关闭语义组合。`core.rs` 无改动。

全局生命周期 `RwLock`：普通业务持读锁，可跨账号同时运行；同账号生命周期由自己的 Mutex 串行化。应用退出和全局配置重建持写锁。扫描工作使用账号级取消标志和 run ID。更新回调不取生命周期锁，不会被等待 Closed 的线程阻塞。

前端 `accountViews.ts` 以 Profile ID 保存视图和状态，`useWorkspace.ts` 的 `profileId` 即 activeProfileId；组件通过 computed 绑定所选视图。IPC 的调用捕获发起账号，响应写回其原视图，避免切换后的异步响应写入别的账号。

## 最终数据结构

以下是实际结构的语义摘要；锁和 Arc 省略。`Settings`／`Profile` 保留为兼容类型别名。

```rust
ApplicationConfig {
    schema_version: u32, api_id: Option<i32>,
    proxy_enabled: bool, proxy_server: String, proxy_port: u16,
    proxy_username: String, last_profile_id: Option<String>,
    left_ratio: f64, right_ratio: f64, card_compact: bool,
}
AccountProfile {
    profile_id: String, telegram_user_id: Option<String>,
    display_name: String, username: Option<String>,
    is_legacy_profile: bool, last_used_at: u64,
    session_directory_id: String, created_at: u64,
}
AccountRegistry { profiles: Vec<AccountProfile>, legacy_imported: bool }
AccountCredentials { // 临时解锁视图，无 Serialize，无读取 IPC
    profile_id: String, session_root: PathBuf,
    remember_database: bool, passphrase: Option<String>,
}
AccountRuntime {
    profile_id: String, epoch: u64, gateway: Option<Arc<TdlibGateway>>,
    connection: Connection, connecting: bool, closing: bool,
    active: Option<(run_id, cancel_flag)>, results: Option<ScanResult>,
    forwarding: bool, pending: Map<target_and_temporary_id, state>,
    sent_updates: Map<target_and_temporary_id, state>,
    unlocked_passphrase: Option<String>, lifecycle: Mutex<()>,
}
Connection {
    profile_id: Option<String>, generation: u64,
    runtime_state: String, auth_state: String,
    connection_state: String, detail: String,
}
```

Telegram ID 为十进制字符串。时间为 Unix 秒；保留旧 `last_used_at=0` 表示尚未使用，旧 Profile 缺失的 `created_at=0` 表示创建时间未知。Session 标识由应用生成，新 Profile 的标识等于其 Profile ID，Legacy 为 `legacy`。API Hash／代理密码不存在于 Profile，客户端 ID 只保存在当前进程的 Gateway 和接收器映射中。

ApplicationConfigService 验证／更新唯一配置，Store 协调持久化；账号代码通过 Store 读取全局初始化配置，不保存账号级 API／代理副本。数据库口令以 `db:<profile_id>` 为凭据键，“是否记住”由对应 DPAPI 密文是否存在决定。Snapshot 只返回 `has_saved_*`。本进程成功解锁的口令可用于全局重建；显式断开／注销后清除这个内存副本。验证码、邮箱验证码、2FA 不保存。

## 状态机对照

| 旧状态／行为 | 新状态／行为 |
|---|---|
| AppState 的唯一 profile / gateway / connection | Profile 注册表与 Runtime Map 分别管理，任意多个 Runtime 同时存在 |
| 连接状态决定“是否有账号” | Profile 是否登记只由注册表决定 |
| 输入口令后才创建唯一客户端 | 全部 Profile 先建 Client；无口令停在 NeedsDatabasePassphrase |
| 授权和网络状态集中在活动账号 | 每个 Connection 独立保存 runtime_state、auth_state、connection_state |
| 切换时取消查询、关闭旧 Gateway | 仅改变 Profile 视图；原账号查询继续 |
| 全局 generation 排斥其他客户端回调 | 每个 Runtime 独立 epoch；事件带 Profile ID 和 generation |
| 一个账号关闭使 receive loop 退出 | 只移除该 ClientSlot；全部 Closed 后才停止并 join 接收器 |

`runtime_state` 使用 NeedsDatabasePassphrase、Initializing、AuthorizationRequired、Ready、Closing、Closed、Error；`auth_state` 保留 TDLib 实际类型名。Ready 表示已授权，网络是否可用仍由 `connection_state` 判断。初始化失败保留单账号 Error 和具体脱敏原因，后续授权等待更新不会覆盖这个原因。关闭超时保留 Gateway 并拒绝在同一目录重开，用户可重试断开。

## 共享接收器

现代 JSON ABI 的 [官方声明](https://github.com/tdlib/td/blob/master/td/telegram/td_json_client.h) 定义了统一 `td_receive` 事件流、`@client_id`、`@extra` 和返回指针生命周期。

`OnceLock<Mutex<Option<SharedReceiver>>>` 串行创建／停止共享线程。创建 Client 与注册 ClientSlot 持同一个映射锁，不丢失早期更新。唯一线程调用 `td_receive(0.1)`，立即复制 C 字符串到 Rust owned String，再解析并按 Client ID 取 ClientSlot，随后按 `@extra` 分发专属 Router。这样两级键等价于 `(client_id, request_tag)`，即使 tag 相同也不跨 Client 匹配。更新在接收线程内按收到的顺序处理，昂贵的 getMe 工作另启后台任务并验证 epoch。

Router 支持多个 Pending，Drop／超时／取消清理自己的请求。Client Closed 时封闭自己的 Router、失败自己的 Pending、移除自己的 Slot，再通知 Closed；不影响其他 Router。未知／已移除 Client 消息直接丢弃。重建使用新的 Client ID 和 epoch。最后一个 Client 的 wait_closed 检查映射为空，停止并 join 接收线程；join 与新建 Client 共享管理锁，避免新旧接收循环重叠。同步 execute 也立即复制返回指针；桌面接收器运行前才执行日志设置。

## 调用流程

| 操作 | 实际流程 |
|---|---|
| 启动 | 打开全局服务 → 迁移旧设置／导入 Legacy → 遍历注册表建立全部 Runtime／Client → 发送首个授权状态请求以启用更新 → 各线程读取各账号口令并初始化；缺失口令不提交参数；一个失败不停止其他账号 |
| 添加 | 持久化新 Profile／独立目录标识 → 创建 Client → 保持需要口令 → 用户在 Settings 输入新口令 → 初始化 → 按 TDLib 实际状态提交手机号／代码／邮箱／2FA → Ready 后 getMe 更新身份 |
| 解锁 | 验证 Profile → 取该 Profile 的手工／保存口令 → 对已有未初始化 Client 提交参数；Closed Client 可新建；失败不生成新密钥／删除数据库 |
| 初始化 | 向同一 Client 提交 addProxy 或 disableProxy → 立即提交 setTdlibParameters → 先等参数、再等代理确认 → 成功后才按用户选项保存口令 |
| 重复身份 | 原子检查注册表 → 不写入重复身份 → 安全断开冲突 Profile → 带 Profile ID 提示显式移除；不注销、不合并、不覆盖原账号 |
| 切换 | 更新前端 profileId → 恢复其缓存视图 → select_profile 只保存上次查看偏好；不执行 Client 生命周期操作 |
| 断开 | 锁定指定 Runtime → 检查其 Pending 转发 → 取消该账号扫描 → close → wait Closed → 清理该 Gateway／内存口令；其他账号持续运行 |
| 注销 | UI 确认和后端 confirmed 校验 → 指定账号必须已授权 → logOut → 等 TDLib 自行 Closed；不紧接着发 close |
| 移除 | UI 确认 → 安全关闭指定 Runtime → 默认只移除登记和保存口令，保留目录；删除文件另需二次确认和所有权／重解析点检查，Legacy 禁止删除 |
| 全局设置 | 验证新配置 → 对全部 Client 提交 close → 并发等 Closed → 统一保存配置 → 并发重建全部 Profile；内存／DPAPI 口令可用则恢复，否则 Locked；逐账号报告失败 |
| 退出应用 | 标记 exiting 拒绝新任务 → 等正在提交的业务释放全局读锁 → 检查 Pending 发送 → 全部提交 close → 并发等待 Closed（共享约 20 秒等待窗口）→ 清理 Pending／join 共享接收器 → 退出；失败保留进程供重试 |

未完成授权的 Profile 可保留后续继续。Pending 转发保护只限制影响该客户端生命周期的操作，切换视图不受此限制；全局重建／应用退出需要等待全部 Pending 的最终发送状态。

## 持久化和兼容

```text
%LOCALAPPDATA%/com.funare.tmsgpicker/
  app-config.json          # 全局非敏感设置
  accounts.json            # Profile 元数据、导入标记
  credential-store.json    # DPAPI 密文；秘密不回填前端
  settings.json            # 如存在，保留旧版本原始备份
  tdlib/db, tdlib/files     # Legacy 原路径
  tdlib/accounts/<profile>/db, files
```

迁移时先写凭据和注册表，最后写 app-config 作为完成标记。此后读取新文件，避免重复读取旧快照。旧 Profile 补齐目录标识；已有 DPAPI 密文原样迁入，Legacy 口令仍对应 `db:legacy`，Base64 参数规则不变。Legacy 导入标记即使移除 Legacy 登记也保留，避免重启再次导入留下的旧目录。损坏存储不静默重置；各文件独立 flush／原子替换。没有在开发期间操作真实 appdata 或删除任何现有 Session／数据库／凭据。

## 自动化验证与人工验收

测试均使用人工 fixture、mock、临时目录和未初始化原生 Client；不使用 Telegram 凭据、不读取真实 Session、不发送真实消息。结果及命令见 [本轮验收记录](MVP_ACCEPTANCE.md#2026-10-14-常驻多账号-runtime-重构)。

1. 正常关闭旧开发程序，再运行新程序；已有 Legacy 应只登记一次，原数据库路径及内容不变，输入原口令可复用授权。
2. 准备 A、B 两个不同账号；为 A 记住口令，为 B 不记住。退出并重开后 A 自行恢复，B 显示需要解锁，A 可继续操作；B 解锁前不得创建替代数据库或改变口令。
3. A 开始历史查询，切到 B 开始另一个查询，再切回 A；A 的聊天、条件、扫描进度、结果和选择恢复，两个账号均保持连接。切换过程中不出现断开／重新登录。
4. 两个账号分别等待授权输入时，不自动打开争抢焦点的窗口；只在选择对应账号后手动打开 Settings，并按各自实际步骤提交。
5. 断开 A 后 B 的聊天和查询仍可使用；恢复 A 复用原 Session。普通应用退出后再开，不要求重新登录 Telegram。
6. 修改全局代理或 API，观察全部 Client 安全关闭并恢复；一个账号口令／网络失败不影响其他账号状态。等待 Pending 转发完成后再修改。
7. 添加并登录重复 Telegram 用户，确认冲突 Profile 断开并明确提示，旧 Profile／凭据／目录保持完整；默认移除冲突条目保留目录。
8. 只有明确点击并确认“退出 Telegram 登录”才验证注销；再次使用该 Profile 按 TDLib 要求重新授权。默认移除账号不注销，Legacy 删除选项不可用。
9. 查看布局拖动、Compact 偏好和独立 Settings 关闭行为；真实原文链接、受保护转发预检和 Pending → Sent／Failed 需由用户在可控目标上确认。

尚未人工验证：真实多账号登录、SOCKS5 连通性、已有加密 Session 复用、真实 getMe 去重、真实发送状态及 WebView2 视觉交互；构建和离线测试不能替代这些步骤。安装包 MSI／NSIS 分发仍不在本轮验收范围。
