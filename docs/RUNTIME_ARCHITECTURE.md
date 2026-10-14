# 应用配置与常驻多账号 Runtime

## 当前账号生命周期（2026-10-14 交互重构）

本节及 [IPC](IPC.md) 描述当前实现。后面的初版 Runtime / P2 部分保留为历史设计记录；其中提前注册账号、持久化布局／Compact、混合 Settings、改名／移除 Profile 等产品行为已由本节取代，共享 Receiver 和并发保护继续沿用。

```text
AccountRegistry
  profiles              已完成 Ready + getMe 的正式账号
  primary_profile_id    主账号关联，优先合法 ID；一次兼容旧 last_profile_id
  pending_profiles      独立的中断／升级恢复元数据，不进入普通账号列表

PendingLogin（login.rs 服务 + AppState 内存所有权）
  profile_id            提前分配最终唯一 ID，不移动运行中的数据库
  phone                 仅内存，实际 WaitPhoneNumber 时提交并清空
  remember_database     首次登录的选择，成功登记时处理 DPAPI 副本
  newly_created         取消时只有本次创建的目录可清理；恢复旧记录保留数据

AccountRuntime
  原有账号级 epoch / Client / 查询 / Pending 发送 / lifecycle Mutex
  user_disconnected     用户主动 close 成功后设置，网络状态不能修改
  logout_progress       本进程确认退出／Closed 进度，失败时重试不重复 logOut

ActiveProfileId         当前窗口内的查看选择，不写配置、不影响任何 Client
```

ApplicationConfig 仅保存 API、SOCKS5 的应用级配置。旧 left_ratio、right_ratio、card_compact 被 serde 忽略；last_profile_id 只用于一次迁移读取、不再写出。主账号无效时按 created_at 选择最早登记账号（相同时间维持注册表顺序），注销主账号后自动回退；切换查看不改变主账号。旧未识别的 Profile／Legacy 移入 pending_profiles，保留目录和原凭据，getMe 成功前不显示占位卡片。

启动创建所有正式账号 Client；待恢复记录也使用独立 Runtime。保存口令可并发初始化，无口令停在 NeedsDatabasePassphrase；必须输入原密码，不能生成替代密钥或清理原数据库。设置窗口关闭不调用生命周期 API。全局配置更新仍使用全局写锁、并发 close / Closed 和并发重建，主动断开账号保持断开。

| 用户操作 | 当前流程 |
|---|---|
| 添加 | 唯一 LoginWindow → 校验全局 API → PendingLogin / 独立 Client / 最终目录 → 按真实授权状态推进 → Ready → getMe → 去重 → 原子提交身份、可选口令、正式登记 → main 选中新账号 → 关闭登录窗，Client 继续运行 |
| 取消登录 | 全局读锁 + 账号生命周期锁 → close / Closed → 本次新建目录严格清理 → 删除 Pending 元数据；旧恢复记录仅关闭并保留原数据，不 logOut |
| 重复身份 | 不晋升／不保存新口令，不修改既有账号；安全 close 冲突 Client，提示取消临时登录 |
| 切换 | 只更新前端视图；select_profile 仅验证正式 ID，不写盘，不创建／关闭 Client |
| 断开 | 原有 Pending 审阅保护 → 取消指定账号查询 → close / Closed → 标记主动断开；保留登记、Session、DPAPI 口令 |
| 恢复 | 只恢复主动断开账号；复用其原 Session，缺口令时提示原密码。启动缺密码的账号直接继续数据库初始化，不增加“解锁”菜单 |
| 注销 | UI 二次确认 + confirmed 校验 → 指定 Ready 账号 logOut → 明确 ok → Closed → 清理该账号私有数据 → 原子删除登记和口令 → 主账号回退；错误保留登记，已确认的退出进度可重试 |
| Debug 全部重置 | Debug 编译限定 + 两次确认／RESET TMSGPICKER → 禁止新任务、取消查询、等待正在提交的转发 → Pending 等待／Unknown 审阅 → 注册表全部关联账号 logOut（主动断开账号必要时恢复原 Session／口令）／Closed → 已授权 Pending Client 注销，未授权者 close／Closed → 全部成功才严格清理固定应用专属目录 → 成功退出；失败保留必要数据和逐账号进度供重试 |

注销使用 services::LogoutProgress。明确 ok 只记录进程内请求接受状态；收到真实 Closed 后才持久化 Profile.logout_completed，重启发现完成标志进入 LogoutCleanupRequired，不重新打开原 Session。旧 logout_pending 不足以证明完成，不自动进入已完成状态。共享接收器按 Client／tag 缓存迟到的注销回执及 LoggingOut 证据，即使请求等待已经超时，重试也观察同一 Client，不重复发 logOut 或追加 close。明确拒绝时提交门恢复 Accepting；接收故障可恢复同一共享接收线程观察真实关闭。安全退出也会记录之前已经发起的注销完成进度，不新发注销请求。全局重建跳过已确认注销账号；清理或持久化失败明确报告。

