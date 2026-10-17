<div align="center">

<img src="docs/assets/tmsgpicker-icon.svg" alt="TMsgPicker icon — messages and filter" width="116" height="116">

# TMsgPicker

**把 Telegram 消息整理成可搜索、可筛选的 Post。**

*A local-first Telegram post browser and visual query builder.*

[![Version](https://img.shields.io/badge/version-v0.2.0-3059df)](https://github.com/Dufunare/TMsgPicker/releases)
[![Windows](https://img.shields.io/badge/platform-Windows%20x64-1765d7)](docs/BUILD_WINDOWS.md)
[![Frontend UI](https://github.com/Dufunare/TMsgPicker/actions/workflows/frontend-ui.yml/badge.svg)](https://github.com/Dufunare/TMsgPicker/actions/workflows/frontend-ui.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-228a7a)](LICENSE)

[**下载 Release**](https://github.com/Dufunare/TMsgPicker/releases) · [快速开始](#快速开始) · [完整使用指南](docs/USER_GUIDE.md) · [构建说明](docs/BUILD_WINDOWS.md)

</div>

---

TMsgPicker 通过 **TDLib** 使用你自己的 Telegram 账号访问历史消息，把相册消息合并成 `Post`，并提供可视化条件编辑器，让你根据浏览、转发、评论、Reaction、媒体类型和时间范围快速找到目标内容。

它是专注于**浏览、检索与整理**的独立桌面工具，而不是 Telegram 官方客户端、Bot 或 Mini App。

## 为什么使用 TMsgPicker？

| | 功能 | 说明 |
|:--:|---|---|
| 💬 | **Post 优先** | 单消息 / 媒体组统一检索，媒体组不被拆成零散结果 |
| 🔎 | **可视化 Query Builder** | 按浏览量、转发数、评论、指定 Emoji 反应、内容类型组合过滤，支持 AND / OR / NOT |
| 🖼️ | **媒体与指标预览** | 文本、链接、图片/缩略图和 Post 级互动指标，单媒体悬停可查看该条消息的浏览和转发 |
| 🔐 | **账号与 Session** | 独立多账号会话、**QR 扫码登录**、手机号验证码、Telegram 2FA、已有本地数据库恢复 |
| 🖥️ | **本地优先** | Rust + TDLib 在本机处理查询，无本项目业务服务器；可选全局 SOCKS5 代理 |
| 🎛️ | **桌面界面** | Tauri 2 + Vue 3，三栏布局、可调整栏宽、主题及 Windows Glass 风格 |

> **当前范围**：Windows x64 预览版。尚不提供完整 Telegram 聊天客户端、全量持续索引、完整视频播放器或批量转发的图形操作入口；具体限制见[使用指南](docs/USER_GUIDE.md)。

## 快速开始

### 1. 获取程序

从 [GitHub Releases](https://github.com/Dufunare/TMsgPicker/releases) 获取对应版本的 Windows x64 构建：

| 文件 | 用途 |
|---|---|
| `TMsgPicker-Setup-x64.exe` | Windows NSIS 安装器 |
| `TMsgPicker-Portable-x64.zip` | 解压即用的便携版，含必要的原生 DLL 与许可证 |
| `TMsgPicker-Windows-x64-Artifacts.zip` | 将安装器、便携版和依赖来源清单打包在一起的 **Release 构建产物合集** |
| `SHA256SUMS.txt` | 上述构建文件和依赖清单的 SHA-256 |
| `native-dependencies.json` | 本次构建使用的原生依赖来源信息 |

Release 首先生成 **Draft / Prerelease**，须人工验收后才公开发布。未签名的安装包可能触发 Windows SmartScreen 提示；不要绕过自己不信任的安全警告。便携版依赖系统中的 **WebView2 Runtime**。

### 2. 准备 Telegram API 凭据

TMsgPicker **不内置共享 API ID / API Hash**。请前往 [my.telegram.org](https://my.telegram.org) 的 **API development tools**，使用自己的账号创建 API 应用，然后在 **应用设置 → API 与代理** 中填写凭据。

按网络环境需要，可在同一页面配置 SOCKS5 代理。API Hash 和代理密码保存在本地，并使用 Windows DPAPI 保护。

### 3. 添加账号

点击 **添加账号**，选择任一授权方式：

- **QR 扫码**：设置本地 Session 数据库密码 → 开始登录 → 用已登录的 Telegram 手机端在 **设置 → 设备 → 连接桌面设备** 扫码并确认。
- **手机号**：输入含国际区号的手机号 → 接收并填写 Telegram 登录验证码；如果验证码迟迟不到，可尝试 QR 扫码。
- 如果 Telegram 账号启用了 **2FA**，按提示输入 Telegram 两步验证密码。

**本地数据库密码 ≠ Telegram 2FA 密码。** 数据库密码用于保护与恢复该账号在本机的 TDLib Session；忘记它可能导致本地 Session 无法恢复。可选择使用 Windows DPAPI 记住本设备。

### 4. 组合一个查询

在左侧选定聊天，右侧设置扫描范围与条件，中间查看结果。例如：

```text
最近 500 个 Post
  AND  浏览量 >= 1,000
  AND  内容包含视频
  AND  内容不包含图片
  SORT 浏览量降序
```

**重要语义**：扫描范围中的 N 是**筛选前**考察的 Post 数；媒体组视为一个 Post。Post 浏览/转发指标取组内最大有效值；评论和 Reaction 以选定的主消息为准。缺失指标不是零，条件会按三值逻辑处理。

更多字段、`Verified / Assumed / Truncated / Unknown` 完整性和扫描资源限制的解释见[完整使用指南](docs/USER_GUIDE.md)。

## 从源码构建（Windows x64）

需要 **Node.js 24、pnpm 10、Rust stable、Visual Studio 2022 C++ Build Tools、Windows SDK** 和固定版本的 TDLib 原生依赖。

```powershell
git clone https://github.com/Dufunare/TMsgPicker.git
cd TMsgPicker
pnpm.cmd install --frozen-lockfile

# 首次构建原生依赖（可能需要较长时间）
.\scripts\build-tdlib.ps1 -Jobs 2

$env:TMSGPICKER_TDLIB_DIR = (Resolve-Path .\native\tdlib).Path
pnpm.cmd tauri dev
```

重新生成同款应用图标：

```powershell
pnpm.cmd tauri icon docs/assets/tmsgpicker-icon.svg --output src-tauri/icons
```

图标的单一设计源是 [`docs/assets/tmsgpicker-icon.svg`](docs/assets/tmsgpicker-icon.svg)，使用 Tauri CLI 生成 Windows ICO、macOS ICNS 和各尺寸 PNG。发版时会自动生成、检验并同步图标资源。

详细的原生依赖来源、安装器打包和常见构建错误见 [Windows 构建文档](docs/BUILD_WINDOWS.md)。

## 技术架构

```text
                  Vue 3 / TypeScript
           主窗口 · 登录 · 设置 · Query Builder
                         │ Tauri IPC
                 Rust 应用层 / 领域层
          AccountRegistry · Post Scan · Cache
                         │ TDLib JSON C API
         本地 TDLib Client / 加密 Session / Telegram
```

- **TDLib**：`1.8.68`，固定至 2026-10-12 的 [`f381f4b`](https://github.com/tdlib/td/commit/f381f4b35ebfd52a086ec3af5352ed77bbdbde01)。
- **UI**：Tauri 2、Vue 3、TypeScript、Vite。
- **Rust**：edition 2024；Post 聚合、查询状态、扫描预算和账号隔离。
- **固定依赖与构建**：见 [`scripts/native-versions.json`](scripts/native-versions.json) 和 [发布说明](docs/RELEASE.md)。

## 安全与边界

- 没有默认 Telegram API Hash；不要在 Issue、截图或日志中泄露 API Hash、登录二维码、验证码、2FA 密码或 Session 文件。
- 数据保存在用户本机；**断开连接**保留 Session，**注销账号**才调用 Telegram `logOut`。卸载/升级不应擅自删除用户数据。
- 便携版/安装器不得携带开发者的真实账号数据。当前版本没有自动更新或代码签名。
- 本项目独立开发，**不隶属于 Telegram，也不受 Telegram 官方认可或背书**。

## 文档

[使用指南](docs/USER_GUIDE.md) · [架构与设计](docs/ARCHITECTURE.md) · [功能与模块](docs/projects.md) · [Windows 源码构建](docs/BUILD_WINDOWS.md) · [Release 与验收](docs/RELEASE.md) · [更新日志](CHANGELOG.md) · [开发记录](docs/Devlog.md)

---

<div align="center">

**TMsgPicker** · Made for thoughtful Telegram exploration

[MIT License](LICENSE) © 2026 Dufunare · TMsgPicker 图标与应用代码遵循仓库许可，第三方依赖遵循其原有许可证。

</div>
