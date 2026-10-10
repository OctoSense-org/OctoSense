# Mail 宿主服务

[English](README.md) | 简体中文

已准入的 App Hub 应用可以通过宿主登录面板连接自己的邮箱，读取邮件、撰写草稿
并请求原生发送审阅。`mail` 声明只说明用途；仍需实际应用/账户访问权和原生审阅。密码和 SMTP 连接始终由本服务持有；
模型或应用不能批准发送。

## 公共撰写接口

| 方法 | 输入 | 结果 |
| --- | --- | --- |
| `mail.compose` | `account`、`to`、`subject`、`body`；可选 `compose_id`、`expected_revision`、`folder`、`message` | 保存草稿，返回 `compose_id`、版本和状态。不会弹窗或发送。 |
| `mail.compose_status` | `account`、`compose_id` | 当前应用/账户的草稿及最后一次提交记录。 |
| `mail.review_send` | 与 `mail.compose` 相同 | 前台原生审阅；取消或提交后回调。 |
| `mail.send` | 与 `mail.compose` 相同 | 第三方应用的兼容入口，打开同一审阅面板；不会直接调用 SMTP。 |

宿主为新草稿生成 `compose_id`，应用应保存它和版本号。修改已有草稿必须提供
`expected_revision`；旧编辑器收到 `revision_conflict`。请求审阅时传入已保存的
完整字段和版本。超时或发送报错后，应先查询 `mail.compose_status`，不能自动
新建撰写会话再发一次。

草稿处理、审阅准备和提交共用最多 16 个工作线程。原生面板立即打开，准备期间
禁用批准控件；准备工作在 UI 线程之外运行。有界通道传递精确快照和结果，
UI 只进行非阻塞轮询，不等待草稿磁盘操作。

面板展示应用身份、账户、发件人、收件人、主题和完整正文。只有在原生
**Approve & Send** 控件上的真实按下与释放才会批准该不可变版本。测试工具、
合成点击、脚本复制的审阅控件以及 Agent 工具都不能批准。
目前 macOS、Android 支持发送审阅；Linux/Windows 可以保存和查询草稿，
但没有经过来源验证的真实输入发送适配器。

只有 SMTP 传输层确认受理，回调才返回 `accepted: true`、`status: "accepted"`
和 `id`（发出邮件的 Message-ID）；这不代表对方已经收到。取消、失败或结果不确定时返回错误，持久状态
仍可查询。不确定的提交不会自动重试。本版公共接口未提供失败后重试方法。
对同一次提交重复审阅或批准不会再次调用 SMTP。

每个应用/账户最多保存 128 条撰写记录、64 MiB，每条最多 512 KiB（含最多
32 次提交记录）。容量已满时，新建返回 `resource_limit`；仍可修改已有草稿，
并为其最终提交结果预留有界空间。已受理或结果不确定的记录不会自动清理，避免
形成自动重发路径。移除当前应用的邮箱授权会删除其草稿，不删除其他应用记录。
暂未提供单条丢弃接口。

限制：一个纯文本收件地址，主题最多 512 个 UTF-8 字节，正文最多 8192 个
UTF-8 字节。拒绝抄送/密送、附件、自定义邮件头和发件人覆盖。回复时可提供
缓存原邮件的 `folder`/`message`，由宿主生成回复邮件头，编辑不能更换原邮件。

## 迁移示例

下例仅说明 Splash 接口，尚未作为参赛应用包运行验证：

```splash
let compose_id = ""
let revision = 0
fn save_reply(account, to, subject, body){
    let args = {account: account, to: to, subject: subject, body: body}
    if compose_id != "" {
        args.compose_id = compose_id
        args.expected_revision = revision
    }
    host.request("mail.compose", args, fn(r){
        if r.is_ok { compose_id = r.data.compose_id; revision = r.data.revision }
        // 展示错误，保留用户尚未保存的文字。
    })
}
fn review_reply(account, to, subject, body){
    host.request("mail.review_send", {
        account: account, to: to, subject: subject, body: body,
        compose_id: compose_id, expected_revision: revision
    }, fn(r){
        // 仅当 r.is_ok 且 r.data.accepted == true 时显示已受理。
        // 报错后先查询 mail.compose_status，再提供下一步操作。
    })
}
```

声明 `mail` 说明用途，省略声明不会拒绝调用。新应用可声明精确主版本要求：
`host_api.required: {"mail.compose": 1, "mail.compose_status": 1,
"mail.review_send": 1}`。消费端契约支持时，Agent 工具可以映射到
`mail.compose` 和 `mail.compose_status`。需要账户的 Agent 声明
`storage.accounts: true`；Shell 将工具别名绑定到所属应用的当前账户，拒绝旧
会话和跨账户参数。发送/审阅接口仅限前台。Agent 结果会明确标记正文截断，
原生审阅始终展示完整正文。

## 所有权与验证

公共草稿 ID 的宿主派生包含应用和账户，记录位于应用沙箱之外。多个应用共享
邮箱访问授权也不共享草稿。历史 `mail.propose_reply`、`mail.draft`、
`mail.suggest_reply`、`mail.propose_send` 及 Mail 原有卡片/编辑器仍专属于
`os.mail`。发送认领时，Shell 重新检查应用准入、账户选择和暂停
状态；服务重新检查账户权限、版本、完整内容、发件人及一次性提交状态。
原始应用隔离实例关闭后，未提交的审阅失效。审阅凭据十分钟后过期，不会序列化
或暴露给脚本。

合成服务测试使用假传输层和隔离邮箱数据，不会发送真实邮件。覆盖应用/账户
隔离、修改后的旧审阅、过期或撤销、结果不确定、不可信手势及单次提交。
本次改动的原生硬件界面和真实 SMTP 验收仍未验证。
