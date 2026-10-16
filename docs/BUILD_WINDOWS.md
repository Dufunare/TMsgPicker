# Windows x64 源码构建与 TDLib 原生依赖

> 面向需要从源代码编译 TMsgPicker 的开发者。普通用户下载经过发布验收的 Windows 安装包/ZIP **无需**安装 CMake、gperf、MSVC、Rust、pnpm 或自行编译 TDLib。发布流程见 [RELEASE.md](RELEASE.md)，应用操作说明见 [README](../README.md)。

## 1. 固定版本与可核对的来源

TMsgPicker v0.1 的**新源码构建基线**在 [`scripts/native-versions.json`](../scripts/native-versions.json) 中集中声明：

| 依赖 | 固定版本/提交 | 用途 |
|---|---|---|
| **TDLib** | **1.8.68**，2026-10-12 更新；`f381f4b35ebfd52a086ec3af5352ed77bbdbde01` | Telegram 用户账号 JSON C API，编译 `tdjson.dll` 和 `tdjson.lib` |
| vcpkg | `e456309491fd875487bf02b8a0dca76c1b1b00cc`（2026-10-10） | 固定 OpenSSL/zlib/gperf port 构建脚本 |
| OpenSSL | `3.6.5#1` | TDLib TLS/密码学依赖，动态运行库 |
| zlib | `1.3.2#2` | TDLib 压缩依赖，动态运行库 |
| gperf | `3.3#1` | **仅构建时**运行代码生成工具 |
| CMake | 系统安装，实际版本须符合上游要求 | **仅构建时**配置与编译 |
| MSVC | Visual Studio 2022，x64 工具链和 Windows SDK | 构建 TDLib 和 Rust Tauri 应用 |

