# 设备日历宿主 API

[English](README.md) | 简体中文

`device_calendar` 声明用于说明用途，不是授权。应用准入、`host-api-v1`、账户范围、用户同意、系统权限和原生审核仍是独立要求。

`device_calendar.*` 为安装的 App Hub 应用提供有范围和数量限制的系统日历访问。macOS 使用 EventKit，Android Home 使用 CalendarProvider。它读取操作系统已配置的日历，不负责登录 Google 或创建日历。内置 `calendar.*` 与基于 OAuth 的 `gcalendar.*` 保持独立。

这里描述的是源代码实现，不代表现有发布版已经包含它。App Hub 合约必须支持 `device_calendar`，宿主也必须注册该服务。先通过 `runtime.list`、`runtime.describe` 和 `device_calendar.permission.status` 检查实际能力。Windows、Linux、iOS 和不含 Home 适配器的 Android 打包明确报告不支持。只有在专用测试日历上完成验证后，才能确认真实 OS 权限和日历操作；目前不能将编译通过称为设备验收通过。

## 声明与迁移

应用声明 `device_calendar` capability、`requires: ["host-api-v1"]`，并将每个使用的方法按版本 `1` 写入 `host_api.required`。以下仅为 **manifest 片段**：

```json
{
  "capabilities": ["device_calendar"],
  "requires": ["host-api-v1"],
  "host_api": {
    "required": {
      "device_calendar.permission.status": 1,
      "device_calendar.permission.request": 1,
      "device_calendar.calendars.list": 1,
      "device_calendar.calendars.select": 1,
      "device_calendar.events.list": 1,
      "device_calendar.events.create": 1
    }
  }
}
```

Muse 自定义的 `calendar.*` 不是标准系统日历接口。应迁移其适配层，不要改名占用内置日历服务，也不要假设用户安装了参赛者定制的 Rust 宿主。若应用需要登录的 Google 账户，而非设备系统中的日历，仍应使用 `gcalendar`。

## 用户流程与方法

1. 在前台请求应用授权。宿主自己的审核界面解释访问范围；用户实际点击批准后，才可能出现系统权限框。
2. 列出日历，以原生日历的 `calendar_id` 调用 `calendars.select`。宿主显示日历及账户，批准后返回不透明的 `handle`。
3. 将 handle 保存到当前应用账户的数据中，用它读取事件。账户范围由宿主决定，不接受调用 JSON 指定账户。
4. 通过 `events.create/update` 提交完整草稿。宿主显示不可变的内容、日历和账户；更新同时显示原事件。只有用户在原生审核界面实际批准，才执行系统写入。
5. 完成后刷新。后续编辑或删除需要使用新返回的 `revision`。

下表名称均以 `device_calendar.` 开头。参数是 JSON 对象；未知字段被拒绝。

| 方法 | 参数 | 结果 / 权限 |
| --- | --- | --- |
| `permission.status` | `{}` | `supported`、`app_consent`、`os_permission`；不弹权限框 |
| `permission.request` | `{}` | 原生应用授权审核，再请求 OS 权限；仅前台 |
| `permission.revoke` | `{}` | 持久化完成后撤销当前应用账户授权及 handle，不撤销整个宿主的系统权限；仅前台 |
| `calendars.list` | `{}` | 最多 64 个日历及来源/账户标签；需要应用及系统授权 |
| `calendars.select` | `calendar_id` | 原生日历/账户审核；返回 `handle` 和元数据 |
| `events.list` | `handle`、`start_ms`、`end_ms`、`limit` | `events` 和 `truncated`；93 天内最多 200 条出现记录 |
| `events.get` | `handle`、`event_id` | 单个事件及 `revision` |
| `events.create` | `handle`、`event` | 审核后返回 `{saved, event}` |
| `events.update` | `handle`、`event_id`、`revision`、`event` | 审核完整替换内容；旧版本冲突会拒绝 |
| `events.delete` | `handle`、`event_id`、`revision` | 审核删除后返回 `{deleted, event_id}` |

