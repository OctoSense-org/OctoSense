# 照片卡片与代理工具

[English](README.md) | 简体中文

照片应用、代理工具与 Glance 卡片使用同一个内置**示例图库**，不读取 Android
手机相册。应用需要 Shell 的照片宿主服务；独立 `card-host` 未注册该服务时会显示
不可用状态，而不会另建一份图库。`catalog.json` 保存 75 条示例记录，缩略图随应用发布。现有相册与收藏
仍保存在 `os.photos/accounts/device/library.json`；AI 回忆的快照也保存在同一
账户文件夹。读取元数据不等于看到图像，代理不得将示例人物姓名当作用户的家人。

应用中的 **Show in Glance** 将当前集合的前 12 张照片展示为卡片；查看器中的
**Glance** 只展示当前照片。卡片显示示例元数据与缩略图，卡片内的 **Open Photos**
可在原有查看器中打开同一组已保存的照片。共享 Glance 工作区提供 Card/Chat，
聊天绑定照片应用的账户代理及当前选集。默认不发送通知，同一有序选集会替换原卡片。
未过期的选集可静默恢复，关闭卡片的状态会保存。上述界面与设备行为仍需在当前构建
验收；本文不宣称已通过手机测试。

| 工具 | 行为 |
| --- | --- |
| `photos.list` | 有界元数据搜索，返回已保存的收藏标记。 |
| `photos.read` | 按 ID 读取一张示例照片。 |
| `photos.collections` | 读取当前保存的相册和收藏。 |
| `photos.publish_card` | 展示 1–12 个已存在的照片 ID，可明确请求通知。 |
| `photos.notify` | 保留原有通用通知。 |

前四个工具可经代理中继共享，但调用方仍需明确授权和宿主准入声明；标记为可共享
不会自动向其他应用授予权限。宿主服务拒绝其他应用直接调用。`photos.view` 仅用于
界面消费导航请求，不是代理工具。照片代理具有明确的新闻工具授权，可在用户要求时研究指定的公开
话题；不得自动将照片元数据发给新闻代理或外部搜索服务。服务在通用通知服务之前注册，因此无需先打开
照片应用，代理也能搜索和发布卡片。

`crates/shell/src/photos.rs` 负责 API、选集持久化和可信本地资源服务器。Glance
仅向照片卡片允许该服务器的精确回环地址。模型只提供图库 ID，不能提供任意 URL
或文件路径。相册和收藏仍由原应用写入；工具读取同一文件，损坏时报告错误而不
覆盖数据。代理说明位于 `bundle/AGENT.md`。

新增回归测试涵盖共享保存状态、拒绝损坏数据、选集标识、L0 合法性、有界照片 ID
和编辑期间保留导航请求。具体结果记录在实现的测试报告中；原生画面、真实模型
推理和手机生命周期需单独验收。

在已配置好的 OctoSense Home 中复现卡片流程：

1. 打开 Photos → Library，搜索 `Evening on the shore`，再打开照片。
2. 点击 **Glance**，返回 Home，右滑进入 **At a glance**。
3. 打开照片摘要，检查缩略图、示例图库标识和 **Open Photos** 按钮。
4. 切换 **Card → Chat → Card**，未发送的聊天草稿应保留。
   聊天需要照片代理授权及已配置的模型服务；仅打开标签页不能证明模型实际运行。
5. 在照片应用中打开另一张照片，再返回原 Glance 卡片，点击 **Open Photos**。
   查看器应切回卡片对应的原选集。

从仓库根目录运行服务与生产脚本回归测试：

```sh
cargo test --locked -p octosense-shell --features mobile-apps photos
cargo test --locked -p octosense-llm-service --test photos_memories
```

原生验收应使用独立的 `OCTOSENSE_HOME`、`OCTOSENSE_APP_DATA` 和 `RINX_DATA_DIR`。
截图、模型会话与账户文件不得提交到仓库。共享工具验收必须经过真实代理中继和
调用方授权；直接调用 Rust 函数只能验证服务逻辑。
