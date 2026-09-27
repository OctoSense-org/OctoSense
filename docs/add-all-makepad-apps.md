# 把 Makepad 全家桶应用到 OctoSense

本文说明如何把 Makepad 仓库里的全部应用(浏览器、文件、终端、天气、金融、邮件、笔记、日历、提醒、计算器、Fabric、Score、视频、导航、VJ、Fab、Director、Image、PDF、AI 等)一次性加进 OctoSense 桌面,让它们全部出现在 **⌘Space** 应用菜单里并可随时打开。

---

## 一、前提条件

1. **Makepad 同级框架仓库就位**

   ```sh
   cd /path/to/OctoSense
   python3 tools/setup-native.py --update
   ```

   把 `../makepad`、`../octoscript`、`../octoscript-makepad` 检出到锁定版本。
   目录里 `"source": "makepad"` 的行要靠 `cargo metadata --offline` 解析出
   这个 checkout 才能生效。

2. **Cargo 缓存完整**(离线 metadata 能成功)

   第一次构建宿主后,跑一次联网 `cargo metadata` 把缺失依赖补进缓存:

   ```sh
   cargo metadata --format-version 1 --manifest-path Cargo.toml > /dev/null
   ```

   之后 `cargo metadata --offline` 正常,目录解析出 Makepad 根。
   (缓存不全时所有 `source: makepad` 行会被静默跳过,日志只显示
   `no app 'X' in the registry`。)

3. **编译宿主**

   ```sh
   cargo build --release
   ```

---

## 二、两个应用目录

| 文件 | 内容 |
|---|---|
| `config/apps.json`(默认) | 只有 `reference`(本项目自带的示例) |
| `config/apps.makepad.json` | Reference + Makepad 全家桶(20+ 个应用) |

`config/apps.makepad.json` 已经包含所有 Makepad 应用,你**不需要手写**。
启动时指定它:

```sh
cargo run --release -- --apps config/apps.makepad.json
```

如果想把它变成默认,改 `config/apps.json` 的内容(或建
`~/.octosense/apps.json` 个人目录覆盖),重启即可。

---

## 三、目录里 Makepad 全家桶的完整条目

`config/apps.makepad.json` 的 `source: makepad` 行(每行 3 要素:
`package` = Makepad 的 Cargo 包名,`bin` = 可执行文件名,`policy` = 多实例策略):

| id | label | package | bin | policy |
|---|---|---|---|---|
| `browser` | Browser | makepad-browser | browser | focus |
| `files` | Files | makepad-files | files | focus (args: `--demo`) |
| `terminal` | Terminal | makepad-terminal | terminal | new |
| `mixer` | Mixer | makepad-mixer | makepad-mixer | focus |
| `task` | Task Manager | makepad-task | task | focus |
| `sheets` | Sheets | makepad-sheets | sheets | focus |
| `photos` | Photos | makepad-photos | photos | focus |
| `clock` | Clock | makepad-clock | clock | focus |
| **`weather`** | **Weather** | **makepad-weather** | **weather** | **focus** |
| `finance` | Finance | makepad-finance | finance | focus |
| `mail` | Mail | makepad-mail | mail | focus |
| `notes` | Notes | makepad-notes | notes | focus |
| `calendar` | Calendar | makepad-calendar | calendar | focus |
| `reminders` | Reminders | makepad-reminders | reminders | focus |
| `calculator` | Calculator | makepad-calculator | calculator | focus |
| `fabric` | Fabric | makepad-fabric | makepad-fabric | focus |
| `score` | Score | makepad-app-score | makepad-app-score | focus |
| `video` | Video Player | makepad-video | video | new |
| `route` | Route | makepad-app-route | route | focus |
| `vj` | VJ | makepad-vj | makepad-vj | focus |
| `fab` | Fab | makepad-fab | makepad-fab | focus |
| `studio` | Director | makepad-director | director | focus |
| `image` | Image Viewer | makepad-image | image | new |
| `pdf` | PDF Viewer | makepad-pdf | pdf | new |
| `aichat` | AI | makepad-aichat | aichat | new |

`policy: focus` = 已有实例就聚焦,`policy: new` = 每次开新实例。

---

## 四、启动并打开全家桶

```sh
# 1. 启动桌面,加载 Makepad 全家桶目录
cargo run --release -- --apps config/apps.makepad.json
```

启动后用任一方式打开应用:

**脚本化**(每个应用独立一行,适合自动化):

```sh
# 在启动参数里同时开多个(可叠加)
cargo run --release -- \
  --apps config/apps.makepad.json \
  --test-action launch-weather \
  --test-action launch-browser

# 或者在桌面已运行时,另开一个 shell 发 test-action
# (test-action 只在启动时执行一次,所以每次要新起进程)
```

**手动**(在桌面里操作,最常用):

- **⌘Space** → 应用菜单,搜索并点击任意应用名
- 或 **⌘1…0** 切工作区,**⌘W** 关 tile,**⌘F** 当前 tile 全屏

每个应用首次打开会现场 `cargo run` 编译(进度显示在 tile 里,
首次冷构建约 90 秒);之后复用 Cargo 缓存,秒开。

---

## 五、验证

启动后在桌面主日志(重定向 stdout 可得)里找:

```
wm: desktop style octosense applied to N clients and 0 modules
# N = 已打开的应用 tile 数

# 对每个打开的应用:
wm: --test-action launch weather
wm: launched weather as client 1
wm: weather client 1 first frame in NNN ms (cold)   ← 出画面

# 打开多个应用:
wm: launched browser as client 2
wm: launched terminal as client 3
...
```

`first frame in NNN ms (cold)` 表示该应用的首帧已渲染,tile 里能看到画面。
每个应用自己的日志在 `~/.octosense/wm/logs/<app>.log`。

---

## 六、常见问题

**打开应用时报 `no app 'X' in the registry`**

Cargo 离线缓存不全。联网跑一次:

```sh
cargo metadata --format-version 1 --manifest-path Cargo.toml > /dev/null
```

再重启 OctoSense。

**应用 tile 里一直显示 "building…" 不出画面**

首次冷构建,进度正常。等 90 秒左右出首帧。之后秒开。

**应用打不开(构建失败)**

看 `~/.octosense/wm/logs/<app>.log` 里的编译错误。
Makepad 全家桶应用都依赖同一个 Makepad checkout,版本不匹配会失败。

**某个应用不想要,隐藏它的启动器图标**

在 `~/.octosense/wm/launcher.hides` 里加一行应用 id,例如:

```json
["image", "pdf", "aichat"]
```

Image/PDF/AI 这三个 helper 应用默认出现在启动器里(除非被隐藏)。

---

## 相关

- 单个应用目录格式:README「Add an app」章节
- 目录解析原理与离线 metadata 踩坑:[open-apps.md](open-apps.md) 第四、五章
- Makepad 全家桶目录是生成物:由上游 `scripts/upstream.py catalog`(读
  Makepad checkout 的依赖图生成)产出,本地在 `config/apps.overlay.json`
  叠加适配。检查漂移或重新生成:
  `python3 scripts/upstream.py catalog`(加 `--apply` 写回)。