**TDLib 官方提交**：[`f381f4b`](https://github.com/tdlib/td/commit/f381f4b35ebfd52a086ec3af5352ed77bbdbde01)，提交时间 **2026-10-12 16:43:06 UTC**。该提交的 `CMakeLists.txt` 声明 `project(TDLib VERSION 1.8.68 ...)`。

上表是**当前项目锁定的目标构建版本**，并不能反推出旧电脑中手工编译的 `Release/tdjson.dll` 的真实源码提交。旧 DLL 没有随之保存来源元数据时，必须标注为“构建来源未验证”，不能冒称来自 `f381f4b`。

vcpkg 的包修订号 `#1` / `#2` 是 port-version，不等价于上游软件自身版本。MSVC、Windows SDK、CMake 和 GitHub Hosted Runner 的**补丁版本**尚未完全固定；这种“固定源码/port 基线”不意味着不同机器可以逐字节复现相同的 PE 文件。每份新发行物会记录实际 SHA-256 以供核对。

## 2. Windows 开发机前置条件

推荐 Windows 10/11 **x64**。安装：

- **Visual Studio 2022 C++ Build Tools**（或 VS 2022），包含“使用 C++ 的桌面开发”、MSVC x64、Windows SDK；
- **Git**、**CMake**，并确保 `git.exe`/`cmake.exe` 可从 PowerShell 调用；
- **Node.js 24**、**pnpm 10**、**Rust** stable x64-pc-windows-msvc（支持 edition 2024）；
- Windows WebView2 Runtime（桌面 WebView 使用）。

**无需手动安装 OpenSSL、zlib、gperf**：下面的脚本将按固定 vcpkg SHA 下载并构建。它们及 TDLib 上游完整源码**不进入本项目 Git 历史**。

使用 PowerShell（Windows PowerShell 5.1 或 PowerShell 7），从仓库根目录执行：

```powershell
git clone https://github.com/Dufunare/TMsgPicker.git
cd TMsgPicker

# 第一次构建耗时较长，且需要访问 GitHub / vcpkg 上游下载源。
.\scripts\build-tdlib.ps1 -Jobs 2
.\scripts\stage-native.ps1
```

若你的 Windows 执行策略阻止脚本，可在受信任的终端用 `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-tdlib.ps1`，而不是永久关闭系统执行策略。

### 构建脚本在做什么

1. 从 `scripts/native-versions.json` 读取 TDLib 与 vcpkg **完整 commit SHA**；
2. 在被 `.gitignore` 排除的 `native/sources/` 中检出指定 commit，并拒绝覆盖有用户改动的已有源码工作区；
3. 验证 TDLib 在 CMake 中宣称的版本，验证 vcpkg 三个 port 的精确版本/修订；
4. Bootstrap 固定 vcpkg，安装 `openssl:x64-windows`、`zlib:x64-windows`、`gperf:x64-windows`；
5. 用 Visual Studio 2022/CMake 配置 `x64`、Release，执行 `cmake --build ... --target tdjson --config Release`；将输出 `tdjson.lib`、`tdjson.dll` 和 vcpkg 的 OpenSSL/zlib DLL 汇集到 `native/tdlib/`；
6. 计算各文件 SHA-256，在 `native/tdlib/build-manifest.json` 记录准确版本/上游提交与原生文件哈希；
7. `stage-native.ps1` 重新核对 manifest 和文件哈希，复制运行库、TDLib/OpenSSL/zlib 的**实际许可声明**到 `native/release/`，生成临时 `src-tauri/tauri.release.generated.json`；
8. Release-only Tauri 配置将运行 DLL 映射到安装目录中 **`tmsgpicker.exe` 的同级**，不把它们埋在无法被 Windows loader 自动找到的资源子目录。

没有将可变的 `master`、`main`、`latest` 作为源码 pin；升级依赖应通过独立 PR 修改版本文件并验证实际产物。

### 生成目录（不提交 Git）

```text
native/
├── sources/td/                  官方 TDLib 指定提交
├── sources/vcpkg/               固定 vcpkg 提交
├── build/tdlib/                 CMake 编译目录
├── tdlib/
│   ├── tdjson.lib
│   ├── tdjson.dll
│   ├── libssl-3-x64.dll
│   ├── libcrypto-3-x64.dll
│   ├── z.dll 或 zlib1.dll
│   └── build-manifest.json
├── release/
│   ├── 运行 DLL
│   └── licenses/
│       ├── TDLib.txt
│       ├── OpenSSL.txt
│       ├── zlib.txt
│       ├── NOTICE.txt
│       └── native-dependencies.json
└── dist/                         打包产物，仅执行发布脚本后存在
```

**不要随意重命名 `z.dll` 为 `zlib1.dll`**：Windows DLL 文件名必须满足 PE import table 的真实依赖。该 vcpkg 版本有过 zlib 动态库命名变化，因此 `src-tauri/build.rs` 与分发脚本支持两种名称，并在审计时检查实际依赖。

## 3. 构建 TMsgPicker

### 本地开发

```powershell
pnpm.cmd install --frozen-lockfile
$env:TMSGPICKER_TDLIB_DIR = (Resolve-Path .\native\tdlib).Path
pnpm.cmd tauri dev
```

`TMSGPICKER_TDLIB_DIR` 指向**同时拥有导入库和运行 DLL 的目录**。`build.rs` 只负责链接与开发期复制 DLL 到 Cargo target/deps；普通 `pnpm.cmd build` 是纯前端构建。旧 `Release/` 路径继续向后兼容，但**不会自动获得新的 pinned-build 来源保证**。

### 编译用于分发的安装包

```powershell
$env:TMSGPICKER_TDLIB_DIR = (Resolve-Path .\native\tdlib).Path
pnpm.cmd tauri build --bundles nsis --config src-tauri/tauri.release.generated.json
.\scripts\package-release.ps1
```

脚本预计生成：

- `native/dist/TMsgPicker-Setup-x64.exe`（Tauri/NSIS 安装器）；
- `native/dist/TMsgPicker-Portable-x64.zip`（免安装 EXE + DLL + 许可声明 + MSVC 运行库）；
- `native/dist/SHA256SUMS.txt`（两个分发文件的 SHA-256）。

打包脚本对 NSIS 文件内容运行 7-Zip 列表检查，对便携版的各 PE 文件使用 MSVC `dumpbin /dependents` 验证依赖是否由**分发目录或 Windows System32** 满足，避免意外依赖构建机 `PATH` 中的 DLL。CI 的 ABI smoke test 还会运行独立 `tdlib_smoke`，核对 TDLib 报告的版本为 `1.8.68`，不使用真实 Telegram Session。

## 4. 为什么需要四个 DLL、导入库与两个 Windows 运行时

| 组件 | 开发构建 | 用户部署 |
|---|---|---|
| TDLib 上游源码 | 必须检出 | 不需要 |
| `tdjson.lib` | MSVC 动态导入链接需要 | 不需要 |
| `tdjson.dll` | 需要 | 必须和 EXE 同目录 |
| `libssl-3-x64.dll` / `libcrypto-3-x64.dll` | 动态 OpenSSL 依赖 | 必须（当前 x64-windows 构建） |
| `z.dll` / `zlib1.dll` | 动态 zlib 依赖 | 部署实际编译产物名 |
| CMake / gperf / MSVC / Rust / pnpm | 需要完成整个构建 | 不需要 |
| Visual C++ Runtime | 开发机通常已有 | 安装包与便携版须携带/满足相应运行库 |
| WebView2 Runtime | 桌面 GUI 使用 | 仍需系统已安装或由 NSIS 按配置引导安装 |

NSIS 的 `bundleVCRuntime` 可将 MSVC CRT DLL 同目录部署；便携版额外从受许可的 VS2022 Redist 目录收集 CRT DLL。该方式和安装 Microsoft VC++ Redistributable 具有不同的部署与许可证审查要求，应在正式发布前核对 Microsoft 的相应再分发条款。

WebView2 采用 Tauri 默认的按需下载 Bootstrapper（安装器缺少时需要网络）；**便携 ZIP 不包含 WebView2 Runtime**，如果目标电脑没有 WebView2，请按微软官方说明安装。不能把“免安装”误认为“无需任何系统先决条件”。

## 5. 版本升级、校验与常见失败

- **TDLib 升级**：修改 `scripts/native-versions.json` 中完整源码提交与 `version`，确认对应 TDLib `CMakeLists.txt`；独立构建并检查 JSON API 兼容、账号迁移/Session 恢复，再发布。
- **OpenSSL/zlib/gperf 升级**：先升级 vcpkg 完整 SHA 与期望 port 版本；如果动态依赖名称变化，要确认 `stage-native`、`build.rs`、NSIS 与 PE 导入检查仍一致。
- **CMake 找不到 gperf**：脚本应从 `installed/x64-windows/tools/gperf` 自动添加 PATH，先检查 vcpkg 安装成功。
- **构建 OpenSSL 失败**：检查 VS2022/MSVC/Windows SDK、vcpkg 工具下载和可用磁盘/内存；不要用其他系统来源的 DLL 直接替换固定产物。
- **找不到 `tdjson.lib`**：确认 CMake 真正完成 `tdjson` Release 目标，而不是只生成 CMake 项目；旧 `Release` 可以用于本地开发，但不能代替发布构建证明。
- **DLL 装载失败**：检查 EXE 同目录与导入名称；使用 `dumpbin /dependents`，不要靠 PATH 中偶然存在的开发工具 DLL 使 CI 假通过。
- **打包报资源缺失**：先运行 `stage-native.ps1` 生成 Release overlay，再运行 Tauri build，最后执行 `package-release.ps1`。
- **编译器内存不足**：`-Jobs 2` 为默认较保守的并行度，可试 `-Jobs 1`，不改变所锁定的源码版本。
- **访问 GitHub 失败**：脚本会报错退出；请在可靠网络环境重试，不自动从第三方非校验镜像替换依赖。

**安全**：构建/测试不需要 Telegram API Hash、真实手机号或 Session。`native/`、`.env`、DB 与 DLL 应保持忽略。请不要向 issue 附上真实凭据或 `%LOCALAPPDATA%` 中的 TDLib 数据。
