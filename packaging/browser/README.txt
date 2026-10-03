OpenTranslator 浏览器扩展

所有译文仅在本机生成（默认 http://127.0.0.1:17890），不会上传云端。
使用前请先启动 OpenTranslator 桌面客户端（托盘中保持运行），或自行运行核心服务。

Edge（使用本包的 Chrome 版）
  1. 解压本压缩包，得到本文件夹
  2. 打开 edge://extensions，打开左下角「开发人员模式」
  3. 点击「加载解压缩的扩展」，选择本文件夹
  4. 刷新网页，选中文字 → 右键「翻译选中文本（OpenTranslator）」或按 Alt+Shift+T

Chrome
  1. 打开 chrome://extensions，其余步骤同 Edge（开发者模式 → 加载已解压的扩展程序）

Firefox（注意：本包未签名，只能临时加载，浏览器重启后失效）
  1. 打开 about:debugging#/runtime/this-firefox
  2. 点击「临时载入附加组件」，选择本文件夹内的 manifest.json
  3. 需要永久安装请使用 AMO 签名版 .xpi（如 Release 提供）

使用提示
  - 点击工具栏图标可查看服务状态、最近翻译，切换目标语言与自动翻译开关
  - Alt+Shift+Y 翻译剪贴板；PDF 等无法注入脚本的页面会弹出独立结果窗口
  - 扩展选项页可修改服务地址、源/目标语言、自动翻译延迟与站点开关
