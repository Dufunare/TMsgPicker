# Windows Release 流程与验收清单

> 本文是**发布操作手册**，与 [README](../README.md)（用户使用）、[ARCHITECTURE.md](ARCHITECTURE.md)（设计决策）、[projects.md](projects.md)（源码导览）和 [Devlog.md](Devlog.md)（时间线）互补。源码依赖构建见 [BUILD_WINDOWS.md](BUILD_WINDOWS.md)。

## 1. v0.2.0 发布约定

- 发布平台：**Windows x64**，提供 **NSIS 安装器、Portable ZIP、完整构建产物合集以及依赖清单**。暂不做 MSI、macOS/Linux、自动更新。
- Telegram API：**只允许用户自行配置 ID/Hash**，发布流水线不注入开发者或其他人的 API 凭据，源码/二进制不带内置默认 API。
- 应用源码、脚本和版本锁定文件可公开；`native/` 里检出的 TDLib/vcpkg 源码与本地编译二进制**不提交 Git**。
- GitHub `vX.Y.Z` 标签必须与 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `package.json` 中的版本一致。发布意图记录在 `scripts/release-intent.json`。
- 发布是**两段式**：CI 成功后自动创建或更新 **Draft + Prerelease**，由维护者完成干净机核验后手动公开。没有人工作为最后一道确认时，不把 CI “构建成功”解释为“可用安装器已经交付给用户”。

## 2. 发版前置条件与供应链

版本集中在 [`scripts/native-versions.json`](../scripts/native-versions.json)，截至 2026-10-16：

| 依赖 | 发布基线 |
|---|---|
| TDLib | **1.8.68**，`f381f4b35ebfd52a086ec3af5352ed77bbdbde01`（2026-10-12） |
| vcpkg | `e456309491fd875487bf02b8a0dca76c1b1b00cc` |
| OpenSSL | `3.6.5#1` |
| zlib | `1.3.2#2` |
| gperf（build only） | `3.3#1` |

构建程序检查完整 commit SHA，校验 CMake 声明的 TDLib 版本、port 修订、构建产物 SHA-256，并在 ZIP 的 `licenses/native-dependencies.json` 中保存构建声明。**不能证明此前手动放在 `Release/` 的旧 DLL 与此基线一致**。

vcpkg 依赖端口定义受固定提交约束，但 Windows runner、MSVC、CMake、Rust/Node/GitHub Actions 版本仍会逐步变化；本方案不承诺 bit-for-bit 重现，产物检查依赖 SHA-256 与实际干净机验证。后续可考虑 pin runner image/工具链或保存 SLSA provenance/签名 attestations。

## 3. CI 发布流水线与打标签

v0.2.0 使用**显式的发布意图文件** [`scripts/release-intent.json`](../scripts/release-intent.json)。在 `main` 更新版本号和该文件并提交后，`Windows x64 Release` 工作流将：

1. 核验三个应用版本与意图文件、固定原生依赖、PowerShell 脚本；
2. 使用 [`docs/assets/tmsgpicker-icon.svg`](assets/tmsgpicker-icon.svg) 生成所有 Tauri 平台图标；
3. 如果图标生成发生改变，先把生成文件提交回 `main`；无法推送则拒绝发版；
4. 完成前端、Rust 离线单测、固定 TDLib 构建、NSIS、ABI Smoke Test、依赖审计和 SHA256；
5. CI 成功后创建 **Draft / Prerelease**，保留产物来源 SHA；维护者完成本地验收后在本地创建指向该 SHA 的 annotated tag 并推送；Tag Push 将触发发布现有 Draft；
6. 将安装器、便携 ZIP、完整 Build Artifacts ZIP、来源清单和校验文件都上传到 GitHub Releases 的 **Assets**。

> 未通过构建时不会创建新标签。已有标签不能重新指向其他提交；同版本重复发布应报错并另起补丁版本。Draft 不等于公开发布。

