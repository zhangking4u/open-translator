OpenTranslator (Windows)
========================

1. 安装：右键 install.ps1 → "使用 PowerShell 运行"（或在 PowerShell 中执行
   powershell -ExecutionPolicy Bypass -File .\install.ps1）
   - 程序复制到 %LOCALAPPDATA%\Programs\OpenTranslator
   - 自动加入开机启动（开始菜单"启动"文件夹快捷方式）

2. 首次运行会自动从 ModelScope 下载模型（约 1.1GB，支持断点续传与校验），
   窗口会显示下载进度。

3. 用法：任意应用选中文字，按 Ctrl+Alt+T 弹出翻译；托盘图标提供
   显示窗口 / 立即翻译 / 退出。

4. 设置：%APPDATA%\open-translator\config（model_path、prompt_style、
   hotkey、source、target、serve_extension、auto_download）。

5. 卸载：powershell -ExecutionPolicy Bypass -File .\install.ps1 -Uninstall
   （或删除上面的程序目录与启动快捷方式）

模型协议为腾讯 Hunyuan 社区许可（HY-MT）；程序本体为 MIT。
