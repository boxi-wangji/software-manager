<p align="center">
  <img src="Logo.svg" width="112" alt="软件管家 Logo">
</p>

<h1 align="center">软件管家</h1>

<p align="center">在 Windows 上集中查询、下载和管理软件安装包。</p>

<p align="center">
  简体中文 · <a href="README.en.md">English</a>
</p>

## 功能

软件管家把 GitHub 便携版、官网安装包和自定义下载源集中到一个本地 Windows 应用中。它可检查版本、缓存安装包、管理便携软件，并引导已适配的安装流程。

不需要账号。设置和下载的安装包保存在程序本地的 `data` 目录。

## 开发

需要 Windows、Node.js、Rust 和 Visual Studio C++ 生成工具。

```powershell
npm install
npm run tauri dev
```

## 构建

```powershell
npm run build
npm run build:installer
```

安装程序输出到 `构建/安装程序`。构建产物和运行数据不会进入 Git。

## 目录

```text
src/          React 界面
src-tauri/    Tauri 与 Rust 后端
scripts/      构建和 Windows 自动化辅助脚本
Logo.svg      正式 Logo 源文件
```

## 安全说明

软件管家可打开用户选择来源的安装包。安装前请自行确认发布者和文件可信。

## 许可证

本项目使用 [GNU GPL v3.0](LICENSE) 许可证发布。