触发方式：只在 `main` 的 `scripts/release-intent.json` 内容变化时自动执行；旧的 `v*` tag 推送和 `workflow_dispatch` 仍可执行构建，但无需再次提交图标。为避免标签越过未经验证的源码，正式流程应以 main 的发布意图提交触发。

| 文件 | 说明 |
|---|---|
| [`.github/workflows/release.yml`](../.github/workflows/release.yml) | Windows x64 CI / 生成图标 / 打标签 / Draft Release |
| [`scripts/native-versions.json`](../scripts/native-versions.json) | TDLib/vcpkg/OpenSSL/zlib/gperf 精确基线 |
| [`scripts/build-tdlib.ps1`](../scripts/build-tdlib.ps1) | 原生依赖固定构建 |
| [`scripts/stage-native.ps1`](../scripts/stage-native.ps1) | DLL、许可证、来源哈希和 Tauri overlay |
| [`scripts/package-release.ps1`](../scripts/package-release.ps1) | NSIS 和 Portable 校验、打包 |
| [`CHANGELOG.md`](../CHANGELOG.md) | 每版主要变化 |

PR 仍以 `frontend-ui.yml` 和 Release 合成文件测试检查；无真实 Telegram Session、API Hash 或 2FA 密钥参与 CI。人工验收仍按第 6 节。

## 4. 发行物组成

| 资产 | 目标用户 | 内容 |
|---|---|---|
| `TMsgPicker-Setup-x64.exe` | 普通 Windows 用户 | Tauri NSIS 安装、`tmsgpicker.exe`、TDLib/OpenSSL/zlib DLL、**MIT 与第三方许可声明**、MSVC CRT、WebView2 按需安装引导 |
| `TMsgPicker-Portable-x64.zip` | 希望免安装的用户、测试人员 | `TMsgPicker/` 目录内 EXE + 原生 DLL + MSVC x64 CRT + `licenses/` |
| `TMsgPicker-Windows-x64-Artifacts.zip` | 完整 CI 构建包 | 安装器、Portable ZIP、依赖清单（方便整套下载） |
| `native-dependencies.json` | 原生来源证明 | 固定的 TDLib/vcpkg 及编译输入 |
| `SHA256SUMS.txt` | 校验文件完整性 | 四个 Assets 的 SHA-256（从构建输出计算） |

便携版的典型目录：

```text
TMsgPicker/
  tmsgpicker.exe
  tdjson.dll
  libssl-3-x64.dll
  libcrypto-3-x64.dll
  z.dll  （或按真实 PE imports 为 zlib1.dll）
  msvcp140*.dll、vcruntime140*.dll 等 MSVC CRT
  licenses/
    TMsgPicker-MIT.txt
    TDLib.txt
    OpenSSL.txt
    zlib.txt
    NOTICE.txt
    native-dependencies.json
```

**DLL 必须位于 EXE 同目录**，尤其是从 `tdjson.lib` 动态链接的 `tdjson.dll`。`src-tauri/build.rs` 在开发期复制 DLL 到 target **不代表安装器自动打包**；发布使用另行生成的 `src-tauri/tauri.release.generated.json`（Tauri `bundle.resources` 将资源直接映射到 Windows 安装目录的可加载位置）。生成配置和 `native/` 一样被 `.gitignore` 排除，不改变日常 `tauri.conf.json` 开发行为。

WebView2：安装器按 Tauri 默认 `downloadBootstrapper` 在系统缺少时下载；**离线安装或 ZIP 不是 WebView2 完全自带版本**，需要已有 WebView2 Runtime。Portable **免安装应用文件**，但仍使用 `%LOCALAPPDATA%\com.funare.tmsgpicker` 保存用户的 TDLib Session 和设置，**不是数据库随 ZIP 迁移的真正数据便携模式**。

## 5. 自动质量门槛与实际含义

