# 在 OctoSense 中打开托管应用(操作记录 + 参考)

这份文档记录 2026-09-26 实际跑通的流程,并整理成可复用的参考。

OctoSense 桌面能把 Makepad 应用作为独立子进程托管在自己的窗口(tile)里。
下面先按时间顺序记录当时怎么做的,再给可照抄的命令。

## 一、当时是怎么操作的(按时间)

1. **准备同级框架仓库**:`python3 tools/setup-native.py --update`
   —— 把 `../makepad`、`../octoscript`、`../octoscript-makepad` 检出到锁定版本。
2. **编译宿主**:`cargo build`(debug 或 release),产物在 `CARGO_TARGET_DIR`
   指定的 target 目录下(本机是 `/Volumes/PSSD/dev/rust-target`)。
3. **启动桌面**:跑编译出的 `octosense` 可执行文件。
   第一次用 `--test-action launch-weather` 打 weather,日志报
   `no app 'weather' in the registry` —— 应用目录里查不到 weather。
4. **查原因**:目录的 `"source": "makepad"` 行要靠
   `cargo metadata --offline` 解析出 Makepad checkout;
   而离线 metadata 因为缓存缺依赖(`agent-client-protocol` 1.3.0)而失败,
   导致所有 makepad 应用行被静默跳过。
5. **修复**:联网跑一次 `cargo metadata` 把缺的依赖补进缓存;
   之后离线 metadata 正常,目录解析出 `/Volumes/PSSD/CodeProjects/makepad`。
6. **再开 weather**:桌面把 weather 作为 **client 1** 托管,现场
   `cargo run` 编译;首次冷构建 91 秒出首帧(`first frame in 91473 ms (cold)`),
   天气应用成功渲染进桌面 tile。

## 二、可照抄的命令

### 启动并打开天气

```sh
# 启动桌面(用 Makepad 全家桶目录)
cargo run --release -- --apps config/apps.makepad.json

# 在另一个 shell 里,或直接加在启动参数上:
cargo run --release -- --test-action launch-weather
```

`--test-action launch-<app-id>` 是脚本化打开应用的方式,
app-id 就是目录里的 `id`(weather、browser、files、terminal、aichat……)。
首次启动会现场 `cargo run` 编译应用,进度显示在 tile 里;
之后复用 Cargo 缓存,秒开。

### 手动操作(不用 test-action)

- **⌘Space** 打开应用菜单/搜索,直接搜 "Weather"
- **⌘W** 关 tile,**⌘F** 当前 tile 全屏,**⌘1…0** 切工作区
- 菜单里 Learn → Keybindings 列全部快捷键

### 验证日志

启动后在桌面主日志(重定向 stdout 可得)里找这几行:

```
wm: --test-action launch weather
wm: launched weather as client 1
wm: weather client 1 first frame in NNN ms (cold)   ← 应用已出画面
```

## 三、两个应用目录

| 文件 | 内容 | 何时用 |
|---|---|---|
| `config/apps.json`(默认) | 只有 `reference` | 默认桌面 |
| `config/apps.makepad.json` | Reference + Makepad 全家桶(Browser、Files、Terminal、Weather、Finance、Mail、Notes、Calendar、Reminders、Calculator、Fabric、Score、Video、Route、VJ、Fab、Director、Image、PDF、AI 等) | 想开满应用时用 |

用 `--apps` 选目录:

```sh
cargo run --release -- --apps config/apps.makepad.json
```

个人目录 `~/.octosense/apps.json` 优先级高于项目默认;`--apps` 永远指向你点名的文件。

## 四、目录里为什么应用会"缺席"

`"source": "makepad"` 的行不是按路径找,而是让 Cargo 报告它用哪个 Makepad checkout
来编译本宿主,然后去那个 checkout 里找应用。

1. 宿主启动时跑一次 `cargo metadata --offline --locked`,
   在依赖图里找带 `apps/wm/Cargo.toml` 标记的 Makepad 仓库根。
   (见 `src/octosense/makepad_source.rs`、`src/octosense/catalog.rs`。)
2. 找不到 checkout(比如离线依赖不全),这些行会被**静默跳过**,
   不会报错 —— 日志里只会看到 `no app 'X' in the registry`。

症状:启动后用 `--test-action launch-weather` 打开应用,
日志打 `octosense: no app 'weather' in the registry`,目录里却没有 weather。

原因:Cargo 缓存里缺少 lock 文件锁定的依赖版本(本次是
`agent-client-protocol` 要 1.3.0,缓存只有 1.2.0),
`cargo metadata --offline` 直接失败 → 解析不出 Makepad 根 →
所有 `source: makepad` 行被丢弃。

修复(联网跑一次 metadata 把依赖补进缓存):

```sh
cd OctoSense
cargo metadata --format-version 1 --manifest-path Cargo.toml > /dev/null
```

之后离线 metadata 正常,目录解析出 `/Volumes/PSSD/CodeProjects/makepad`。
(前提:同级框架仓库已经用 `python3 tools/setup-native.py` 准备好。)

## 五、往目录里加一个新应用

目录是一组 JSON 数组,每行三选一:

- `"manifest"` + `"package"` + `"bin"` —— 指到某个 Cargo 工程(相对目录所在目录)
- `"executable"` —— 指到已安装的可执行文件
- `"source": "makepad"` + `"package"` + `"bin"` —— 让宿主按本构建的 Makepad checkout 解析

```json
{
  "id": "notes",
  "label": "Notes",
  "manifest": "../notes/Cargo.toml",
  "package": "my-notes",
  "bin": "notes",
  "policy": "new",
  "args": []
}
```

规则(与 `src/octosense/catalog.rs` 校验一致):

- `id` 只允许小写字母、数字、`-`、`_`;目录内不允许重复
- `label` 必填;`policy` 缺省是 `focus`(已有实例就聚焦),`new` 强制开新实例
- `args` 是字符串数组,原样传给子进程(不经 shell)
- 应用必须支持 `--stdin-loop` 托管协议,且与本宿主同一 Makepad 版本
- 可执行文件托管的条目不要写 `package` / `bin`

放哪:

```sh
# 项目默认目录:改 config/apps.json,重启 OctoSense
# 个人目录(优先于项目默认):~/.octosense/apps.json
# 临时选目录:--apps /path/to/your.json
```

写完之后,应用就会出现在 **⌘Space** 应用菜单里;也可以 `--test-action launch-<id>` 直接打开。
不可用(找不到 manifest / 构建失败)的应用会在桌面日志里报原因,不会崩溃。

## 相关

- 应用目录格式:README「Add an app」章节
- 每个应用托管时自己的日志:`~/.octosense/wm/logs/<app>.log`
- 目录里不存在的 app 快捷键会提示「unavailable」,不会崩溃
