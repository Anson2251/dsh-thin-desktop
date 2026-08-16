# DSH Thin Desktop

> **DeepSeek Harness（`dsh web`）的原生桌面窗口。**
>
> 🇨🇳 中文 · [🇬🇧 English](README.md)

如果你平时在浏览器标签页里用 DeepSeek Harness 的网页界面，这个应用能给你更地道的桌面体验：独立窗口、菜单栏、系统托盘图标，以及 dsh 需要你注意时的原生通知。

---

## 这是什么？

**DSH Thin Desktop** 是 DeepSeek Harness 网页界面的轻量「薄壳」。它只做三件事：

- **帮你启动 `dsh web`**（或直接连你已经跑起来的实例）。
- 用一个**原生窗口**展示它，带菜单栏和**托盘图标**。
- 当 agent 需要你决策时，给你发**桌面通知**。

---

## 环境要求

- 本机已安装 **DeepSeek Harness（`dsh`）**——`dsh` 命令需要在 `PATH` 中可用。
- 64 位桌面系统（macOS / Windows / Linux）。
- **仅当你从源代码构建时需要**：Rust 工具链（见 [从源码构建](#从源码构建)）。

## 安装 dsh

如果还没有 `dsh`，从 npm 全局安装为命令行工具。**包管理器不重要，pnpm 和 npm 都行**：

```bash
pnpm install -g @deepseek-ai/dsh      # 或：
npm install -g @deepseek-ai/dsh
```

`dsh` 要求 **Node.js `^22.19.0 || >=24.0.0`**。你只需要 node/npm 来安装 CLI——运行本桌面应用**不需要** node/npm。

装完后确认命令在 `PATH` 上、且 web 档案能启动（这正是本应用所依赖的）：

```bash
dsh --version
dsh web   # 启动服务并打印类似 "dsh web: http://127.0.0.1:3080"
```

> 只需安装这一次。应用会复用已经运行的 `dsh web`，没有则会自己启动一个。

---

## 安装 / 运行

### 从 Releases 下载

各平台的预编译安装包已发布在 GitHub
[Releases](https://github.com/Anson2251/dsh-thin-desktop/releases)。请按平台选择：

- **macOS（Apple Silicon）**——`dsh-thin-desktop_*_aarch64.dmg`（把 App 拖入 `Applications`）
- **Windows x86-64**——`dsh-thin-desktop_*_x64-setup.exe`（安装程序）或 `.msi`
- **Linux x86-64**——`.AppImage`（chmod +x 后运行，或安装）或 `.deb`

> macOS：如果 Gatekeeper 拦截未签名的包，首次请右键「打开」；并在首次询问时允许通知。

### 已有编译好的应用？

直接打开即可。首次启动会自动启动 `dsh web`（或复用已经在运行的），并载入窗口。

### 从源码构建

```bash
# 开发模式运行
cd src-tauri && cargo run

# 编译可分发的正式包（macOS 上是 .app / .dmg）
cargo build --release
# 或如果你装了 Tauri CLI：
cargo tauri build
```

> 无需 Node.js 或前端构建步骤。

---

## 使用方法

### 窗口

- 窗口显示的是正在运行的 DeepSeek Harness 界面。
- **窗口标题**会显示当前会话的名称（会话还没有标题时，退化为短 session id）。
- 关闭窗口是**收进托盘**而不是退出——应用继续在后台运行。

### 菜单栏

| 菜单 | 项 | 快捷键 | 作用 |
|---|---|---|---|
| File | Start / Restart Backend | `Cmd/Ctrl+Shift+R` | (重)启动 `dsh web` 后端 |
| File | Quit | `Cmd/Ctrl+Q` | 退出应用 |
| Edit | Undo / Redo / Cut / Copy / Paste / Select All | `Cmd/Ctrl+Z/Y/X/C/V/A` | 网页界面里的标准文本编辑快捷键 |
| View | Reload | `Cmd/Ctrl+R` | 重载当前页面 |
| View | Toggle DevTools | `Cmd/Ctrl+Alt+I` | 打开/关闭开发者工具 |

### 系统托盘

托盘 / 菜单栏图标让应用一键可达：

- **左键单击**托盘图标：显示 / 隐藏窗口。
- **右键单击**：一个小菜单——显示/隐藏、Reload、Restart Backend、Quit。
- 真正退出用 **Quit**（或 `Cmd/Ctrl+Q`）；关闭窗口只会隐藏。

### 通知

当 agent 需要人工决策时，应用会弹出一条**原生桌面通知**，并**把窗口带到前台**：

- **「DSH 需要你批准」** —— 某次工具调用或沙箱权限升级需要你批准。
- **「DSH 提问」** —— agent 正在问你一个交互式问题。

> **没收到通知？** macOS 的专注/勿扰模式会压制横幅（只留角标）。请先关闭专注模式，并在 macOS 首次询问时**允许**该应用发送通知。另外注意：**开发模式**（`cargo run`）下通知是以 *Terminal* 这个小程序的名义显示的；**签名后的正式构建**才会以你自己应用的名义显示。

### 语言

应用自身界面文案（菜单、托盘、通知、启动页）支持 **中文** 与 **English** 两种语言。语言通过环境变量读取——经典的 `LANG`/`LC_ALL`，可用 `DSH_LANG` 显式覆盖：

```bash
# 强制中文（或直接把 LANG 设为 zh_CN.UTF-8）
DSH_LANG=zh_CN.UTF-8 ./dsh-thin-desktop

# 强制英文
DSH_LANG=en_US.UTF-8 ./dsh-thin-desktop
```

内部实现是一个轻量的 `t!` 宏（见 `src/i18n.rs`），思路类似 `vue-i18n`：编译期 key、运行时按语言取词、支持 `{}` 占位符。

### `dsh` 的位置（查找机制）

应用需要 `dsh` 命令来启动后端，它按 **以下顺序** 查找、在第一个成功的步骤停下：

1. **`DSH_BIN_DIR`** —— 若你设置了此环境变量，优先使用这个显式覆盖。
2. **常见 npm/pnpm 全局 bin 目录** —— `$HOME/.npm-global/bin`、`$HOME/.npm/bin`、`$HOME/.local/bin`、`$HOME/bin`、`$PNPM_HOME`、`~/Library/pnpm`（macOS）、`/usr/local/bin`、`/opt/homebrew/bin`；Windows 上还有 `%APPDATA%\npm`、`%LOCALAPPDATA%\pnpm`、`%ProgramFiles%\nodejs`。
3. **你的 shell 的 rc 文件** —— 应用会调用你的 shell `source` 其配置文件并回显得到的 `$PATH`，从而找回 GUI 启动的应用通常看不到的目录。按以下顺序、且仅当文件存在时尝试：
   - `~/.zshrc`（zsh）
   - `~/.config/fish/config.fish`（fish）
   - `~/.bashrc`（bash）
4. **都不行则报错** —— 启动失败，启动页会显示「重启后端」错误；请检查 `dsh` 是否已安装。

在 Windows 上，`dsh` 通常以 npm shim（`dsh.cmd`、`dsh.ps1`）形式安装，并没有真正的 `dsh.exe`。应用会自行解析这些 shim：在上述 `PATH` 中依次查找 `dsh.exe`/`dsh.cmd`/`dsh.bat`/`dsh.ps1`，并用 cmd.exe 启动 `.cmd` shim（`.ps1` shim 则通过 PowerShell 启动）。

> `DSH_BIN_DIR` 是应对任何非常规安装位置的兜底开关：
>
> ```bash
> DSH_BIN_DIR=/path/to/dsh/bin ./dsh-thin-desktop
> ```
>
> 提示：用 `which dsh` 或 `npm prefix -g` 可找到你的 CLI 工具装在哪。

---

## 常见问题

| 现象 | 可能原因与解决 |
|---|---|
| 打开后停在「正在启动 / 重连」页并让你重试 | `dsh web` 没启动成功。确认 `dsh` 能被找到，然后点 **Start / Restart Backend**。 |
| 应用找不到 `dsh`（PATH 问题） | 从 dock/开始菜单启动时，GUI 应用只看到系统 PATH，看不到 shell 里的配置。启动时设置 `DSH_BIN_DIR=/path/to/dsh/bin`（或先确认 `which dsh`）。 |
| 没有通知横幅 | macOS 专注/勿扰模式开着，或应用还没被允许通知——在系统弹窗里点一次「允许」。 |
| 端口 3080 被别的程序占用 | 客户端会跟随 `dsh web` 实际报告的地址去加载，不受影响。 |

---

## 给开发者（简版）

- **架构：** 原生 Tauri webview 直接加载 dsh web 的 URL（不用 iframe）。Rust 侧负责启动 `dsh web`、从输出中探测 host/port、并监控连接状态。
- **通知：** 应用通过 WebSocket 订阅 dsh 的实时事件流（`/api/events.mux`），对批准/提问事件发原生通知。
- **布局：** `src/lib.rs`（生命周期）、`src/sse.rs`（事件流）、`src/notify.rs`（通知）、`src/titles.rs`（会话标题）、`src/tray.rs`（托盘）、`src/i18n.rs`（i18n 宏）。
- 实时冒烟测试：`tests/ws_smoke.rs`、`tests/title_smoke.rs`。

---

## 许可证

MIT
