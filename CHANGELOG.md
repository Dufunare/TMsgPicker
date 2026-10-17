# Changelog

此日志记录影响用户与发布行为的主要变化。完整提交记录请查看 GitHub。

## [0.2.0] — 2026-10-17

### Added
- TDLib 原生 **QR 扫码登录**，支持新建 Session、从手机号验证切换及扫码后的 2FA 授权。
- 新的 TMsgPicker 应用图标和简洁的项目首页；完整版操作文档移至 `docs/USER_GUIDE.md`。
- Windows Release Assets 新增完整构建产物合集 ZIP 和独立的原生依赖来源清单。

### Changed
- 统一登录流程的状态提示，隐藏 QR 初始化时多余的手机号步骤和 QR 2FA 中无关的验证码输入框。
- 移除临时的 Telegram 验证码投递诊断面板及后端投影。
- 优化登录提交按钮布局，在手机号验证码界面提示可改用 QR 登录。
- Windows Release 工作流可在明确的发布意图提交后自动进行检查、同步图标、创建新 tag 与 Draft Release；保留人工发布验收。

### Notes
- Windows x64 预览版本；没有代码签名或自动更新。
- 原生 TDLib/vcpkg 版本锁定保持不变；具体依赖提交见 `scripts/native-versions.json`。
- Telegram API ID/Hash 仍需用户自行提供。

## [0.1.0] — 2026-10-17

- 第一轮 Windows x64 预览构建：Tauri + Vue + Rust + TDLib、Post 查询、可视化过滤器、多账号 Session、SOCKS5、NSIS 安装包与 Portable ZIP。
- Release 默认以 Draft 形式创建，须人工验收后发布。
