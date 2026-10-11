# 后端登录验收

[English](README.md) | 简体中文

这是内部测试夹具，用于验证正式的签名安装、宿主授权界面、浏览器回调、凭据保险库、受保护身份读取、退出登录和应用间连接隔离，不是 App Hub 投稿。生成的上架图片明确标为占位图；每次运行的原始原生窗口和浏览器截图才是独立的验收证据。

`main.splash` 不包含密码输入框。模拟后端在独立 Chrome 会话中提供注册和登录表单；宿主通过授权码、PKCE 和正常平台凭据保险库完成登录，不注入令牌或账户。只有非默认验收构建允许本机 HTTP 回环测试端点，普通构建没有该例外及其注册接口。

构建开发示例：

```sh
cargo build --locked --release -p octosense-shell \
  --features mobile-apps,acceptance-fixtures \
  --example connected-app-host --example connected-install
```

使用已有 Chrome 可执行文件及安装了 Playwright 的 Python 环境。将 `CHROME`、`HUB` 和 `RUN_DIRECTORY` 设为本地路径；运行目录必须尚不存在。此流程已在 macOS 上使用已有 Chrome 和 Playwright 环境运行验证。

```sh
python tools/connected-e2e/backend_login.py \
  --binary target/release/examples/connected-app-host \
  --installer target/release/examples/connected-install \
  --hub "$HUB" --chrome "$CHROME" --out "$RUN_DIRECTORY"
```

驱动通过 Makepad instrument 注入原生输入并保存原始 PNG。它读取宿主 LinkLabel 中的实际授权地址，在独立浏览器中打开，不点击会启动系统默认浏览器的链接。示例程序显式的 `--capture-browser-url` 参数仅在 `acceptance-fixtures` 构建中可用，将地址写入新建的 0600 私有文件，不改变授权及回调处理。输入事件绝不重放；只读截图重试单独记录。

测试覆盖浏览器新用户注册、登录、受保护数据、宿主进程重启、退出、再次登录，以及两个分别准入的应用尝试访问彼此连接。成功结束时通过宿主断开两个虚构账户，停止自行启动的原生进程和模拟服务器。运行失败后，驱动也会通过宿主尝试清理，并记录本地连接元数据是否已移除。它请求平台保险库删除凭据，但不独立读回验证删除结果。若清理失败，删除配置前应启动同一隔离宿主并断开虚构账户。

运行目录必须保密，内有临时授权地址、宿主元数据和诊断日志。仅发布审查过的虚构场景原始截图和脱敏回执，不复制配置目录、回调地址、注册信息或原始日志。功能断言不等于视觉验收；模拟测试也不证明真实 GitHub/Google 授权、Android 回调或后端 WebView 已受支持。


2026-10-07 的原生运行通过了七项检查，二进制 SHA-256 为 `7798c1fd23ae092a323177d1af7d3118d3f117941711140616617f3969fd37c7`：

1. 创建虚构账户，在浏览器登录，经实际 PKCE 回调读取受保护身份。
2. 45 秒令牌触发刷新；一次受保护数据 HTTP 503 后，使用已持久保存的轮换凭据再次刷新并成功读取身份。
3. 关闭并重新启动原生进程，保留同一应用连接，通过正常平台保险库重新读取受保护数据。
4. 退出后本地连接句柄移除，后续受保护数据请求被拒绝。
5. 已有虚构用户再次通过浏览器登录，得到新连接。
6. 第二个签名应用在自己登录之前和之后，都不能使用第一个应用的句柄。
7. 通过宿主撤销两个本地连接，已请求保险库删除凭据，未独立验证删除结果。

查看[脱敏回执和虚构场景原始截图](../evidence/backend-login-20261007/README.zh-CN.md)。前两次尝试发现测试服务器的浏览器表单策略过严；第三次发现驱动在宿主弹层关闭前点击应用。这些失败尝试单独保留。本驱动不会自动操作真实 GitHub 或 Google 登录。
