# 软件管家

Windows 桌面软件管理工具。前端是 React + TypeScript + Vite，后端是 Tauri 2 + Rust。它负责查询软件版本、下载或缓存安装包、安装/卸载、管理自定义下载源，以及执行 Windows 安装器自动化。

这份 README 同时是项目的主架构说明。后续 AI 或开发者修改前先读这里。

## 1. 项目边界

源码根目录：

```text
C:\test\learn3\software-manager
```

这里是**唯一源码真相**。桌面上的便携版 EXE、`release/` 里的产物和 `dist/` 都是构建结果，不要把它们当成主要编辑对象。

## 2. 启动与构建

```powershell
npm install
npm run tauri dev
```

常用命令：

```text
npm run build             # TypeScript 检查并构建前端到 dist/
npm run build:exe         # 构建 Tauri EXE（不制作安装包）
npm run build:portable    # 生成便携版发布目录
npm run hot-update        # 打包后重新启动便携版
```

`npm run dev` 只启动 Vite 前端；需要完整 Windows/Tauri 能力时使用 `npm run tauri dev`。

## 3. 主目录结构

```text
software-manager/
├─ src/                               # React 前端
│  ├─ main.tsx                        # React 启动入口
│  ├─ App.tsx                         # 主界面、页面状态、Tauri invoke 调用
│  └─ App.css                         # 主界面样式
│
├─ src-tauri/                         # Rust / Tauri 后端
│  ├─ src/
│  │  ├─ main.rs                      # Windows 进程入口：单实例、DPI、WebView2
│  │  ├─ lib.rs                       # Tauri Builder 与全部 IPC 命令注册
│  │  ├─ software.rs                  # 内置软件目录、软件源模型、版本查询
│  │  ├─ custom_software.rs           # 自定义下载源、图标、JSON 配置
│  │  ├─ installer.rs                 # 下载缓存、安装、卸载、安装状态检测
│  │  ├─ config.rs                    # 便携版 data/ 与安装路径配置
│  │  ├─ github.rs                    # GitHub Release 数据模型
│  │  ├─ ocr_install.rs               # WeGame 安装器相关流程
│  │  └─ visual_target.rs             # 屏幕目标、自动化步骤、规则与模板
│  ├─ capabilities/default.json       # Tauri 权限声明
│  ├─ tauri.conf.json                 # 窗口、构建、图标与应用标识配置
│  └─ icons/                          # Tauri 打包图标
│
├─ scripts/                           # 打包、图标、视觉检查辅助脚本
├─ public/                            # Vite 静态资源
├─ dist/                              # 前端构建产物，不手改
├─ release/                           # 发布产物，不手改
├─ node_modules/                      # 依赖，不手改
├─ package.json                       # 前端依赖与 npm 脚本
├─ Cargo.toml                         # Rust 依赖清单
└─ README.md                          # 本文件：项目主架构
```

根目录的 `errors*.txt` 与 `fix_*.cjs/js` 是历史排错材料，不是主业务代码。新功能不要依赖它们；确认无价值后再单独整理。

## 4. 三层运行结构

```text
用户操作
  -> React UI（src/App.tsx）
  -> invoke("命令名", 参数)
  -> Tauri IPC 路由（src-tauri/src/lib.rs）
  -> Rust 业务模块
  -> 网络 / 文件系统 / Windows 安装器 / 屏幕自动化
  -> 结果返回 React UI
```

前端只有一个主界面文件 `src/App.tsx`，目前承担软件库、安装包、自动化、设置等页面的状态与交互。修改 UI 时优先沿用它已有的状态、组件和 `invoke()` 调用模式；不要无必要地引入新的状态管理库。

`src-tauri/src/lib.rs` 是后端总装配点。新增一个 Rust 命令，必须同时完成三件事：

1. 在对应业务模块实现命令函数。
2. 在 `lib.rs` 导入并加入 `tauri::generate_handler![]`。
3. 在 `src/App.tsx` 使用同名 `invoke()` 调用，并处理加载、成功与失败状态。

## 5. 业务模块职责

| 模块 | 负责什么 |
| --- | --- |
| `software.rs` | 内置软件条目、GitHub/官网/WeGame/AMD 等来源的版本与下载信息 |
| `custom_software.rs` | 自定义下载源的增删改查、下载候选扫描、图标保存与读取 |
| `installer.rs` | 安装包下载到缓存、安装、卸载、检测是否已安装 |
| `config.rs` | 便携版运行数据路径、缓存位置、默认安装目录与用户设置 |
| `visual_target.rs` | 屏幕颜色/位置目标、自动化步骤、自动化模板和执行链路 |
| `ocr_install.rs` | WeGame 安装器的辅助自动化流程 |
| `github.rs` | GitHub Release API 的序列化数据结构 |

## 6. 运行时数据与安装路径

便携版以 EXE 所在目录为根目录。运行时数据自动放在：

```text
<便携版目录>\data\
├─ config.json                 # 用户设置，例如安装根目录
├─ custom_software.json        # 自定义下载源
├─ packages\                   # 已下载的安装包缓存
└─ ...                          # 图标、自动化规则等运行数据
```

默认的软件安装目录是：

```text
%LOCALAPPDATA%\software-manager\apps
```

重要：源码配置与便携版运行数据不是同一个东西。修改 `src-tauri/src/*.rs` 后需要重新构建；修改已发布便携版的 `data/` 则只会影响那一份运行实例。

## 7. 当前 UI 范围

主界面有四个核心区域：

```text
软件库       查询版本、查看来源、下载或安装
安装包       管理官网安装包缓存与安装动作
自动化       定义并执行安装器的视觉自动化步骤
设置         安装目录、缓存目录及相关运行配置
```

新增功能前先判断它属于哪一个区域和哪一个 Rust 模块。不要把下载逻辑直接写进 React，也不要把纯 UI 状态塞进 Rust。

## 8. 图标与自定义下载源

自定义下载源的配置由 `custom_software.rs` 管理，并写入便携版 `data/custom_software.json`。图标地址可能是本地路径、HTTP(S) 地址或 `data:image/...` 数据 URI。

处理图标时必须同时验证：

1. 配置写入是否成功。
2. Tauri 对本地文件是否有可访问的 Asset Scope。
3. React `<img>` 最终拿到的地址是否可被浏览器直接加载。
4. 发布后的正式 EXE 中，软件库是否真实显示图标。

不要只看图标文件是否存在就判定成功。

## 9. 修改规则（给 AI）

1. 先用 `rg` 搜索已有命令、类型、配置和 UI 状态，再动手。
2. 前端与 Rust 命令要成对修改；不要只改其中一侧。
3. 文件路径、下载地址、安装器执行都要考虑 Windows 路径和管理员权限。
4. 修改自动化或安装逻辑前，先保留用户已有的 `data/`；不要清空下载缓存或配置。
5. 不手改 `dist/`、`release/`、`node_modules/`。
6. 不用二进制补丁替代可在源码中完成的修复。
7. 完成前至少运行 `npm run build`；涉及桌面能力时再用 `npm run tauri dev` 或新构建的 EXE 做真实验证。

## 10. Windows 启动约束

`src-tauri/src/main.rs` 已处理以下行为：

- 单实例：重复打开不会再启动第二个程序。
- DPI 感知：避免高 DPI 下界面模糊。
- 便携版 WebView2：同目录存在 `WebView2/` 时优先使用内置运行时。
- Release 模式不显示额外控制台窗口。

这些逻辑属于 Windows 运行基础设施，除非明确修复相关问题，否则不要删除或绕过。
