# OpenTranslator

本地优先的 AI 翻译平台：模型推理全部在本机完成，数据不出本机。

三大组件：

| 组件 | 路径 | 说明 |
|---|---|---|
| 核心服务 | `core/translator-service` | Rust + Axum，调用本地 Ollama，默认 `http://127.0.0.1:17890` |
| 桌面划词 | `desktop/translator-popup` | GNOME Wayland 划词弹窗（GTK4），按快捷键翻译选中文本 |
| 浏览器扩展 | `browser/extension` | Firefox 划词翻译（右键菜单 / `Alt+Shift+T`，页内气泡） |

## 快速开始

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

```bash
sudo apt install -y libgtk-4-dev pkg-config wl-clipboard
./desktop/install.sh                 # 构建两个 crate 并注册快捷键（默认 Ctrl+Alt+T）
```

选中任意文字按快捷键即可弹窗；服务未启动时弹窗会自动拉起 Ollama 与核心服务（冷启动约 3s）。
`./desktop/install.sh --uninstall` 卸载快捷键；`--binding "<Control><Alt>t"` 自定义按键。

弹窗配置：`~/.config/open-translator/config`（可选）

```ini
service_url = http://127.0.0.1:17890
source = en
target = zh
```

命令行参数 > 配置文件 > 环境变量 > 默认值。其他参数：`--clipboard`（读剪贴板）、`--stdin`/`--print`（脚本化）、`--no-start`。
弹窗内可直接用「目标语言」下拉切换（支持中文/英语/日语/韩语/法语/德语/西班牙语/俄语，切换后自动重译并写回配置文件）。`source` 可设为 `auto` 自动识别源语言。

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

- Windows 桌面划词：`powershell -ExecutionPolicy Bypass -File desktop\install-windows.ps1` 会构建核心服务与弹窗并加入开机启动；选中文字按 `Ctrl+Alt+T`（通过模拟 `Ctrl+C` + 剪贴板取词，目标应用需支持复制）。托盘菜单提供「显示窗口 / 立即翻译 / 退出」。`-Uninstall` 卸载。
- macOS 桌面划词：`./desktop/install-macos.sh` 构建并安装 `~/Applications/OpenTranslator.app`（内含核心服务）并注册 LaunchAgent 开机启动（`--uninstall` 卸载）。首次使用需在「系统设置 → 隐私与安全性 → 辅助功能」中允许 OpenTranslator（模拟 `Cmd+C` 取词所需）；菜单栏图标提供「显示窗口 / 立即翻译 / 退出」；Ollama 可在 GitHub 可达时用 `brew install ollama`。
- 热键与语言对可在配置文件中改（Windows `%APPDATA%\open-translator\config`、macOS `~/Library/Application Support/open-translator/config`），键位：`hotkey = Ctrl+Alt+T`（也支持 `Ctrl+Shift+Space`、`Alt+F2` 等，辅助键可用 Ctrl/Alt/Shift/Meta，主键支持字母数字/空格/回车/Tab/F1–F12）；临时覆盖用环境变量 `TRANSLATOR_HOTKEY`。
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
models/                    HY-MT → Ollama 导入脚本
docs/                      架构、状态、开发日志
```

## 开发

```bash
cd core/translator-service && cargo test    # 核心服务
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