草稿必须有 `title`、`start_ms`、`end_ms`、有效 IANA `timezone`；可选字段为 `all_day`（默认 false）、`location` 和 `notes`（默认空）。时间使用 Unix 毫秒，范围 1970 至 2100 年，结束边界不包含在事件中，最长 93 天。标题/位置/备注的 UTF-8 字节上限分别为 512/2048/8192。全天草稿使用 `timezone: "UTC"` 和 UTC 零点日期边界。审核显示对应时区的本地时间、缩写及 UTC 偏移，正确处理夏令时；全天明确显示结束日期不包含在范围内。

**尚未执行的合成数据交互验收流程：** 选择专用测试日历；创建 `America/Los_Angeles` 09:00 的 “Synthetic visit”；手动批准；列出当天事件；使用返回的 revision 修改标题；批准并刷新；最后使用新的 revision 审核删除。自动验证不要使用个人日历。

## Agent 与安全边界

只读 status/list/get 可在正常应用授权后作为 agent 工具提供。授权、选择、撤销和事件修改仅支持前台；本版没有无人审核的后台日历写入工具。应用 agent 可以在应用或卡片里生成草稿，引导用户打开审核，不能通过工具、复制的 Splash 界面、合成点击、sheet 方法或猜测 ticket 伪造批准。

宿主重新验证安装包和 `host-api-v1`，并通过回调取得当前已认证的应用账户；无账户应用使用明确的宿主范围，需要账户却未登录的应用被拒绝。授权、选择 handle、版本和待审请求都绑定该应用及账户的宿主数据目录。OS 日历的来源账户也单独核对。账户切换或移除应用会使请求失效；选择或撤销会递增授权版本。撤销以回调确认持久化完成为准；已经进入系统提交的操作无法回滚。

第 1 版将重复事件和包含参加者的邀请设为只读。不提供邀请发送、重复规则编辑、提醒、创建日历、忙闲共享或后台调度器。Android 的原子 provider batch 同时核对日历来源和已审核的旧事件。EventKit 在提交前再次检查内容，但其 API 无法与其他日历编辑器的同时修改执行原子比较。

## 执行与限制

原生操作和授权持久化在 UI 线程之外运行：宿主使用任务池，Android 使用有界 provider 执行器。结果经有界、非阻塞通道返回。最多 16 个待处理任务/审核、两个宿主 worker；任务 45 秒、审核 300 秒超时；结果最多 1 MiB（Android 扩展桥为 256 KiB），超限需减小查询数量。过期排队任务不执行，无任务/审核时停止定时器。关闭、账户切换和超时使回复失效；Android 接收取消 ID。系统内部已经开始的操作可能完成，即使回复被丢弃；写入超时后先刷新再重试，以免重复创建。

宿主专用授权文件原子替换，Unix 权限 0600。正常读取使用非阻塞缓存查询，不等待 worker 的锁。先调用 `permission.status`：它在任务池读取最多 1 MiB 的授权数据，再查询 OS 状态。初始化前的数据或弹窗调用返回明确错误，缓存忙时也明确提示重试。每进程最多缓存 64 个应用目录。持久化失败返回错误，不虚报授权或撤销成功。

macOS 打包包含 Calendar 使用说明及 entitlement。缺少使用说明的裸二进制会拒绝权限请求，避免触发 Apple 的隐私终止。Android Home 声明 READ_CALENDAR/WRITE_CALENDAR，经 OS 权限 Fragment 请求，并向 Rust 通报适配器存在。这些系统声明不会自动给 App Hub 应用授权。

## 代码与验证

- `mod.rs`：包准入、账户、API 发现、有界调度、单次审核状态。
- `model.rs`：参数限制、时区校验、不可变事件版本。
- `store.rs`：应用/账户授权、handle、持久化版本。
- `prompt.rs`：原生审核、本地时间、可信输入和失焦复位。
- `macos.rs`：EventKit；`phone/resources/android/java/dev/makepad/octosense/DeviceCalendarClient.java`：CalendarProvider。

合成 Rust 测试覆盖范围、时区/夏令时、授权隔离和撤销、旧版本冲突、单次审核，以及拒绝合成批准。编译不代表真实 OS 权限及 provider 行为已验收。测试不读取个人日历、不授予 OS 权限。
