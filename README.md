# OpenTranslator

本地优先的 AI 翻译平台：模型推理全部在本机完成，数据不出本机。

三大组件：

| 组件 | 路径 | 说明 |
|---|---|---|
| 核心服务 | `core/translator-service` | Rust + Axum，进程内 llama.cpp 推理（也可用本地 Ollama），默认 `http://127.0.0.1:17890` |
| 桌面划词 | `desktop/translator-popup-tauri` | Tauri v2 桌面客户端（Windows/macOS/Linux 发布包），内嵌 llama.cpp，按快捷键翻译选中文本 |
| 浏览器扩展 | `browser/extension` | Firefox 划词翻译（右键菜单 / `Alt+Shift+T`，页内气泡） |

## 快速开始

### 普通用户（Windows / macOS / Linux）

从 [Releases](https://github.com/zhangking4u/open-translator/releases) 下载安装包：

- **Windows**：解压 `OpenTranslator-windows-x64.zip` → 右键 `install.ps1` →「使用 PowerShell 运行」。程序装入 `%LOCALAPPDATA%\Programs\OpenTranslator` 并加入开机启动。
- **macOS**：打开 `OpenTranslator-macos-*.dmg`，把 `OpenTranslator.app` 拖入「应用程序」。首次打开如被 Gatekeeper 拦截，右键 →「打开」或在「系统设置 → 隐私与安全性」中允许（当前未签名）；使用取词功能还需在「辅助功能」中授权。
- **Linux（GNOME）**：`sudo apt install ./OpenTranslator-linux-x64.deb`，安装后运行一次 `open-translator-setup` 绑定全局快捷键（默认 `Ctrl+Alt+T`，同时写入开机自启；安装时会提示）。deb 基于 Ubuntu 24.04 构建，需要 glibc ≥ 2.39（Ubuntu 24.04+ / Debian 13+）；运行时依赖（libwebkit2gtk-4.1-0、libgtk-3-0、libayatana-appindicator3-1、wl-clipboard、libnotify-bin、libgomp1）由 apt 自动安装，同时推荐 speech-dispatcher（朗读译文）与 pkexec（客户端内一键更新）。
- **浏览器扩展**：`OpenTranslator-browser-chrome.zip`（Edge/Chrome）或 `OpenTranslator-browser-firefox.zip`（Firefox），解压后按扩展页面的「加载已解压缩的扩展程序 / 临时载入附加组件」安装，详见「浏览器扩展」一节。

首次运行会在后台从 ModelScope 下载模型（约 1.1GB，带进度与断点续传），托盘/菜单栏提示下载进度；程序开机启动后保持静默、不弹窗，按 `Ctrl+Alt+T` 或在托盘选「显示窗口」才会显示。之后任意应用选中文字按 `Ctrl+Alt+T` 即可翻译；托盘/菜单栏图标提供 显示窗口 / 历史… / 设置… / 有新版本 / 退出。

升级：Windows 重新解压并运行 `install.ps1`（或客户端内「立即更新」）；macOS 用新 dmg 替换应用；Linux 客户端内「更新到 v…」会下载 deb、校验 SHA-256 后通过 pkexec 认证安装，装完自动重启，也可 `sudo apt install ./新版OpenTranslator-linux-x64.deb` 覆盖升级（手动升级后需重启客户端才会加载新版本）。从 v0.2.x 的旧客户端升级到 v0.3.0 的 Tauri 包需要重新下载安装一次（旧客户端无法一键更新）。

> 安装包由 CI 在打 tag 时生成（`git tag v0.2.0 && git push --tags` 即创建 Release）。

### 开发 / 进阶用户

按下面的步骤从源码运行（桌面端为 Tauri 客户端，见第 2 节；核心服务可独立运行）。

### 0. 运行时与模型（Ollama）

```bash
# 本机已安装（用户态）：~/.local/opt/ollama/bin/ollama serve
ollama pull qwen2.5:7b                 # 通用后备模型

# 推荐默认模型：HY-MT（从 ModelScope 下载 GGUF 后导入，HuggingFace 不可达）
#   https://modelscope.cn/models/Tencent-Hunyuan/HY-MT1.5-1.8B-GGUF
./models/import-hymt-ollama.sh <path-to>/HY-MT1.5-1.8B-Q4_K_M.gguf hy-mt1.5-1.8b
```

### 1. 核心服务

```bash
cd core/translator-service
TRANSLATOR_ENGINE=ollama \
TRANSLATOR_MODEL=hy-mt1.5-1.8b \
TRANSLATOR_PROMPT_STYLE=hymt \
cargo run --release
```

- 健康检查：`curl http://127.0.0.1:17890/health`
- 翻译：`curl -X POST http://127.0.0.1:17890/translate -H 'content-type: application/json' -d '{"text":"kernel panic","source":"en","target":"zh"}'`
- 流式翻译（SSE，逐 token 返回，`delta` 事件后以 `done`/`error` 结束）：`curl -N -X POST http://127.0.0.1:17890/translate/stream -H 'content-type: application/json' -d '{"text":"kernel panic","source":"auto","target":"zh"}'`

| 环境变量 | 默认 | 说明 |
|---|---|---|
| `TRANSLATOR_BIND_ADDR` | `127.0.0.1:17890` | 监听地址 |
| `TRANSLATOR_ENGINE` | `mock` | `mock` / `ollama`（需 `TRANSLATOR_MODEL`）/ `llama-cpp`（需 `TRANSLATOR_MODEL_PATH`，进程内推理，无需 Ollama） |
| `TRANSLATOR_MODEL` | — | Ollama 模型名，如 `hy-mt1.5-1.8b` |
| `TRANSLATOR_MODEL_PATH` | — | `llama-cpp` 引擎的 GGUF 模型路径 |
| `TRANSLATOR_N_CTX` | `4096` | `llama-cpp` 引擎的上下文长度 |
| `TRANSLATOR_MODEL_URL` | `http://127.0.0.1:11434` | Ollama 地址 |
| `TRANSLATOR_PROMPT_STYLE` | `generic` | `generic` / `translategemma` / `hymt` |
| `TRANSLATOR_TIMEOUT_MS` | `30000` | 单次翻译超时（>0） |
| `TRANSLATOR_WARMUP` | `true` | 启动时预热模型 |
| `TRANSLATOR_KEEP_ALIVE` | `30m` | 模型空闲卸载时间（Ollama `keep_alive`，如 `5m`/`1h`/`-1`） |
| `TRANSLATOR_MAX_CHARS` | `1500` | 单次请求最大字符数，超出立刻返回 400（避免长文本等到超时） |
| `RUST_LOG` | `info` | 日志级别 |

### 2. 桌面划词（Tauri 客户端）

发布版：Linux 安装 deb 后运行一次 `open-translator-setup`；Windows/macOS 见上面的安装步骤。从源码构建 Tauri 客户端（Linux）：

```bash
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf pkg-config wl-clipboard
cargo build --release --manifest-path desktop/translator-popup-tauri/Cargo.toml
# 可选，生成 deb：VERSION=0.3.0 ./packaging/linux/make-deb.sh
```

选中任意文字按快捷键即可弹窗；客户端内嵌 llama.cpp 推理（无需外部服务），首次运行自动下载模型（约 1.1GB，带进度与断点续传），之后冷启动约 1–2s；`TRANSLATOR_MODEL_PATH` 指向已有 GGUF 时不触发下载。

客户端配置：`~/.config/open-translator/config`（Windows `%APPDATA%\open-translator\config`、macOS `~/Library/Application Support/open-translator/config`，可选）

```ini
service_url = http://127.0.0.1:17890
source = auto
target = zh
# model_path = /path/to/model.gguf   # 默认 ~/.local/share/open-translator/models/
# prompt_style = hymt                # generic / translategemma / hymt
# auto_download = true               # 首次运行自动下载默认模型
# check_updates = true               # 启动时检查 GitHub 新版本并提示
# hotkey = Ctrl+Alt+T                # 全局快捷键（也可用 TRANSLATOR_HOTKEY 覆盖）
# serve_extension = true             # 运行期间在 127.0.0.1:17890 提供 HTTP 供浏览器扩展
```

命令行参数 > 配置文件 > 环境变量 > 默认值。参数：`--autostart`（登录自启静默进托盘）、`--translate`（取词翻译并弹窗）、`--settings` / `--history`（打开设置/历史页）、`--stdin`/`--print`（脚本化）。

弹窗内可用「源语言 / 目标语言」下拉切换（源语言含「自动检测」，会识别选中文本的语种并用于翻译；切换后自动重译并写回配置文件）。目标语言支持中文/英语/日语/韩语/法语/德语/西班牙语/俄语；⇄ 互换语言对，`Ctrl+1/2/3` 切换最近使用的目标语言。译文流式显示并同步显示已接收字数。卡片提供 复制译文 / 替换原文（Windows 与 Linux/X11）/ 朗读（Windows/macOS 用系统语音，Linux 经 speech-dispatcher，按目标语言选择语音，无对应语音时不显示）/ 重新翻译 / 固定，`Ctrl+Enter` 重译、`Ctrl+Shift+C` 复制；托盘菜单提供 显示窗口 / 历史… / 设置… / 有新版本 / 退出。

### 3. 浏览器扩展（Edge / Chrome / Firefox）

从 [Releases](https://github.com/zhangking4u/open-translator/releases/latest) 下载对应压缩包（内含安装说明）：

- **Edge / Chrome**：`OpenTranslator-browser-chrome.zip` → 解压 → `edge://extensions` 或 `chrome://extensions` → 打开「开发人员模式」→「加载已解压的扩展程序」→ 选解压出的文件夹
- **Firefox**：`OpenTranslator-browser-firefox.zip` → 解压 → `about:debugging#/runtime/this-firefox` →「临时载入附加组件」→ 选文件夹内的 `manifest.json`（未签名包重启浏览器后失效；永久安装见文末签名说明）

源码调试：`./browser/build.sh [firefox|chrome|all] [--zip]` 生成 `browser/dist/{firefox,chrome}`，加载方式同上。

刷新已打开的网页，选中文字 → 右键「翻译选中文本（OpenTranslator）」或 `Alt+Shift+T`；气泡流式显示译文，默认只保留目标语言、复制和「⋯」菜单（复制双语/原文、朗读、替换原文、重新翻译、互换语言、语言设置），翻译中只显示「停止」、出错只显示「重试」。`Alt+Shift+Y` 翻译剪贴板；PDF 等无法注入脚本的页面会弹出独立结果窗口。点击工具栏图标查看服务状态与最近翻译、快速修改目标语言/自动翻译开关与当前网站开关；设置页可改服务地址、源语言、自动翻译延迟/最短字符数，并管理已关闭自动翻译的站点。

**边写边译**（默认关闭）：在工具栏弹窗或设置页开启后，在任意输入框/文本域/富文本编辑器（含网页版邮件、飞书、Notion 等）打字，停顿约 0.5 秒即在其光标处流式提示当前句子的译文，按 `Tab` 用译文替换原句、`Esc` 忽略；中文等输入法组合输入不会误触发，密码框不生效，不会写入历史（按 `Tab` 采用后才记录）。设置页可调最短字符数与停顿延迟；「本网站不自动翻译」的站点同时禁用边写边译。气泡左下角可切换目标语言（切换后立即重译当前句，输入框不丢焦点），右侧齿轮直接打开设置页；`Alt+Shift+L` 可在气泡显示时循环切换最近使用的目标语言（可在浏览器扩展快捷键页改绑）。

> Firefox 的正式版本已按上述流程签名（AMO unlisted，自动审核通过）并在本机永久安装；临时加载仅用于开发调试。

**长期安装（Firefox，AMO unlisted 签名）**

```bash
./browser/build.sh firefox --zip                   # 产出 dist/open-translator-firefox-<version>.zip
export WEB_EXT_API_KEY=... WEB_EXT_API_SECRET=...  # 一次性申请：https://addons.mozilla.org/developers/addon/api/key/
./browser/sign.sh                                  # 签名，产出 browser/dist/signed/*.xpi
```

签完在 `about:addons` → 齿轮 → 「从文件安装附加组件」选择 `.xpi` 即可永久安装（也可在 AMO 开发者页直接下载签名文件）。注意：**版本号不能复用**，再次签名前先提升 manifest 版本；`web-ext sign` 的最后一步才是下载 xpi，终端请等它完整结束。也可用 Developer Edition/Nightly（`xpinstall.signatures.required=false`）安装未签名包。

## 平台支持

| 组件 | Linux | Windows | macOS |
|---|---|---|---|
| 核心服务 | ✅ | ✅（CI `windows-latest` 每提交测试） | 理论可用，未验证 |
| 浏览器扩展 | ✅ | ✅ | ✅ |
| 桌面划词 | ✅ Tauri（deb；Wayland 会话走 XWayland） | ✅ Tauri（zip/install.ps1，真机验证通过） | ✅ Tauri（dmg，CI 构建，未真机验证） |

- Windows 桌面划词：`powershell -ExecutionPolicy Bypass -File desktop\install-windows.ps1` 构建**内嵌模型推理**的 Tauri 客户端（`translator-popup-tauri.exe`）并加入开机启动；把 GGUF 放到 `%LOCALAPPDATA%\open-translator\models\hy-mt1.5-1.8b-q4_k_m.gguf`（或配置 `model_path`）。选中文字按 `Ctrl+Alt+T`（模拟 `Ctrl+C` + 剪贴板取词）；托盘菜单提供 显示窗口 / 历史… / 设置… / 有新版本 / 退出。运行期间在 `127.0.0.1:17890` 提供 HTTP 供浏览器扩展；客户端内「立即更新」会下载 `OpenTranslator-windows-x64.zip`、解压并运行其中的 `install.ps1`（会先关闭运行中的旧版）。
- macOS 桌面划词：发布包为 dmg（把 `OpenTranslator.app` 拖入「应用程序」）；源码安装 `./desktop/install-macos.sh` 构建内嵌推理的 Tauri 客户端到 `~/Applications/OpenTranslator.app` 并注册 LaunchAgent 开机启动。模型放到 `~/Library/Application Support/open-translator/models/`（或配置 `model_path`）。首次使用需在「系统设置 → 隐私与安全性 → 辅助功能」允许 OpenTranslator（模拟 `Cmd+C` 取词所需）；菜单栏图标提供 显示窗口 / 历史… / 设置… / 有新版本 / 退出（更新在浏览器打开 release 页）。
- Linux 桌面划词：发布包为 deb（安装后运行一次 `open-translator-setup` 注册快捷键与开机自启）。客户端优先使用 X11（XWayland）：复制写入 X11 剪贴板（Wayland 应用可粘贴），取词读 `wl-paste --primary` 并以 X11 PRIMARY 兜底（纯 Xorg 会话可用），替换原文对 X11/XWayland 源窗口可用（原生 Wayland 应用受协议限制不提供），朗读经 speech-dispatcher（`spd-say`）。客户端内「更新到 v…」下载 deb 后调用 `pkexec apt-get install` 安装并自动重启；缺少 pkexec/apt 时回退为打开 release 页。
- 桌面客户端配置（Windows `%APPDATA%\open-translator\config`、macOS `~/Library/Application Support/open-translator/config`）：`model_path`、`prompt_style`（默认 `hymt`）、`serve_extension`（默认 `true`）、`auto_download`（默认 `true`，首启自动从 ModelScope 下载模型，支持断点续传与 SHA-256 校验）、`check_updates`（默认 `true`，启动时检查 GitHub 新版本并在窗口/托盘提示）、`hotkey`、`source`（默认 `auto`，自动识别源语言）、`target`；环境变量 `TRANSLATOR_MODEL_PATH` / `TRANSLATOR_PROMPT_STYLE` / `TRANSLATOR_HOTKEY` / `TRANSLATOR_CHECK_UPDATES` 可临时覆盖。
- Windows/macOS 上也可只用浏览器扩展：安装 Ollama + 运行核心服务（`cargo run --release`）即可；HY-MT 导入脚本（Windows 需 Git Bash）或按脚本内 Modelfile 手动 `ollama create`。

## 常见问题

- **网络**：`ollama.com` 与 HuggingFace 不可达；GitHub release 资产走代理（如 `https://gh-proxy.com/`）；模型从 ModelScope 下载。
- **浏览器扩展连不上服务**：确认核心服务在运行；扩展权限的 match pattern 不能带端口（已用 `http://127.0.0.1/*`）；若 Firefox 配置了代理，确保 localhost 直连。
- **GNOME Wayland 限制**：原生 Wayland 不支持置顶（keep-above），托盘点击也没有激活令牌、GNOME 不允许后台窗口置顶/聚焦；Tauri 客户端因此在 Wayland 会话下默认走 XWayland（`GDK_BACKEND=wayland` 可退回原生 Wayland，此时「固定」仅阻止隐藏）。系统不提供 data-control 协议，选区读取依赖 `wl-paste`（`wl-clipboard` 包）。
- **日志**：服务日志 `~/.local/state/open-translator/{ollama,translator-service}.log`；服务运行日志用 `RUST_LOG` 控制。

## 仓库结构

```
core/translator-service/          核心服务（Rust）
core/inference/                   进程内 llama.cpp 推理（Rust）
desktop/translator-popup-tauri/   桌面客户端（Tauri v2，Windows/macOS/Linux 发布包）
desktop/install-windows.ps1       Windows 源码安装（Tauri）
desktop/install-macos.sh          macOS 源码安装（Tauri）
browser/extension/                Firefox 扩展（纯 JS，无构建）
packaging/                        deb / zip / dmg 打包脚本
models/                           HY-MT → Ollama 导入脚本
docs/                             架构、状态、开发日志
```

## 开发

```bash
cd core/translator-service && cargo test    # 核心服务
cd core/inference && cargo test             # 进程内推理（构建 llama.cpp 需 cmake + clang/libclang）
cd desktop/translator-core && cargo test    # 桌面共享库（跨平台：参数/配置/翻译调用/服务自启）
cd desktop/translator-popup-tauri && cargo test  # 桌面客户端（Tauri；Linux 需 webkit2gtk-4.1 等构建依赖）
./browser/test.sh                           # Chrome MV3 端到端（服务未运行会自启 mock 引擎）

# Linux 安装包：先 cargo build --release --manifest-path desktop/translator-popup-tauri/Cargo.toml
#            再 VERSION=0.3.0 ./packaging/linux/make-deb.sh
```

- 架构设计：`docs/ARCHITECTURE.md`
- 当前状态与进度：`docs/PROJECT_STATUS.md`
- 开发日志：`docs/DEVELOPMENT_LOG.md`
- CI：`.github/workflows/ci.yml`（core/inference/desktop-core 测试、core Windows 测试、Tauri 三平台测试与前端静态检查、扩展静态检查/lint + Chrome 端到端）

## 许可证

MIT（见 `LICENSE`）
