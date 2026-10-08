# ADR 0013：由操作系统认证的写入审批

[English](0013-os-authenticated-write-approval.md) | 简体中文

状态：实现审阅中，OS 认证用户验收待完成。Linux/Windows 的物理指针来源验证仍不受支持。浏览器和凭据库测试不能证明写入审批可用。

## 问题与决定

`crates/shell/src/connected_review.rs` 的宿主原生审阅页显示完整 Gmail 回复、GitHub 修改、Calendar 日程或已声明的后端变更。其原生 `oauth-service` 能力要求可信的按下与抬起事件。Android/macOS 适配器支持这种验证；Linux/Windows 当前也会拒绝真实用户。不能通过把合成输入标记为可信来修复按钮。

保留现有物理审批。Linux/Windows 增加 **OS 认证审批**：用户审阅内容后，点击原生页的 **认证并发送/保存**，再完成操作系统自己的认证。普通、远程或合成点击本身都不能提交写入。

该证据不同于 `trusted_user_input()`，不得设置该标志、伪造 `down/up=true`，也不能进入脚本/工具参数。两种证据使用同一个内部的一次性执行声明。提供商登录、令牌存储和普通浏览器使用仍是独立操作。

## 绑定与生命周期

原生审阅所有者创建挑战，包含随机 nonce、较短的单调时钟期限、已接纳应用/包身份、连接与活动账户代次、审阅 ID、完整显示操作的规范化 SHA-256 摘要，以及原生窗口代次与宿主页/isolate 所有者。摘要覆盖目的地、内容和资源版本。不信任应用传入的摘要、命令、回调或认证结果。

只有可见、前台的原生审阅页可以发起认证。关闭/替换、账户/草稿变更、超时、断开连接、撤回应用以及窗口销毁/复用都取消挑战。完成时再次检查全部绑定与当前接纳的操作，再原子消费 nonce。迟到或重复结果、修改后的草稿必须拒绝。提交时仍执行现有账户、权限和资源冲突检查。

状态机为 `Reviewing → Authenticating → Claimed → Finished`；取消/超时是终态。失败后只有生成新挑战才能返回 Reviewing。认证异步执行，取消永远不能授权写入。OS 弹窗仅使用宿主生成的简短说明，完整邮件正文和凭据不进入 polkit 参数或日志。

## Linux

直接调用系统 polkit authority 的专用 `org.octosense.approve-business-action` action。受支持安装包携带 root 所有的策略：`allow_any=no`、`allow_inactive=no`、`allow_active=auth_self`，不保留授权。不执行特权命令/helper；这里批准的是同一用户的业务操作，不是特权系统操作。

非交互预检必须报告需要新的认证挑战，已经授权/缓存的结果不能通过。交互 `CheckAuthorization` 绑定当前进程 PID、启动时间、UID 与唯一取消 ID。保留/临时授权、取消、缺少 authority/agent/policy 或异常结果都应拒绝。所有者消失时取消检查。参见 [polkit 策略语义](https://polkit.pages.freedesktop.org/polkit/polkit.8.html)和 [authority 接口](https://polkit.pages.freedesktop.org/polkit/eggdbus-interface-org.freedesktop.PolicyKit1.Authority.html)。

系统管理员仍在信任边界内。AppImage/源码启动必须明确报告策略缺失，不能偷偷安装策略或退回合成审批。本任务没有授权在共享测试主机安装系统策略。

## Windows

使用 `UserConsentVerifier.CheckAvailabilityAsync` 和桌面 `IUserConsentVerifierInterop::RequestVerificationForWindowAsync`，绑定准确的 HWND。只有 `Verified` 可以授权。取消、设备忙碌/未配置/禁用、API 不可用、窗口销毁与 HRESULT 错误均拒绝。OctoSense 接收验证结果，不接收 PIN 或生物识别数据。Microsoft 的[桌面用法](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.ui.userconsentverifier?view=winrt-26100)指定此接口；[接口要求](https://learn.microsoft.com/en-us/windows/win32/api/userconsentverifierinterop/nf-userconsentverifierinterop-iuserconsentverifierinterop-requestverificationforwindowasync)列出的最低版本为 Windows build 22000。旧平台应显示不可用，除非另行实现并审阅适配器。

## 实施与验收

1. 在 `oauth-service` 增加仅供原生宿主使用的审批模块：不可变绑定、挑战/取消状态、没有公开构造器或序列化的不透明证据。Gmail、连接器和后端变更消费类型化证据，保持物理输入语义。
2. 平台适配器放在该边界之后，只为活动且绑定的挑战生成证据，不直接执行写入。
3. 扩展现有原生审阅控件的认证状态与可用性提示，复用完整内容审阅和提交线程，不增加应用可调用的审批 API。
4. 在受支持安装包中包含 Linux action，检查策略与所有权；注册缺失时仍不可用。
5. 测试账户/摘要/窗口变更、重放、迟到结果、取消、超时。真实原生负向测试须拒绝缺少 policy/Hello 的环境。随后用独立测试账户验证 OS 认证后的写入，并证明只有合成输入时不能提交。

登录、原生凭据库、普通嵌入浏览器、OS 认证审批和最终远程写入分别记录。编译、单元、Xvfb 和凭据存储通过不能证明用户审批。物理输入正向通过需要实际用户/设备证据。原生交互完成前，审批与业务写入验收仍未验证。

已实现边界、安装包前提和未验证的原生正向验收，见[桌面写入审批](../os-authenticated-approval.zh-CN.md)。
