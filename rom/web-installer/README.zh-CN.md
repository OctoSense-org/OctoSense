# OctoSense Installer — 开发预览版

[English](README.md) | 简体中文

这是通过 WebUSB 为 OnePlus 6 安装 OctoSense ROM 的静态网页工具。在完成发布签名验证和真机验证之前，公共网站上的刷写功能保持禁用。架构及公开发布条件见 [ADR 0001（英文）](../docs/adr/0001-public-web-installer.md)。

## 在浏览器中刷入 ROM

本指南介绍在电脑上运行的本地预览版，目前还没有开放刷写功能的公共网站。**准确识别 OnePlus 6 的方法仍待真机验证**：如果页面无法确认手机身份，安装按钮会保持禁用。下面的步骤不会绕过这个检查。

当前安装器界面为英文，因此本指南保留英文按钮名，并在旁边说明中文含义。

### 1. 准备电脑、手机和镜像

- 使用电脑版 Chrome 或 Edge，以及支持数据传输的 USB 线。下面的命令适用于已安装 Git 和 Python 3 的 macOS 或 Linux。完整的浏览器、操作系统和真机兼容性验证尚未完成。
- 机型必须是 OnePlus 6（一加 6，`enchilada`）。OnePlus 6T（`fajita`）属于另一个目标机型。
- 开始前备份手机并充好电。**解锁 bootloader 和全新安装都会清除手机数据**，包括应用、账号和文件。
- 从同一次可信的 OctoSense 开发构建中获取完整镜像，将以下文件放在电脑上的同一个目录内：

  ```text
  system.img
  vendor.img
  dtbo.img
  vbmeta.img
  boot.img
  ```

  可以使用构建输出中的分区镜像，或由该构建维护者提供的镜像包。OTA ZIP、`payload.bin`、Home APK 和 `update.json` 不能代替这五个文件。不要混用不同构建的镜像。请向构建维护者确认所需的底层固件版本；预览版尚不能自动检查这个条件。
- 电脑需要空间存放原始镜像，以及浏览器下载保存的另一份镜像。安装器会在下载前检查浏览器的可用存储配额。

电脑浏览器通过 USB 直接与手机的 bootloader 通信，手机不需要 root 权限，也不需要安装终端应用。

### 2. 在电脑上启动安装页面

如果还没有检出源码：

```sh
git clone https://github.com/OctoSense-org/OctoSense-ROM.git
cd OctoSense-ROM
```

在仓库根目录运行下面的命令，把 `/absolute/path/to/rom-images` 替换成存放五个镜像的**绝对路径**。命令会生成 `manifest.json`，准备一个独立的临时目录，并启动仅本机可访问的 HTTP 服务。保持这个终端运行：

```sh
(
  set -eu
  octosense_images="/absolute/path/to/rom-images"
  python3 scripts/make-manifest.py "$octosense_images" "OctoSense development" enchilada

  octosense_preview="$(mktemp -d "${TMPDIR:-/tmp}/octosense-web.XXXXXX")"
  cp -R web-installer/index.html web-installer/fastboot.mjs \
    web-installer/src web-installer/vendor "$octosense_preview/"
  for partition in system vendor dtbo vbmeta boot; do
    ln -s "$octosense_images/$partition.img" "$octosense_preview/$partition.img"
  done
  cp "$octosense_images/manifest.json" "$octosense_preview/manifest.json"
  python3 -m http.server 8321 --bind 127.0.0.1 --directory "$octosense_preview"
)
```