reset.rs 提供可由 Mock 实现的两阶段协调器：先从注册表枚举全部正式和待恢复记录，逐一完成注销／未授权客户端关闭并汇总结果；任一失败均不能执行最终清理。主动断开不是跳过条件，缺原口令、原数据库、有效授权或网络必须恢复后重试。完成的注销持久化保留登记，失败的 Session／凭据不由应用删除。全部确认完成后才清理 ApplicationConfig、AccountRegistry、全部 DPAPI 凭据及应用专属目录；成功退出避免运行期旧状态残留。

managed_data.rs 统一固定所有者清理：检查绝对路径、禁止 ParentDir、检查所有祖先及整棵目录树，不跟随符号链接／Windows Reparse Point，清理后验证确实不存在。Windows 使用 OPEN_REPARSE_POINT 句柄检查节点，锁定祖先和当前节点禁止重命名／替换，并通过已验证的句柄删除，防止检查后路径被 Junction 替换。普通 delete_session 仍拒绝 Legacy；注销 Legacy 使用专用 cleanup_logged_out_session，仅删除 tdlib/db、tdlib/files，保护 tdlib/accounts。完整 Debug Reset 固定比对 LOCALAPPDATA/com.funare.tmsgpicker，不接受前端路径；开始删除后若失败，继续拒绝新账号任务／配置写入，只允许重试重置或安全退出。测试只使用 Mock 和独立临时合成目录。

Vue 职责分离为 AccountPanel、AccountSettings、ApplicationSettings、LoginWizard；useLogin 独立管理临时输入与授权状态，useApplicationSettings 只管理全局配置，useWorkspace / accountViews 保留账号视图和消息任务。账号设置以 account-<ID> 窗口固定绑定 Profile；主窗口切换不会重新定向，注销后关闭绑定窗或显示失效。

Windows 原生标题栏和三个辅助窗口共用 src/theme.json。window_theme.rs 使用 COLORREF = R | G<<8 | B<<16，设置 DWM 35（caption）、34（无边框描画）、36（text），不支持时保持原生降级。未引入自绘标题栏。详见 [Microsoft DWM 枚举文档](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)。

验证范围与仍需人工验收的行为见 [当前交付记录](ACCOUNT_LIFECYCLE_ACCEPTANCE.md)。

## 初版 Runtime 设计记录

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
    forwarding: bool, pending: Map<target_and_temporary_id, PendingSend>,
    sent_updates: Map<target_and_temporary_id, state>,
    unknown_sends: Vec<UnknownSend>, // 本进程内历史，跨断开／重建保留
    unlocked_passphrase: Option<String>, lifecycle: Mutex<()>,
}
PendingSend { started_at: Instant } // 单调时钟，不持久化
UnknownSend { profile_id, generation, target, old_message_id, state: "Unknown" }
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

每个 ClientSlot 额外共享提交锁与 `Accepting / Closing / ReceiverFailed` 状态。enqueue 在同一锁内登记 Router 并发送；close／logOut 在同一锁内先转为 Closing、再发送终止请求。因此 close 之后、真实 Closed 之前的新业务也会被拒绝。close 幂等，logOut 已提交后不追加 close；该提交锁不限制其他 Client 并发。

接收线程使用 Rust unwind 捕获及退出守卫。异常终止先封闭所有旧 Router，立即错误完成全部 Pending，再逐账号通知 Runtime Error；通知回调异常也不会阻止其他账号收到故障。等待 Closed 的线程立即获知故障，不伪造原生 Closed，也不打开同一 Session。用户重试断开时，先 join 故障线程，再启动唯一的接收线程继续观察原 Client Map 的关闭；旧 Router 保持封闭，禁止新业务和新 Client，直到全部旧 Client 真正 Closed。原生 Closed 内部清理先于外部回调，回调异常不能阻止安全关闭记录。此机制处理 Rust panic／线程异常返回，不声称恢复原生访问违规、abort 或强制杀进程。

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

未完成授权的 Profile 可保留后续继续。Pending 转发保护只限制影响该客户端生命周期的操作，切换视图不受此限制。断开、全局重建和退出先检查 Pending：每项至少等待 30 秒后，可让用户明确确认将未确认发送标为 Unknown，再 close。关闭失败仍保留 Gateway，不强开原目录。移除和注销仍等待最终发送状态。

`close_guard` 返回数量、剩余等待毫秒和可选确认令牌，不改变 Client。令牌绑定 Profile、epoch、目标／临时消息集合，5 分钟有效且只能用一次。执行关闭前，在发送更新／Pending 锁下统一校验所有账号；新增 Pending、换代、过期令牌或仍在提交转发均拒绝，避免部分账号先被错误标记。确认后记录 Unknown 并按 Profile 广播；不推断失败或送达、不自动重试。主窗口关闭请求通过仅发给 main 的事件显示确认，设置窗口不争抢确认。取消不修改 Client、配置或未保存输入。Unknown 历史跨本进程的断开／重建保留，旧 Client 的明确回执可修正；退出后记录不持久化，确认框明确提醒核对 Telegram。

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

