# Daycast 天气应用集成说明

[English](README.md) | 简体中文

已将用户 OctoScript 天气应用接入 OctoSense 桌面系统应用清单。bundle 保留为 `apps/weather-assistant/bundle/`，应用 ID 使用系统应用命名空间 `os.weather-assistant`。

家庭天气提醒在 Daycast 打开期间每 15 分钟检查各家庭城市，首次只建立基线。之后开始降水、较上次升降温 ≥5°C、进入 ≥35°C 高温或 ≤0°C 冰点、风速上升至 40 km/h 时生成提醒；重复或旧观测不重复触发。最近 20 条提醒与天气基线保存在应用私有存储，关闭应用后停止检查。

在「我的」读取已打开并登录的 Rinx 聊天室，选择家人聊天室，再开启 Rinx 提醒（默认关闭）。UI 调用 `storage.daycast.rinx.rooms`/`send`，由 DaycastService 转交宿主 `host_tools/daycast_rinx.rs`。桥接只接受 Daycast 系统/商店应用 ID，以及固定的聊天室/正文参数；AI bus 调用 Rinx 原生的 `list_rooms`/`send_message`。Rinx 展示目标聊天室和完整正文，用户确认后使用 Matrix SDK 会话发送；Daycast 只在原生返回成功后标记已发送。凭据不进入应用。

失败或取消可手动重试；重启时正在发送的提醒显示结果未知，请先在 Rinx 检查再决定重试。历史草稿不会在重启或开启开关时自动发送。注册使用 https://auth.matrix.rinx.chat，聊天服务器使用 https://matrix.rinx.chat；在 Rinx 用完整 Matrix 账号邀请家人加入聊天室。参见 [Rinx 说明](https://github.com/gosimfoundation/hackathon-agenticapp26/blob/main/docs/rinx-guide.md)。真实账号收件和移动设备表现尚未验证，需要用户用自己的家庭聊天室验收。

问答页使用宿主的单次 `model.complete` 服务，把当前天气、衣橱、偏好、家庭城市和本地日程作为上下文。输入框上方的模式按钮可以让 AI 帮忙添加衣物、创建天气偏好、添加家庭城市或创建日程；AI 先展示待确认卡片，用户确认后才保存。家庭城市会先通过地理编码核对地点，再供用户确认。“创建日程”会将下一条消息标记为明确的日程创建请求，因此“提醒我吃药”会按 Daycast 日程处理，而不是被当作闹铃问题。AI 也可以根据空闲时段和天气建议活动时间；只有用户点击“确认并添加”后才会写入日程，保存前会规范化常见时间格式并再次检查冲突。

你可以直接在 Daycast 助手聊天里让它添加日程。助手会先补齐缺少的信息、检查时间冲突，然后由 OctoSense 显示包含事项和准确时间的授权卡片；只有你批准后才会写入 Daycast 私有日程。写入完成前不会声称已经添加。系统助手也能从系统聊天添加 Daycast 日程，同样需要这次明确授权。

“今天”页使用宿主的单次 `model.complete` 调用，结合当前城市天气、天气偏好、衣橱和日程生成出门建议。输入包含当前温度、体感温度、当日高低温、降水及当日降水概率和紫外线预报。刷新天气或修改个人数据后会重新生成；AI 不可用时会明确标记并显示本地参考建议。上述上下文会发送给用户配置的模型。

衣橱条目可以选填家中存放位置，也可以用应用内相机拍摄衣物照片。照片会缩小为缩略图并保存在应用私有目录，不会发送给天气或 AI 服务。当前宿主版本暂不支持从本地相册或文件选择已有照片。

# Weather Assistant integration

The user's OctoScript weather bundle is included in OctoSense's desktop system-app list at `apps/weather-assistant/bundle/`, using the reserved system id `os.weather-assistant`.

The host opens the app's `octos.*` context on the person's lane. The system agent runs on the peer's parallel lane; both lanes share history. The bundle declares `octos.session.history` so its Q&A page can display messages from both lanes. The system agent is instructed to answer only from recent weather context in that shared history; no weather query tool is currently available to it.

The Today card uses the host's one-shot `model.complete` service to write an outing suggestion from the current city's weather and the preferences selected in Mine. It regenerates after a weather refresh or preference change and labels its local reference suggestion when AI is unavailable. The city's weather and selected preferences are sent to the person's configured model.
