# 项目规范入口

本仓库的完整 v0.1 产品与架构设计基线保存于 [TMsgPicker_PROJECT_SPEC.md](TMsgPicker_PROJECT_SPEC.md)。正式开发任务中的 `docs/PROJECT_SPEC.md` 引用统一从本页进入，原始规范保留不变。

实现与验证记录见 [MVP_ACCEPTANCE.md](MVP_ACCEPTANCE.md)，命令与 DTO 见 [IPC.md](IPC.md)。

2026-10-14 起，应用配置、账号注册表、账号凭据和 Runtime 的所有权与生命周期以 [常驻多账号 Runtime 架构](RUNTIME_ARCHITECTURE.md) 为准：启动全部注册账号，独立 Client／Session／数据库口令，共享应用 API／代理和一个接收器；切换只改变前端视图。原 v0.1 文档继续作为查询、Album、筛选和动作业务语义基线。