| CI 步骤 | 能证明 | 不能证明 |
|---|---|---|
| 前端测试、类型检查和 Rust Mock 回归 | 编译/单测和基本领域语义 | Windows GUI 的真实交互 |
| TDLib/vcpkg 固定源码、版本和哈希验证 | **新生成文件**来自指定构建输入，避免意外替换 | 工具链和二进制位级再现 |
| `tdlib_smoke` | TDLib 本机 DLL/FFI 基础加载、Client 授权状态生命周期、版本号 | 真实手机号授权、SOCKS5、Session/转发 |
| NSIS `7z l -slt` 审计 | 安装器归档中有预期 DLL 和许可证 | 安装路径实际写入/卸载无误 |
| Portable `dumpbin /dependents` | 分发文件的直接 PE 导入可由 ZIP/Windows 系统库满足，不靠开发环境 PATH | 所有动态加载插件、所有 OS 版本上的运行情况 |
| SHA256SUMS | 用户下载文件与发布产物哈希一致 | 二进制无恶意代码或已获得可信签名 |

Release 默认**没有代码签名**，可能出现 SmartScreen 声誉警告。不要将“禁用杀毒软件”“点击忽略所有安全警告”当作安装步骤。后续优先增加可信签名/时间戳和签名验证，而不是加壳/反调试。

## 6. 人工验收清单（公开 Draft 前必须完成）

### A. 干净机安装与启动

- [ ] 从 Draft Assets 取得 Setup、ZIP 和 SHA256SUMS，核对 SHA-256；
- [ ] 在干净 Windows 10/11 x64 VM 测试 **没有 CMake、vcpkg、Rust、Node 或 Visual Studio** 时的运行能力；
- [ ] 安装 NSIS，检查安装目录的 EXE/DLL 位置、开始菜单和卸载入口；
- [ ] 启动并确认 TDLib DLL、VC++ CRT、WebView2 全部正常加载；
- [ ] 在没有 WebView2 的 Windows VM 测试联网引导；离线情形给用户明确说明；
- [ ] 对 Portable ZIP 解压并直接运行，确认不靠用户 PATH 中的开发版 DLL；
- [ ] 运行构建 ABI smoke（无真实 Telegram 凭据）；核对 TDLib version = 1.8.68。

### B. 用户设置、账号和数据安全

- [ ] 没有预置 API，首次运行清楚引导用户到 `my.telegram.org` 申请 ID/Hash；
- [ ] 用户自行配置 API 与 SOCKS5 后，在自愿/受控账号上进行 QR 扫码和手机号验证码/2FA 登录、会话复用和主动断开/恢复；
- [ ] 两个账号并存、切换前台账号不破坏其他后台 Client 与数据；
- [ ] 测试运行中的应用退出、重新启动后的既有 Session 可恢复；
- [ ] 以旧版本已经存在的账号数据库升级安装，确认配置、DPAPI 凭据、Session 不被误删；
- [ ] NSIS 升级与卸载不执行 Telegram `logOut`，不把 `%LOCALAPPDATA%` 当作程序安装目录清理；
- [ ] 在 VM 中核验升级回滚/失败后的可恢复性；不要使用生产账号测试破坏性 Debug Reset；
- [ ] 帮助说明不暗示提供 Telegram 官方客户端或内置 API Hash。

### C. 实际功能、体积与发行说明

- [ ] 完成真实 Post 检索、跨页 Album、日期范围、空值和媒体原图加载的手工核对；
- [ ] 较大频道扫描时观察 CPU/RSS/网络/磁盘峰值与 `BudgetExceeded`、取消行为；
- [ ] 安装器/ZIP 不携带真实日志、手机号、`.env`、Session、DPAPI 存储或测试账号；
- [ ] 逐一检查 TDLib/OpenSSL/zlib 的实际分发许可及 copyright notices；
- [ ] **确认构建基于合入 LICENSE 后的新提交**，安装包与便携版均包含与仓库根目录一致的 `licenses/TMsgPicker-MIT.txt`；
- [ ] 检查 Visual Studio VC++ 可再分发组件许可证、Telegram API Terms/隐私与商标表述；
- [ ] 检查 Draft release notes、下载哈希、来源提交、已知限制及无签名说明；
- [ ] 通过以上人工门槛后，在 GitHub Releases 将 Draft **手动发布**；记录验证设备/Windows 版本与结果。

