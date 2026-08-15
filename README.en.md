<p align="center">
  <img src="Logo.svg" width="112" alt="Software Manager logo">
</p>

<h1 align="center">Software Manager</h1>

<p align="center">A Windows desktop app for finding, downloading, and managing software installers.</p>

<p align="center">
  <a href="README.md">简体中文</a> · English
</p>

## What it does

Software Manager brings GitHub portable releases, official installers, and custom download sources into one local Windows app. It can check releases, cache installers, manage portable apps, and guide supported installer flows.

No user account is required. Application settings and downloaded packages stay in the app's local `data` directory.

## Development

Requirements: Windows, Node.js, Rust, and the Visual Studio C++ build tools.

```powershell
npm install
npm run tauri dev
```

## Build

```powershell
npm run build
npm run build:installer
```

The installer is written to `构建/安装程序`. Build output and runtime data are intentionally excluded from Git.

## Project layout

```text
src/          React interface
src-tauri/    Tauri and Rust backend
scripts/      Build and Windows automation helpers
Logo.svg      Official logo source
```

## Safety note

Software Manager can open installers obtained from sources selected by the user. Always verify the publisher and file before installation.

## License

Released under [GNU GPL v3.0](LICENSE).
