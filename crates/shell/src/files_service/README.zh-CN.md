# 受限应用的文件导入与导出
[English](README.md) | 简体中文

`files` 将前台应用接入系统文档选择器。导入会将一个所选文档复制到应用已有的
Splash 文件系统；导出会保存已有应用文件的快照。应用只收到应用内路径和字节数，
不会得到宿主路径或 Android 内容提供器 URI，也不会创建第二套文件存储。

| 方法 | 参数 | 返回值 |
| --- | --- | --- |
| `files.status` | `{}` | `import_supported`、`export_supported`、`storage_granted`、`max_file_bytes`、`foreground_required` |
| `files.import` | `{"path":"/documents/report.pdf"}` | `{"cancelled":false,"path":"/documents/report.pdf","bytes":123}` |
| `files.export` | `{"path":"/documents/report.pdf","name":"Report.pdf"}` | 相同的成功字段；可选的 `name` 是建议给系统对话框的文件名，不含目录 |

取消选择返回 `{"cancelled":true}`。读取失败、不支持的平台、缺少授权、存储已满和
传输忙碌都会返回错误。导入目标必须是新文件，不能覆盖应用已有文档。应用可用
`fs.read_bytes(path)` 读取导入内容，将字节数组传给图像组件，或通过
`fs.write_bytes(other_path, fs.read_bytes(path))` 复制文件。
`fs.write_bytes` 接受 U8 类型数组，使用与文本写入相同的限制。

传输要求清单同时声明 `files` 和 `storage`；状态查询只要求 `files`。导入与导出
仅限前台，代理工具包装调用也不能绕过此限制。显示对话框前和收到结果后都会检查
已准入清单及请求所属的存活隔离实例。原生代码通过认证后的请求实例键取得已有
存储，并核对宿主设置的应用标识。应用标识、真实存储根、配额、原生目标和 URI
均不来自脚本参数。原生加载在读取前和交付字节前再次检查授权；导出在桌面
重命名提交前及回复前再次检查授权。

导入沿用每文件 1 MiB、已授予的全应用字节配额及 256 个目录项限制，拒绝目录逃逸、
符号链接、盘符、备用数据流和 Windows 设备名。导入提交与脚本执行串行，复用现有
配额写入器。读取所选文档和写出到系统目标由已有的有界任务池执行。同一时刻只
保留一次传输及其快照；导入或导出提供器阻塞时，即使应用已关闭或请求超时，
仍会持续占用这个名额，直到工作线程返回。

请求五分钟后超时。关闭应用会使请求失效，迟到的对话框结果不能导入或开始导出。
已经显示的系统对话框可能仍需手动关闭。原生导出写入一旦开始，关闭应用不能撤销
该写入；Android 提供器没有原子回滚机制。已关闭或超时请求的回复会被丢弃。
桌面导出使用同目录临时文件加重命名；Android 使用 ContentResolver 文档流。
受限应用选择文档时不会保留持久 URI 授权。

macOS、Windows 和 Android 已有字节适配器。Linux 还需 PATH 中存在可执行的 zenity、
qarma、matedialog 或 kdialog。不支持时状态返回 false，发现接口也不列出导入和
导出方法。该服务暂不支持 iOS、OpenHarmony、Web 和直接帧缓冲 Linux。适配器可用
不代表设备已安装文档提供器，也不代表已完成真机交互验证。

集成点：在 `apps.rs` 的平台服务注册旁调用 `files_service::register()`；在
`lib.rs` 的平台事件处理旁调用 `files_service::handle_event(cx, event)`，并保持
在 App Hub 服务互斥锁外执行。App Hub 合约需要认识 `files` 并提供
`Replier::isolate_key`；Makepad 需要包含对应的存储和选择器适配器。
运行时可为原生 `fs.write_bytes` 声明 `storage.binary_write@1` 特性。

验证：五项原生存储单元测试已通过，涵盖归属、配额、拒绝覆盖、路径逃逸、目录项
上限和符号链接。五项原生选择器测试和 Android 目标 Makepad platform Rust 检查也已通过；
编译器仍报告已有的无关警告。
宿主服务测试需在合并集成后执行。系统对话框交互和 Android 提供器真机行为仍为
**未验证**；修复本机 JDK 后，合并后的 Android Java 模板已编译通过，
有 23 条已有的弃用警告。
