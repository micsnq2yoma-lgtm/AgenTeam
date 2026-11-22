<div align="center">

<img src="src/renderer/assets/brand/agenteam-mark.svg" width="72" alt="AgenTeam">

# AgenTeam

**Focus Your Attention on Valuable Things.**

Every project is a team. You set the direction and make the calls; AI employees get the work done in your local workspace.

[中文](README.md) · **English**

[Quick start](#quick-start) · [How it works](#how-it-works) · [Features](#features) · [Development guide](DEVELOPMENT.md)

[![stars](https://img.shields.io/github/stars/micsnq2yoma-lgtm/AgenTeam?style=flat&logo=github)](https://github.com/micsnq2yoma-lgtm/AgenTeam/stargazers) [![Tests](https://img.shields.io/github/actions/workflow/status/micsnq2yoma-lgtm/AgenTeam/test.yml?branch=main&label=tests)](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/workflows/test.yml) [![license](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE) ![platform](https://img.shields.io/badge/platform-Windows%2010%2B%20%7C%20macOS%2013%2B-informational) ![node](https://img.shields.io/badge/node-%E2%89%A524.14-339933) ![built on](https://img.shields.io/badge/built%20on-MyAgents-orange)

<img src="specs/assets/readme/agenteam-team-skins.png" width="860" alt="AgenTeam Agent Team view in the Modern office, Imperial court and Starship skins">

</div>

## How it works

| ① New project | ② New task | ③ Employees do the work | ④ You decide |
| --- | --- | --- | --- |
| Pick a folder (new, existing, or the default location) and optionally turn on assistant mode | Each task starts with the workspace as context and appears as an employee in Agent Team | Claude Agent SDK, DeepSeek Harness or Codex read and write files and run commands on your machine | Steer at any time; hand recurring work to automations |

In the sidebar's **Agent Team** view, every project is a department: the assistant sits at the head desk and employees at their desks, acting out what their task is really doing (working, on schedule, on chat duty, standing by, resting). Click an employee to open the task; click "New employee" to create a new task in that project.

> The structure is **project → assistant → tasks**. A task and an employee are the same thing: Agent Team calls it an employee, everywhere else calls it a task.

## Who it is for

- ✅ You want to organise many AI tasks by project instead of scrolling one long thread
- ✅ You already have Claude / Codex subscriptions or model APIs and want them working on your own computer
- ✅ You want daily checks, tidying and reports to run on a schedule and only wake you when needed
- ❌ Not a multi-user SaaS: one install per computer, with workspaces and tasks stored locally

## Features

| | |
| --- | --- |
| 🗂️ **Projects** A project is a folder: create one, adopt an existing folder, or use the default location; optional assistant mode | 👥 **Agent Team** A pixel office showing every project's employees, with Modern office / Imperial court / Starship skins |
| 🤖 **Runtimes** Claude Agent SDK, DeepSeek Harness, managed Codex, or the Claude Code / Codex CLI on your system | 🔑 **Subscriptions and APIs** Sign in with a subscription or use any model API, chosen per Agent |
| ⏰ **Automations** One-off, interval and Cron schedules; run a script first and wake the Agent only when a condition is met | 🧰 **Skills and tools** Skills capture your methods; MCP connects tools and data (STDIO / HTTP / SSE) |
| 💬 **Chat bots** Built-in Feishu, DingTalk and Telegram, more platforms via plugins | 🛰️ **Multi-device network** Self-hosted AgentNet hands work across devices, end-to-end encrypted |
| 🔍 **Full-text search** The sidebar search bar finds any task by title or content | 🧑‍💻 **CLI** Agents use the `myagents` CLI to create tasks and call other Agents; authorised external Agents can too |

## Quick start

You need Node.js 24.14+ and npm 11.15+ (see [package.json](package.json)). Native builds also need the Visual Studio C++ desktop workload with the Windows SDK, Rustup and CMake; see the [Windows build guide](specs/guides/windows_build_guide.md).

```powershell
git clone https://github.com/micsnq2yoma-lgtm/AgenTeam.git
cd AgenTeam
.\setup_windows.ps1     # prepare Node, dependencies and build resources (slow the first time, cached afterwards)
npm run tauri:dev       # start the desktop dev build
```

| Goal | Command |
| --- | --- |
| Tweak the UI only | `preview_windows.cmd`: browser preview with hot reload; desktop features such as search and files are unavailable |
| Debug desktop features | `.\setup_windows.ps1`, then `npm run tauri:dev` |
| Build an installer | `.\build_windows.ps1 -Candidate -SkipPortable`; output in `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/` |

On macOS run `./setup.sh` then `./start_dev.sh`; on Ubuntu 24.04 run `./setup.sh` and `./build_dev_linux.sh`. See the [development guide](DEVELOPMENT.md) for details.

Once it is running:

1. **Connect a model**: Settings → Model Providers, sign in to a subscription or enter an API, then pick the Runtime and model. See the [models and Runtime guide](bundled-skills/agenteam-docs/references/models-providers-runtimes.md).
2. **Create a project**: click **+** next to "Projects" in the sidebar, choose the folder location and whether to turn on assistant mode.
3. **Create a task**: create a task in the project, share the material and the goal; for ongoing work, ask the Agent to set up an automation.

App data lives in `~/.myagents` (the upstream MyAgents directory name, kept on purpose; do not rename it).

## What you can change in Settings

- **Team look**: choose the Agent Team skin; on supported systems you can also enable the desktop pet to summon AgenTeam at any time.
- **Chat bots**: connect Feishu / DingTalk / Telegram to a project.
- **External calls**: authorise other Agents to use AgenTeam through the CLI; see the [external Agent guide](bundled-guides/external-myagents-cli/SKILL.md).

## Architecture

| Path | Contents |
| --- | --- |
| `src/renderer/` | React 19 + Vite desktop UI; `features/team-office/` holds Agent Team and its skins |
| `src/server/` | Node.js Sidecar: SessionEngine and Runtime adapters, one process per session |
| `src-tauri/` | Tauri v2 Rust shell: processes, persistence, HTTP/SSE proxy, automation store |
| `src/cli/` | The `myagents` CLI |
| `src/shared/` | Types and pure logic shared by the UI and the Sidecar |
| `bundled-workspaces/` `bundled-skills/` | Built-in 小 T assistant template and skill docs |
| `specs/` | Architecture, design system, module tech docs and build guides |

## Documentation

- [Development guide](DEVELOPMENT.md): environment, builds, checks and tests
- [Architecture](specs/ARCHITECTURE.md) · [Design system](specs/DESIGN.md)
- [Tasks and automation](bundled-skills/agenteam-docs/references/automation.md) · [Skills, tools and plugins](bundled-skills/agenteam-docs/references/tools-skills-plugins.md) · [Agents and chat bots](bundled-skills/agenteam-docs/references/agents-channels.md) · [Multi-device network](bundled-skills/agenteam-docs/references/agent-network.md)
- [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md)

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=micsnq2yoma-lgtm/AgenTeam&type=Date)](https://star-history.com/#micsnq2yoma-lgtm/AgenTeam&Date)

## Acknowledgements

AgenTeam stands on the shoulders of these projects; full third-party licences are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

- [MyAgents](https://github.com/hAcKlyc/MyAgents) (AGPL-3.0-only): upstream of the desktop app, Runtimes and task system
- [AgenTeam pixel-office](https://github.com/micsnq2yoma-lgtm/Agent) (MIT): pixel employees and office
- [claude-code-pixel-pet](https://github.com/zhizunbao-studio/claude-code-pixel-pet): the DeepSeek whale silhouette
- Cormorant Garamond (SIL OFL 1.1): wordmark font

## License

This repository keeps the upstream [GNU AGPL v3.0](LICENSE) (`AGPL-3.0-only`). If you distribute a modified version or offer one to users over a network, you must provide the corresponding source under the AGPL. See [LICENSING.md](LICENSING.md) and [COMMERCIAL-LICENSING.md](COMMERCIAL-LICENSING.md) for details, and the [acknowledgements](#acknowledgements) above for third-party components.
