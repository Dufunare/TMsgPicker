# Tauri IPC v0.1

生产接口仅接受业务 DTO，不允许前端提交任意 TDLib JSON。类型见 `src/types.ts`，Rust 定义见 `core.rs` / `services.rs` / `app.rs`。所有 ID 为十进制字符串，所有日期为 UTC Unix 秒；指标缺失序列化为 null。

## 命令

Tauri 顶层参数用 camelCase，DTO 内部字段用 snake_case。命令错误返回短错误字符串；TDLib 错误仅显示错误码，避免把请求内容或账号数据放进日志/UI 错误文本。

| 命令 | 参数 | 返回 |
|---|---|---|
| `connection_status` | 无 | `{auth_state,connection_state,detail}` |
| `connect` | `{config:{api_id?,api_hash?,passphrase,proxy?}}` | 按序提交代理请求和 TDLib 参数，然后等待两者响应；不等待代理响应后才提交参数 |
| `auth_submit` | `{value}` | 按当前授权状态提交手机号/代码/密码/邮箱 |
| `disconnect` | 无 | 取消查询，close 并等待 authorizationStateClosed |
| `chats` | `{search}` | `Chat[]`；空字符串加载列表，非空搜索已知聊天 |
| `public_chat` | `{username}` | 公开 Chat，支持输入 @ 前缀 |
| `saved_messages` | 无 | getMe + createPrivateChat 解析出的 Chat |
| `start_query` | `{spec,runId}` | 立即启动后台扫描；结果由事件发送 |
| `cancel_query` | `{runId}` | 仅取消对应活动 run；等待中的请求最多约 50ms 检查取消 |
| `message_link` | `{key:{chat_id,message_id},album}` | TDLib 返回且通过协议检查的 URL |
| `preview_forward` | `{target,postKeys,runId}` | `ForwardReport`，只预检不发送 |
| `forward_selected` | `{target,postKeys,runId,confirmed:true}` | `ForwardReport`；校验选择属于最近查询结果，再重新检查权限 |

`ProxyConfig = {server,port,username,password}`；`port` 为 1–65535。没有代理时显式停用 TDLib 持久化旧代理。密码/API 凭据只在初始化/授权的内存请求中存在，不使用浏览器 storage。

`TelegramGateway.request_batch` 在等待前按序提交所有请求，返回值保持提交顺序；先等待最后的参数请求，以便参数错误立即返回并清理其余 pending。TDLib 将初始化前的 addProxy/disableProxy 排队，因此不可用单条阻塞 request 先等待代理确认。代理请求成功仅表示设置被接受，不表示网络连接成功。

## 查询 DTO

```json
{
  "version": 1,
  "chat_id": "-1001",
  "scope": {"kind":"recent","count":50},
  "filter": {
    "kind":"and",
    "items":[
      {"kind":"compare","field":{"kind":"views"},"op":"ge","value":100},
      {"kind":"is_not_null","field":{"kind":"reply_info"}}
    ]
  },
  "sort":[{"field":{"kind":"views"},"descending":true}],
  "output_limit":20
}
```

日期 Scope：`{kind:"dates",from,to,budget}`；区间 `[from,to)`，budget 包括为定位范围而扫描的更新消息。

字段：views、forwards、comments、reply_info、reaction_total、`{kind:"reaction",emoji:"👍"}`、post_kind、media_album_id、content_kind、date。post_kind/content_kind/media_album_id 是字符串，其余是整数；`reply_info` 表示存在的回复信息对应计数，可用 `IS NULL` 检查存在性。post_kind 值仅 single/album。

比较：eq/ne/gt/ge/lt/le；空值检查：`{kind:"is_null"|"is_not_null",field}`；组合：`{kind:"and"|"or",items:[...]}`、`{kind:"not",item:...}`。服务端限制嵌套深度 16、每组条件 100、排序字段 10，校验字段和值类型。空 AND 为 True，空 OR 为 False。

## 事件

| 事件 | 负载 / 行为 |
|---|---|
| `connection` | 仅授权状态类型、连接状态类型和通用提示；不广播原始手机号/验证码/邮箱/密码提示 |
| `query-progress` | `ScanResult = {run_id,status,scanned,pages,exact,detail,posts}`；status 为 Scanning/Partial/Complete/Cancelled/Error |
| `forward-update` | `{target,old_message_id,state:"Sent"|"Failed"}`；对应发送临时 ID |
| `app-error` | 安全关闭失败时的短错误；保持进程，允许重试 |

前端应先注册监听再启动命令；只应用等于当前 run_id 的扫描事件，后台同样检查活动 run。开始新查询自动取消旧 run，旧请求的迟到响应不会覆盖新结果。

`LogicalPost` 包括 key、kind、members、representative、date、text、metrics、provenance、partial。members 是规范化的 TelegramMessage，包含字符串联合 ID、content_kind、caption/text、media_album_id 和快照 metrics。provenance 为字段到 source_message_id 的映射。

ReactionKey 序列化为 `{kind:"emoji"|"custom_emoji"|"paid"|"unknown",value?}`。所有非 Unicode 反应保留类型和计数。反应对象缺失为 null，已知为空为 `[]`。reaction_total 汇总同一个指标来源内的 reaction 计数，绝不跨 Album 成员累加。

`ForwardReport = {items:[{source:{chat_id,message_id},state,detail,sent_id}]}`。预检为 Allowed/Rejected；执行为 Rejected/Failed/Pending/Sent。Pending 不代表送达，监听 forward-update 并按 target + 临时 sent_id 匹配；进程不持久化动作记录。