Store 的公开修改方法创建私有暂存对象；校验／保护秘密／修改元数据只作用于暂存对象。三个文件先全部写入并 flush 到临时文件，然后写 `storage-transaction.json`（三个固定路径的旧字节，秘密为 DPAPI 密文），逐个原子替换正式文件，最后删除日志作为提交点。全部成功后才发布新内存。任一步失败恢复整组旧文件；恢复失败保留日志并明确拒绝后续写入。启动先验证并恢复未完成日志，再加载配置，不能读取半次提交。清除代理密码及用户名在同一事务中完成。该日志协调多文件提交，不宣称三个文件的文件系统级同时替换或断电硬件保证。

迁移仍保留原始 settings.json，不反复读取旧快照。旧 Profile 补齐目录标识；已有 DPAPI 密文原样迁入，Legacy 口令仍对应 `db:legacy`；Base64 参数规则不变。Legacy 导入标记即使移除 Legacy 登记也保留，避免重启再次导入留下的旧目录。损坏存储不静默重置。没有在开发期间操作真实 appdata 或删除任何现有 Session／数据库／凭据。

## 自动化验证与人工验收

测试均使用人工 fixture、mock、临时目录和未初始化原生 Client；不使用 Telegram 凭据、不读取真实 Session、不发送真实消息。结果及命令见 [本轮验收记录](MVP_ACCEPTANCE.md#2026-10-14-常驻多账号-runtime-重构)。

1. 正常关闭旧开发程序，再运行新程序；已有 Legacy 应只登记一次，原数据库路径及内容不变，输入原口令可复用授权。
2. 准备 A、B 两个不同账号；为 A 记住口令，为 B 不记住。退出并重开后 A 自行恢复，B 显示需要解锁，A 可继续操作；B 解锁前不得创建替代数据库或改变口令。
3. A 开始历史查询，切到 B 开始另一个查询，再切回 A；A 的聊天、条件、扫描进度、结果和选择恢复，两个账号均保持连接。切换过程中不出现断开／重新登录。
4. 两个账号分别等待授权输入时，不自动打开争抢焦点的窗口；只在选择对应账号后手动打开 Settings，并按各自实际步骤提交。
5. 断开 A 后 B 的聊天和查询仍可使用；恢复 A 复用原 Session。普通应用退出后再开，不要求重新登录 Telegram。
6. 修改全局代理或 API，观察全部 Client 安全关闭并恢复；一个账号口令／网络失败不影响其他账号状态。有 Pending 时先等待明确回执，或按超时后的 Unknown 确认流程关闭。
7. 添加并登录重复 Telegram 用户，确认冲突 Profile 断开并明确提示，旧 Profile／凭据／目录保持完整；默认移除冲突条目保留目录。
8. 只有明确点击并确认“退出 Telegram 登录”才验证注销；再次使用该 Profile 按 TDLib 要求重新授权。默认移除账号不注销，Legacy 删除选项不可用。
9. 查看布局拖动、Compact 偏好和独立 Settings 关闭行为；真实原文链接、受保护转发预检和 Pending → Sent／Failed 需由用户在可控目标上确认。

尚未人工验证：真实多账号登录、SOCKS5 连通性、已有加密 Session 复用、真实 getMe 去重、真实发送状态及 WebView2 视觉交互；构建和离线测试不能替代这些步骤。安装包 MSI／NSIS 分发仍不在本轮验收范围。

## 2026-10-14 P2 修复补充验收

本轮基于用户合并并微调 UI 后的本地 main。构建产物使用独立 target，未停止现有应用、未操作真实 Session、未推送或合并。自动结果见 [最新修复记录](MVP_ACCEPTANCE.md#2026-10-14-p2-稳定性修复)。

1. 在用户明确允许发送的可控目标上造成 Pending，立即断开／保存全局配置／关闭主窗口，应提示剩余等待时间，且不关闭任何 Client。等待每项至少 30 秒后重试；取消确认应保留连接和未保存的设置输入。
2. 超时后确认断开 A，A 的发送显示 Unknown，B 继续运行。重连 A 或全局重建后仍可查看本进程 Unknown 记录。先在 Telegram 核对实际结果，不据 Unknown 自动重发。
3. 全局配置／退出确认期间若后台又出现新的 Pending，操作应拒绝并要求重新审阅；不能沿用旧令牌。退出确认只在主窗口出现。确认退出后重开仍复用登录，未执行 logOut。
4. 在独立临时数据目录中测试写入权限／文件占用失败，确认设置错误后内存快照及磁盘仍是旧值；修复权限后可重试。不得在真实 Session 上破坏配置来测试恢复。
5. 接收线程 panic、Pending 立即失败、仅为关闭恢复接收线程及并发关闭由原生自动测试覆盖；真实 DLL 进程崩溃无法通过 Rust 捕获恢复。真实断网、长 Pending、DPAPI 故障及 WebView2 确认窗口仍需本机手工验收。