用户数据完整性高于安装便捷。**正式发布时更新/卸载不应静默注销账号或删除 TDLib 本地数据**。本项目目前未实现自动更新；未来加入更新器前，需要先建立签名、迁移和回滚测试。

## 7. 许可证与公开仓库

- **TDLib**：Boost Software License 1.0；**OpenSSL 3**：Apache-2.0；**zlib**：zlib License。脚本从**实际检出或安装的包**中收集对应许可文件，避免手工维护易失真的副本。
- **gperf**：编译期工具，不包含在最终 ZIP/Setup 中；构建环境仍需遵守其 GPL 许可，不要把可执行程序当作本项目发行物分发。
- **Visual C++ Runtime**：属于微软可再分发组件，随应用部署的权利及限制应遵循 Visual Studio 许可条款。
- **TMsgPicker 本身**：项目权利人已选择 **MIT License**，根目录 [`LICENSE`](../LICENSE) 使用 `Copyright (c) 2026 Dufunare`。源码和分发包应保留该版权及授权声明。`licenses/TMsgPicker-MIT.txt` 是它的分发副本；并不替代第三方依赖许可。
- 未使用 Telegram 品牌暗示官方背书；应持续核对适用的 [Telegram API 条款](https://core.telegram.org/api/terms) 和隐私要求。

## 8. 发布后的响应与下一阶段

v0.1.x 优先稳定 NSIS/ZIP、真实登录/代理/Session 与 DLL 包装；保持 BYO API 和人工发布确认。收集崩溃/升级/安装反馈时**不要要求用户上传真实 Session、API Hash、验证码或消息历史**。

后续可考虑：签名与 SmartScreen 声誉、自动升级（必须首先保证数据库迁移与回滚）、MSI、Windows ARM64、依赖构建缓存/文件证明、真实 GUI/干净机自动化。上述是**规划方向而非当前已提供功能**。

## 9. Tag 与 Release 一一对应（v0.2.0 起）

当前仓库的 GitHub Actions `GITHUB_TOKEN` 不允许创建包含 workflow 文件的 Git Tag（HTTP 403）。因此采用 **CI 产出 Draft → 维护者验收 → 本地创建并推送 Tag → Actions 发布已有 Draft** 的流程。

- Windows 构建成功时创建 Draft Release 和 Assets，但不创建 Tag。
- 本地测试通过后，维护者在自己的 Git 工作区创建 **annotated tag**，指向实际构建的 Commit，而非当前最新的 main。
- 推送 `v0.2.0` 后触发 `.github/workflows/publish-release.yml`：验证构建、Tag SHA、唯一 Draft Release、五个 Assets，再将已有 Draft 发布为正式 Latest 版本；不会创建新的 Release，也不重新构建安装器。
- Git Tag 永远不能重指向其他提交；发布之后需要修复则递增 patch 版本。
- 保留 `v0.1.0` Tag 和其历史 Draft Release；它们按版本号配对。如果以后决定只保留公开的正式版本，可单独处置旧 Draft 和旧 Tag，必须成对操作。

**v0.2.0 在本地验收后执行：**

```powershell
git fetch origin
git tag -a v0.2.0 229fd56cc630ea1092eea12e633a81b9311553f0 -m "TMsgPicker v0.2.0"
git push origin refs/tags/v0.2.0
```

若已存在同名 Tag，停止操作并核对它的提交，不使用 `--force`。
