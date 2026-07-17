<div align="center">

# CC Switch

### The desktop manager built for Kimi Code

Keep your Kimi Code providers, MCP servers, prompts, Skills, sessions, and usage in one focused workspace.
![CC Switch Kimi-Code](/images/index.png)

![CC Switch Kimi-Code](/images/usage.png)

![CC Switch Kimi-Code](/images/more.png)
</div>

## One control center for Kimi Code

Kimi Code is powerful, but its configuration, tools, instructions, and conversation history live in different files and directories. CC Switch brings the daily workflow together in a native desktop interface:

- switch between provider profiles without editing TOML by hand;
- manage MCP servers from one panel;
- maintain global prompts and reusable Skills;
- browse and clean up Kimi Code sessions;
- keep an eye on subscription usage and quota.

Your existing Kimi Code workflow stays intact. CC Switch edits the files Kimi Code already understands and lets Kimi Code remain the runtime.

## What CC Switch manages

| Area | What you can do | Kimi Code data |
| --- | --- | --- |
| Providers | Create, edit, reorder, switch, and back up provider profiles | <code>config.toml</code> |
| MCP | Add, import, edit, enable, disable, and remove MCP servers | <code>mcp.json</code> |
| Prompts | Edit the shared instruction file with a Markdown editor | <code>AGENTS.md</code> |
| Skills | Discover, install, enable, disable, back up, and restore Skills | <code>skills/</code> |
| Sessions | Browse, search, preview, and delete conversation sessions | <code>sessions/</code> |
| Usage | View subscription status, quota, and recent usage | Kimi Code account |

CC Switch respects <code>KIMI_CODE_HOME</code>. When it is not set, it uses Kimi Code's default home directory.

## A cleaner Kimi Code workflow

### Provider switching without configuration drift

Keep multiple Kimi Code provider profiles in one place. Switch the active profile with one click and let CC Switch write a validated configuration atomically, with backups available when you need to roll back.

### MCP without manual JSON editing

Manage local and remote MCP servers through a structured form. Import existing servers, edit their transport and arguments, and synchronize the result to Kimi Code's <code>mcp.json</code>.

### Prompts and Skills that are easy to maintain

Use a proper editor for <code>AGENTS.md</code>, and manage Skills as first-class resources instead of copying folders around manually. Installation, enablement, backup, and restoration stay visible and reversible.

### Sessions you can actually find

Browse Kimi Code's session history by workspace, inspect conversation content, and remove stale sessions without navigating through nested directories.

### Usage at a glance

See the current Kimi Code account state and quota from the same place where you manage the active provider.

## Data paths

CC Switch works with Kimi Code's native files:

~~~text
KIMI_CODE_HOME/
├── config.toml       # provider and model configuration
├── mcp.json          # MCP servers
├── AGENTS.md         # shared prompts and instructions
├── skills/           # installed Skills
└── sessions/         # conversation history
~~~

It does not replace Kimi Code or introduce a second runtime. It is a management layer for the configuration and history you already own.

## Quick start

1. Download the latest version for Windows, macOS, or Linux from [Releases](https://github.com/farion1231/cc-switch/releases/latest).
2. Open CC Switch and select **Kimi Code**.
3. Add a provider or import your existing <code>config.toml</code>.
4. Switch the active provider, then launch Kimi Code as usual.
5. Open **MCP**, **Prompts**, **Skills**, or **Sessions** whenever you need to manage that part of your workflow.

If you use Kimi Code OAuth, authentication remains owned by Kimi Code. CC Switch only reads the account state needed for usage and quota display.

## Safe by default

- Uses Kimi Code's existing file layout.
- Validates structured configuration before writing.
- Writes files atomically to reduce corruption risk.
- Keeps provider and configuration backups available.
- Separates management data from Kimi Code credentials.

## Development

### Requirements

- Node.js 18+
- pnpm 8+
- Rust 1.85+
- Tauri CLI 2.8+

### Commands

~~~bash
pnpm install
pnpm dev
pnpm typecheck
pnpm test:unit
pnpm tauri build
~~~

For Rust checks:

~~~bash
cargo fmt --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
~~~

## Contributing

Issues, feature requests, and pull requests are welcome. Please run the relevant type checks and tests before submitting a change.

## License

MIT © Jason Young
