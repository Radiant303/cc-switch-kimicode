<div align="center">

# CC Switch

### 为 Kimi Code 打造的桌面管理工具

在一个清晰、集中的桌面工作区中管理 Kimi Code 的供应商、MCP、提示词、Skills、会话与用量。

[![版本](https://img.shields.io/github/v/release/farion1231/cc-switch?color=2563eb&label=版本)](https://github.com/farion1231/cc-switch/releases)
[![平台](https://img.shields.io/badge/平台-Windows%20%7C%20macOS%20%7C%20Linux-64748b.svg)](https://github.com/farion1231/cc-switch/releases)
[![基于 Tauri](https://img.shields.io/badge/基于-Tauri%202-f97316.svg)](https://tauri.app/)
[![许可证](https://img.shields.io/github/license/farion1231/cc-switch)](LICENSE)

**[下载最新版本](https://github.com/farion1231/cc-switch/releases/latest)** · **[English](README_EN.md)** · **[官方网站](https://ccswitch.io)**

</div>

## Kimi Code 的统一控制中心

Kimi Code 很强大，但供应商配置、工具、指令和会话记录分散在不同的文件与目录中。CC Switch 将日常工作集中到一个原生桌面界面里：

- 不手动编辑 TOML，即可切换供应商配置；
- 在一个面板中管理 MCP 服务器；
- 集中维护全局提示词和可复用的 Skills；
- 按工作区浏览、搜索和清理 Kimi Code 会话；
- 查看账号状态、订阅用量与额度。

你不需要改变原有的 Kimi Code 使用方式。CC Switch 直接管理 Kimi Code 已经使用的文件，让 Kimi Code 继续负责实际运行。

## CC Switch 能管理什么

| 模块 | 可以做什么 | Kimi Code 数据 |
| --- | --- | --- |
| 供应商 | 创建、编辑、排序、切换和备份供应商配置 | <code>config.toml</code> |
| MCP | 添加、导入、编辑、启用、停用和删除 MCP 服务器 | <code>mcp.json</code> |
| 提示词 | 使用 Markdown 编辑器维护全局指令文件 | <code>AGENTS.md</code> |
| Skills | 发现、安装、启用、停用、备份和恢复 Skills | <code>skills/</code> |
| 会话 | 浏览、搜索、预览和删除会话记录 | <code>sessions/</code> |
| 用量 | 查看订阅状态、额度和近期使用情况 | Kimi Code 账号 |

CC Switch 支持 <code>KIMI_CODE_HOME</code> 环境变量。未设置时，会使用 Kimi Code 的默认目录。

## 让 Kimi Code 更顺手

### 切换供应商，不再手动改配置

在一个地方维护多个 Kimi Code 供应商配置，一键切换当前配置。写入前会进行校验，并使用原子方式更新文件；需要回退时，还可以使用备份。

### 管理 MCP，不再手动编辑 JSON

通过结构化表单管理本地和远程 MCP 服务器。你可以导入已有配置，编辑传输方式、命令和参数，并将结果同步到 Kimi Code 的 <code>mcp.json</code>。

### 提示词和 Skills，集中维护

使用编辑器维护 <code>AGENTS.md</code>，把 Skills 当作独立资源管理，不再手动复制目录。安装、启用、备份和恢复状态清晰可见，也随时可以撤销。

### 会话记录，按工作区快速查找

按工作区浏览 Kimi Code 的历史会话，查看对话内容，并清理不再需要的记录，无需手动进入多层目录。

### 用量状态，一眼掌握

在管理当前供应商的同时，查看 Kimi Code 账号状态和订阅额度。

## 数据目录

CC Switch 管理的是 Kimi Code 原生使用的文件：

~~~text
KIMI_CODE_HOME/
├── config.toml       # 供应商和模型配置
├── mcp.json          # MCP 服务器
├── AGENTS.md         # 全局提示词和指令
├── skills/           # 已安装的 Skills
└── sessions/         # 会话记录
~~~

CC Switch 不会替代 Kimi Code，也不会引入另一套运行时；它只是帮助你管理已有的配置和历史数据。

## 快速开始

1. 从 [Releases](https://github.com/farion1231/cc-switch/releases/latest) 下载适用于 Windows、macOS 或 Linux 的版本。
2. 打开 CC Switch，选择 **Kimi Code**。
3. 新建供应商，或导入现有的 <code>config.toml</code>。
4. 切换当前供应商，然后照常启动 Kimi Code。
5. 需要管理对应内容时，打开 **MCP**、**提示词**、**Skills** 或 **会话** 面板。

如果使用 Kimi Code OAuth，请在 Kimi Code 中完成登录。CC Switch 不接管登录凭据，只读取显示用量和额度所需的账号状态。

## 默认安全

- 使用 Kimi Code 原生文件目录。
- 写入前校验结构化配置。
- 使用原子写入，降低配置损坏风险。
- 提供供应商和配置备份。
- 将管理数据与 Kimi Code 登录凭据分开处理。

## 开发

### 环境要求

- Node.js 18+
- pnpm 8+
- Rust 1.85+
- Tauri CLI 2.8+

### 常用命令

~~~bash
pnpm install
pnpm dev
pnpm typecheck
pnpm test:unit
pnpm tauri build
~~~

Rust 检查：

~~~bash
cargo fmt --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
~~~

## 贡献

欢迎提交 Issue、功能建议和 Pull Request。提交修改前，请先运行相关的类型检查和测试。

## 许可证

MIT © Jason Young
