# OpenTranslator

本地优先的 AI 翻译平台：模型推理全部在本机完成，数据不出本机。

三大组件：

| 组件 | 路径 | 说明 |
|---|---|---|
| 核心服务 | `core/translator-service` | Rust + Axum，进程内 llama.cpp 推理（也可用本地 Ollama），默认 `http://127.0.0.1:17890` |
| 桌面划词 | `desktop/translator-popup` | GNOME Wayland 划词弹窗（GTK4），按快捷键翻译选中文本 |
| 浏览器扩展 | `browser/extension` | Firefox 划词翻译（右键菜单 / `Alt+Shift+T`，页内气泡） |

## 快速开始

### 普通用户（Windows / macOS / Linux）

从 [Releases](https://github.com/zhangking4u/open-translator/releases) 下载安装包：

- **Windows**：解压 `OpenTranslator-windows-x64.zip` → 右键 `install.ps1` →「使用 PowerShell 运行」。程序装入 `%LOCALAPPDATA%\Programs\OpenTranslator` 并加入开机启动。
- **macOS**：打开 `OpenTranslator-macos-*.dmg`，把 `OpenTranslator.app` 拖入「应用程序」。首次打开如被 Gatekeeper 拦截，右键 →「打开」或在「系统设置 → 隐私与安全性」中允许（当前未签名）；使用取词功能还需在「辅助功能」中授权。
- **Linux（GNOME Wayland）**：`sudo apt install ./OpenTranslator-linux-x64.deb`，首次启动会自动注册快捷键（默认 `Ctrl+Alt+T`，也可手动运行 `open-translator-setup`）。deb 基于 Ubuntu 24.04 构建，需要 glibc ≥ 2.39（Ubuntu 24.04+ / Debian 13+）与 GTK4 ≥ 4.10。

首次运行会自动从 ModelScope 下载模型（约 1.1GB，带进度与断点续传）。之后任意应用选中文字按 `Ctrl+Alt+T` 即可翻译；托盘/菜单栏图标提供 显示窗口 / 立即翻译 / 退出。

> 安装包由 CI 在打 tag 时生成（`git tag v0.1.0 && git push --tags` 即创建 Release）。

### 开发 / 进阶用户

按下面的步骤从源码运行（Linux 桌面目前走"核心服务 + GTK 弹窗"路径）。

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

### 2. 桌面划词（Ubuntu / GNOME Wayland）

普通用户安装 deb（首次启动自动注册快捷键）；从源码安装：

```bash
sudo apt install -y libgtk-4-dev pkg-config wl-clipboard
./desktop/install.sh                 # 构建两个 crate 并注册快捷键（默认 Ctrl+Alt+T）
```

选中任意文字按快捷键即可弹窗；服务未启动时弹窗会自动拉起核心服务（进程内 llama.cpp），首次运行自动下载模型（约 1.1GB，带进度与断点续传），之后冷启动约 1–2s。
`./desktop/install.sh --uninstall` 卸载快捷键；`--binding "<Control><Alt>t"` 自定义按键。

弹窗配置：`~/.config/open-translator/config`（可选）

```ini
service_url = http://127.0.0.1:17890
source = auto
target = zh
# clipboard = false                  # 改读剪贴板而不是划词选区（弹窗内也可切换）
# model_path = /path/to/model.gguf   # 默认 ~/.local/share/open-translator/models/
# prompt_style = hymt                # generic / translategemma / hymt
# auto_download = true               # 首次运行自动下载默认模型
# check_updates = true               # 启动时检查 GitHub 新版本并提示
```

命令行参数 > 配置文件 > 环境变量 > 默认值。其他参数：`--clipboard`（读剪贴板）、`--stdin`/`--print`（脚本化）、`--no-start`。
`TRANSLATOR_ENGINE=ollama` 可切回本地 Ollama（模型名用 `TRANSLATOR_MODEL`，默认 `hy-mt1.5-1.8b`）；`TRANSLATOR_MODEL_PATH` 指向已有 GGUF 时不触发下载。
弹窗内可直接用「源语言 / 目标语言」下拉切换（源语言含「自动检测」，会识别选中文本的语种并用于翻译；切换后自动重译并写回配置文件）。目标语言支持中文/英语/日语/韩语/法语/德语/西班牙语/俄语。译文通过 `POST /translate/stream` 流式返回，边生成边显示，状态栏同步显示已接收字数。右上角「剪贴板」开关可在划词选区与剪贴板之间切换，选择会写入配置并在下次启动时恢复。

### 3. 浏览器扩展（Firefox / Chrome）

```bash
./browser/build.sh            # 生成 browser/dist/{firefox,chrome}
```

- **Firefox**：`about:debugging#/runtime/this-firefox` → 「临时载入附加组件」→ 选 `browser/dist/firefox/manifest.json`
- **Chrome/Edge**：`chrome://extensions` → 打开「开发者模式」→「加载已解压的扩展程序」→ 选 `browser/dist/chrome`

刷新已打开的网页，选中文字 → 右键「翻译选中文本（OpenTranslator）」或 `Alt+Shift+T`；气泡底部可直接切换目标语言（就地重译），设置页可改服务地址/语言对，并可开启「划词自动翻译」。

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
| 桌面划词 | ✅ GNOME Wayland | ✅ MVP（`translator-popup-desktop`，CI 构建） | ✅ MVP（同客户端，未真机验证） |

- Windows 桌面划词：`powershell -ExecutionPolicy Bypass -File desktop\install-windows.ps1` 构建**内嵌模型推理**的客户端（无需 Ollama/服务进程）并加入开机启动；把 GGUF 放到 `%LOCALAPPDATA%\open-translator\models\hy-mt1.5-1.8b-q4_k_m.gguf`（或配置 `model_path`）。选中文字按 `Ctrl+Alt+T`（模拟 `Ctrl+C` + 剪贴板取词）；托盘菜单提供「显示窗口 / 立即翻译 / 退出」。运行期间在 `127.0.0.1:17890` 提供 HTTP 供浏览器扩展。
- macOS 桌面划词：`./desktop/install-macos.sh` 构建并安装 `~/Applications/OpenTranslator.app`（内嵌推理）并注册 LaunchAgent 开机启动；模型放到 `~/Library/Application Support/open-translator/models/`（或配置 `model_path`）。首次使用需在「系统设置 → 隐私与安全性 → 辅助功能」允许 OpenTranslator（模拟 `Cmd+C` 取词所需）；菜单栏图标提供「显示窗口 / 立即翻译 / 退出」。
- 桌面客户端配置（Windows `%APPDATA%\open-translator\config`、macOS `~/Library/Application Support/open-translator/config`）：`model_path`、`prompt_style`（默认 `hymt`）、`serve_extension`（默认 `true`）、`auto_download`（默认 `true`，首启自动从 ModelScope 下载模型，支持断点续传与 SHA-256 校验）、`check_updates`（默认 `true`，启动时检查 GitHub 新版本并在窗口/托盘提示）、`hotkey`、`source`（默认 `auto`，自动识别源语言）、`target`；环境变量 `TRANSLATOR_MODEL_PATH` / `TRANSLATOR_PROMPT_STYLE` / `TRANSLATOR_HOTKEY` / `TRANSLATOR_CHECK_UPDATES` 可临时覆盖。
- Linux GTK 弹窗（`desktop/translator-popup`）默认同样使用进程内 llama.cpp + 首次运行自动下载模型；`TRANSLATOR_ENGINE=ollama` 可切回外部核心服务 + Ollama。
- Windows/macOS 上也可只用浏览器扩展：安装 Ollama + 运行核心服务（`cargo run --release`）即可；HY-MT 导入脚本（Windows 需 Git Bash）或按脚本内 Modelfile 手动 `ollama create`。

## 常见问题

- **网络**：`ollama.com` 与 HuggingFace 不可达；GitHub release 资产走代理（如 `https://gh-proxy.com/`）；模型从 ModelScope 下载。
- **浏览器扩展连不上服务**：确认核心服务在运行；扩展权限的 match pattern 不能带端口（已用 `http://127.0.0.1/*`）；若 Firefox 配置了代理，确保 localhost 直连。
- **GNOME Wayland 限制**：应用无法指定窗口位置（"贴近鼠标"需 GNOME Shell 扩展）；系统不提供 data-control 协议，选区读取依赖 `wl-paste`（`wl-clipboard` 包）。
- **日志**：服务日志 `~/.local/state/open-translator/{ollama,translator-service}.log`；服务运行日志用 `RUST_LOG` 控制。

## 仓库结构

```
core/translator-service/   核心服务（Rust）
desktop/translator-popup/  桌面弹窗（Rust + GTK4）
desktop/install.sh         桌面端一键安装（构建 + 注册快捷键）
browser/extension/         Firefox 扩展（纯 JS，无构建）
packaging/                 deb / zip / dmg 打包脚本
models/                    HY-MT → Ollama 导入脚本
docs/                      架构、状态、开发日志
```

## 开发

```bash
cd core/translator-service && cargo test    # 核心服务
# Linux 安装包：VERSION=0.1.0 ./packaging/linux/make-deb.sh（先 cargo build --release 两个 crate）
cd core/inference && cargo test             # 进程内推理（构建 llama.cpp 需 cmake + clang/libclang）
cd desktop/translator-core && cargo test    # 桌面共享库（跨平台：参数/配置/翻译调用/服务自启）
cd desktop/translator-popup && cargo test   # 桌面弹窗（Linux/GTK）
cd desktop/translator-popup-desktop && cargo test  # 桌面客户端（Windows/macOS，eframe；CI 双平台构建）
./browser/test.sh                           # Chrome MV3 端到端（服务未运行会自启 mock 引擎）
```

- 架构设计：`docs/ARCHITECTURE.md`
- 当前状态与进度：`docs/PROJECT_STATUS.md`
- 开发日志：`docs/DEVELOPMENT_LOG.md`
- CI：`.github/workflows/ci.yml`（两个 crate 的测试 + 扩展静态检查/lint + Chrome 端到端）

## 许可证

MIT（见 `LICENSE`）
