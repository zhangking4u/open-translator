OpenTranslator (Windows)
========================

1. 安装/升级：右键 install.ps1 → "使用 PowerShell 运行"（或在 PowerShell 中执行
   powershell -ExecutionPolicy Bypass -File .\install.ps1）
   - 程序复制到 %LOCALAPPDATA%\Programs\OpenTranslator
   - 自动加入开机启动（开始菜单"启动"文件夹快捷方式，静默启动到托盘）
   - 旧版本正在运行时会先自动结束，安装完成后自动在托盘重启（-NoStart 可跳过）
   - 手动双击 exe 启动时会显示窗口（下载/报错可见），开机启动才静默

2. 首次运行会自动从 ModelScope 下载模型（约 1.1GB，支持断点续传与校验），
   窗口会显示下载进度。

3. 用法：任意应用选中文字，按 Ctrl+Alt+T 弹出翻译；托盘右键菜单提供
   显示窗口 / 历史… / 设置… / 有新版本 / 退出。

4. 设置：托盘右键 →「设置…」（快捷键、模型路径、auto_download、
   check_updates、serve_extension，以及打开配置目录），也可直接编辑
   %APPDATA%\open-translator\config；设置页与托盘均基于 Tauri 客户端。

5. 卸载：powershell -ExecutionPolicy Bypass -File .\install.ps1 -Uninstall
   （或删除上面的程序目录与启动快捷方式）

模型协议为腾讯 Hunyuan 社区许可（HY-MT）；程序本体为 MIT。