使用电脑版 Chrome 或 Edge 打开 **[http://127.0.0.1:8321](http://127.0.0.1:8321)**，确认页面显示预期的版本名称和构建时间。请打开这个 HTTP 地址，不要直接双击 `index.html`。运行安装页面无需执行 npm 安装或 JavaScript 构建。

刷机及后续恢复检查期间，应始终使用同一浏览器配置、同一主机名和端口，因为操作日志保存在这个站点对应的浏览器存储中。服务运行时不要修改源镜像。所有设备操作结束后，在终端按 **Ctrl+C** 停止服务。

已经配置了私有构建主机的维护者，也可以使用 `scripts/release.sh <build-tag> --serve`。省略 `--serve` 时只准备本地构建产物。这两种调用都不会重启手机、刷写手机或公开发布版本。

### 3. 进入 bootloader 并连接

1. 关闭其他安装器标签页，以及正在占用这部手机的 USB 刷机工具。
2. 将 OnePlus 6 关机，再按住 **音量上 + 电源键** 进入 bootloader。这是 [LineageOS 的 OnePlus 6 设备说明](https://github.com/LineageOS/lineage_wiki/blob/main/_data/devices/enchilada.yml) 中记录的按键组合。
3. 用 USB 线将手机直接连接到电脑的 USB 端口。
4. 点击 **Connect phone**（连接手机）。如果浏览器弹出 USB 设备选择框，选中这部手机并允许连接。
5. 检查手机信息和 bootloader 状态。只有页面确认设备身份、分区布局与当前安装方案匹配时，才继续下一步。

仅有 `sdm845` 这个芯片标识无法确定设备是一加 6。如果页面显示 **model unverified**（机型未确认），请停止并保存下文所述的诊断报告。当前没有受支持的强制安装选项。

### 4. 如果 bootloader 已锁定，先解锁

如果页面已经显示 **Unlocked**（已解锁），跳过此步。锁定的手机需要先在 Android 的开发者选项中允许 **OEM 解锁**，bootloader 才能接受解锁请求。如果还没有启用，请先返回 Android 设置，再进入 bootloader 重新连接。

安装器确认手机身份后，阅读并勾选单独的解锁数据清除确认项，然后点击 **Unlock bootloader**（解锁引导加载程序），按手机屏幕提示确认。等待解锁及可能发生的重启完成，再返回 bootloader，重新点击 **Connect phone**。页面必须重新读取到 **Unlocked** 状态才能继续。

### 5. 校验并安装

1. 点击 **Download and verify release**（下载并校验版本），等待显示 **All images verified**（全部镜像校验通过）。安装器会先保存并完整校验全部镜像，再开始写入任何分区。
2. 核对版本以及全新安装的数据清除说明，勾选 **I have backed up what I need and agree to erase all phone data**（我已备份所需数据，并同意清除手机上的全部数据）。
3. 点击 **Erase data and install OctoSense**（清除数据并安装 OctoSense）。保持 USB 连接和电脑唤醒，不要关闭网页或本地 HTTP 服务。
4. 等待 **Images written and data erased. Ready to restart**（镜像写入且数据清除完成，可以重启）。如果写入失败，请按下文处理恢复问题，不要直接重新执行安装。

### 6. 重启并确认手机正常启动

当 **Restart phone**（重启手机）按钮可用时点击它。首次启动可能需要几分钟。完成手机上的初始设置，只有在**手机实体屏幕上确实看到了 OctoSense Home** 后，才点击 **I can see OctoSense Home**（我已看到 OctoSense Home）。这一步记录的是你的人工确认，尚不能自动核验手机实际运行的构建版本。

后续受支持的升级请使用 [ROM 更新流程（英文）](../docs/updates.md)。浏览器预览版当前执行的是会清除数据的全新安装。

### 常见问题

| 现象 | 处理方式 |
| --- | --- |
| 页面打不开，或终端显示 `Address already in use` | 确认本地服务正在运行，且 8321 端口对应本次预览。刷机过程中不要替换正在使用的服务。 |
| 没有版本信息、HTTP 404 或 manifest 无效 | 确认五个镜像来自同一构建且 manifest 生成成功。HTML、`src/`、`vendor/`、`fastboot.mjs`、manifest 和镜像需要由同一个目录提供。 |
| 浏览器不受支持，或 Connect 按钮不可用 | 使用电脑版 Chrome/Edge 打开本地 HTTP 地址。页面需要 WebUSB、Web Locks、WebCrypto 和浏览器文件存储功能。 |
| USB 选择框里没有手机，或连接被拒绝 | 检查 bootloader 模式、数据线和浏览器权限，关闭占用手机的其他工具。系统 USB 权限或驱动也可能需要配置，参见 [Chrome WebUSB 文档](https://developer.chrome.com/docs/capabilities/usb)。 |
| 无法确认机型、模式、槽位或分区状态 | 停止操作并保存诊断报告，不要修改 manifest 或代码来强行匹配。真机验证仍待完成。 |
| 存储空间不足、校验失败或下载失败 | 准备阶段尚未写入分区。释放空间或获取完整且匹配的镜像后，重新执行校验。 |
| 写入、USB 断连或刷新页面后出现 Recovery review required | 保持手机处于 bootloader 模式并保存报告。不要清除浏览器站点数据、切换站点地址绕过日志，或直接重复安装。 |

保存报告时，展开 **Technical details and support report**（技术详情和诊断报告），点击 **Download support report**（下载诊断报告），交给构建维护者检查恢复方式。预览版不会自动恢复，也不会续写中断的刷机任务。[手动刷机与恢复记录（英文）](../docs/flashing.md) 介绍了另一条通过 recovery 操作的流程；发出后续命令前，需要根据手机的实际状态确定适用的恢复步骤。

## 当前实现

- 要求完整的 schema 1 开发版 manifest，包含 system、vendor、dtbo、vbmeta 和 boot 五个镜像，以及完整 SHA-256、文件大小、展开后的大小和固定槽位规则。旧格式的 manifest 需要重新生成。
- 检查准确机型、bootloader 模式、解锁状态、槽位、分区布局和传输能力。`sdm845` 本身不足以确定机型；适配器还会读取 `getvar:device`，但该识别方式尚未在 OnePlus 6 真机上完成验证。缺失或含糊的信息会阻止写入，即使页面运行在本机。当前不支持 OnePlus 6T。
- 将全部镜像暂存到浏览器的站点私有文件存储（OPFS），在 worker 中逐字节计算哈希，并在首次刷写前检查原始或稀疏镜像的结构。哈希校验不需要把整个多 GB 镜像一次性读入 JavaScript 内存。
- 全新安装和 bootloader 解锁分别要求确认清除数据。当前不提供保留数据的升级、重新锁定 bootloader、root 或临时启动内核功能。
- 使用 Web Locks 排除并发操作，绑定选定的 USB 设备和已确认的槽位，并在写入前重新读取设备状态。不会自动切换到新接入的手机，也不会自动重试或重放中断的分区写入。
- 在 USB 写入前记录操作意图。部分完成或结果不确定的安装会阻止再次执行普通安装，即使刷新页面或打开另一个标签页也一样。自动恢复方案尚未实现，需要人工检查。
- 区分“镜像写入完成”“已请求重启”和“用户确认看到 Home”。人工确认不等于自动核验运行中的构建版本。

运行环境需要安全上下文、WebUSB、Web Locks、WebCrypto，以及足够的站点私有存储配额。浏览器存储不能作为长期备份，关闭标签页会中断准备工作；强制结束标签页还可能遗留临时文件。自动清理遗留文件和断点下载尚未实现。不要通过清除站点存储来绕过未解决的写入记录。

## 开发与验证

CI 使用 Node 24 和 Python 3。安装页面本身无需 JavaScript 构建，npm 依赖只用于测试：

```sh
cd web-installer
npm ci --ignore-scripts
npm test
npx playwright install chromium
npm run test:browser
cd ..
python3 -m unittest discover -s tests -p test_installer_manifest.py -v
bash -n scripts/release.sh
```

测试不会操作真实手机。Chromium 测试运行实际页面、worker、OPFS 和 Web Locks，USB 与下载使用模拟数据。覆盖范围包括 64 MiB 镜像末尾损坏、manifest 缺少镜像、后续下载或格式检查失败、不兼容设备、多标签页冲突、写入中断、刷新后恢复检查、公共站点写入限制、键盘确认和窄屏布局。其他测试使用模拟 USB 句柄检查真实 fastboot 适配器，将哈希结果与 Node crypto 对比，并使用生成的原始或稀疏镜像验证打包脚本。

`src/contracts.mjs` 负责 manifest 和设备检查，`session.mjs` 负责操作顺序和日志，`transport.mjs` 封装仓库内的 fastboot 库，下载 worker 与 `image-format.mjs` 验证暂存文件，`app.mjs` 渲染页面。运行时依赖固定为本地源码：`android-fastboot` 1.1.3，以及 `@noble/hashes` 2.4.0 的 SHA-256 相关文件。来源与许可见 [NOTICE](../NOTICE) 和 [哈希依赖完整性记录](vendor/noble-hashes/provenance.json)。

## 后续工作

1. 使用专门的 OnePlus 6 测试设备验证准确身份、模式、固件要求和分区规则，并实现经过测试的恢复及启动检查。
2. 为发布 manifest 签名，定义密钥轮换和发布渠道晋级流程，拒绝过期产物、失败构建和包含开发用 ADB 预授权的公开镜像，发布内容固定的镜像并验证浏览器下载行为。
3. 完成安装器界面的中文化、各操作系统的 USB 帮助以及公开网站集成。在发布公开测试版前，完成浏览器、电脑系统和 ROM 真机测试。显示驱动补丁仍是独立工作。
