<div align="center">

<img src="src/renderer/assets/brand/agenteam-mark.svg" width="72" alt="AgenTeam">

# AgenTeam

**把注意力放在真正有价值的事情上 · Focus Your Attention on Valuable Things.**

一个项目就是一支团队。你定方向、做决定，AI 员工在本机工作区里把事情做完。

**中文** · [English](README.en.md)

[快速开始](#快速开始) · [它怎么工作](#它怎么工作) · [功能](#功能) · [记忆](#记忆助理怎么记住你) · [开发指引](DEVELOPMENT.md)

[![stars](https://img.shields.io/github/stars/micsnq2yoma-lgtm/AgenTeam?style=flat&logo=github)](https://github.com/micsnq2yoma-lgtm/AgenTeam/stargazers) [![Tests](https://img.shields.io/github/actions/workflow/status/micsnq2yoma-lgtm/AgenTeam/test.yml?branch=main&label=tests)](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/workflows/test.yml) [![license](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE) ![platform](https://img.shields.io/badge/platform-Windows%2010%2B%20%7C%20macOS%2013%2B-informational) ![node](https://img.shields.io/badge/node-%E2%89%A524.14-339933) ![built on](https://img.shields.io/badge/built%20on-MyAgents-orange)

<img src="specs/assets/readme/agenteam-team-skins.png" width="860" alt="AgenTeam 智能体团队：现代办公室、朝堂、星舰三套皮肤">

</div>

## 它怎么工作

| ① 新建项目 | ② 创建新任务 | ③ 员工去做 | ④ 你来拍板 |
| --- | --- | --- | --- |
| 选一个文件夹（新建、已有或默认位置），按需打开助理模式 | 每个任务带着工作区上下文开工，在智能体团队里就是一名员工 | Claude Agent SDK、DeepSeek Harness 或 Codex 在本机读写文件、跑命令 | 你随时调整方向；持续性的事交给自动化任务 |

在侧栏 **智能体团队** 里，每个项目是一个项目部：助理坐在主位，员工坐在工位上，动作就是任务此刻的真实状态（工作中、定时值守、值守聊天、待命、休息）。点员工打开任务，点「新员工」在该项目里创建一个新任务。

> 结构是 **项目 → 助理 → 任务**。任务和员工是同一个东西：在智能体团队里叫员工，其余地方都叫任务。

## 适合谁

- ✅ 想按项目管理一批 AI 任务，而不是在一条长对话里来回翻
- ✅ 已经有 Claude / Codex 订阅或模型 API，想让它们在自己的电脑上干活
- ✅ 想把每天的检查、整理、汇报交给定时任务，只在需要时被叫醒
- ❌ 不是多人协作的 SaaS：每台电脑一套，工作区和会话保存在本机

## 功能

| | |
| --- | --- |
| 🗂️ **项目管理** 项目 = 文件夹；新建、接管已有文件夹，或用默认位置；可选助理模式 | 👥 **智能体团队** 像素办公室看全部项目的员工状态，现代办公室 / 朝堂 / 星舰皮肤可换 |
| 🤖 **多种 Runtime** Claude Agent SDK、DeepSeek Harness、托管 Codex，也可接系统里的 Claude Code / Codex CLI | 🔑 **订阅与 API** 订阅登录或任意模型 API，按 Agent 单独指定模型 |
| ⏰ **自动化任务** 定时、间隔、Cron；先跑脚本，满足条件再唤醒 Agent | 🧰 **技能与工具** Skill 沉淀方法，MCP 接工具与数据源（STDIO / HTTP / SSE） |
| 💬 **聊天机器人** 内置飞书、钉钉、Telegram，插件扩展更多平台 | 🛰️ **设备组网** 自建 AgentNet，跨设备交接任务，端到端加密 |
| 🧠 **助理记忆** 工作区文件做记忆：用户画像、长期会话感知更新、跨助理共享、可选的主动提醒 | 📚 **知识库** 本地转换与全文索引，带 `kb://` 引用，可由整理 Agent 维护 |
| 🔍 **全文搜索** 侧栏搜索栏检索全部任务的标题和内容 | 🧑‍💻 **CLI** Agent 通过 `myagents` CLI 建任务、调其他 Agent；外部 Agent 授权后也能用 |

## 记忆：助理怎么记住你

打开助理模式后，助理的"记忆"就是工作区里的几份 Markdown 文件，没有隐藏的数据库：你能读、能改，换一个 Runtime 也不丢。

```text
对话 / IM / 定时任务
   │  日常学习：会话空闲 30 分钟后，后台 memory-update 把新认识追加到"近期观察"
   ▼
.claude/rules/            ← 每个会话自动加载
  01-IDENTITY  02-SOUL    身份与性格
  03-USER                 你是谁：八个固定章节（基本信息 / 背景 / 沟通风格 / 工作方式 /
                          偏好 / 写作风格 / 近期观察 / 来源概况）
  04-MEMORY               长期经验与规则
memory/topics/ · 日志      ← 按需读取，不占每轮上下文
   ▲
   │  Gardener（约每 3 天）：合并"近期观察"、修剪自动加载层，降级而不是删除
   │  Molt（约每 2 周）：反思并修订判断和原则
```

| 能力 | 做什么 |
| --- | --- |
| **用资料生成画像** | 给助理简历、个人网站或几份文档，后台 Task 读完写进 `03-USER`：只保留理解，不保留原文 |
| **章节归属 + 受检写入** | 每个章节只有固定的流程可以写；写入经 `section_edit.py`：只改点名的章节、文件被别处改过就拒绝、限定可写章节和大小、原子落盘 |
| **长期会话也能"知道"** | 记忆被更新后，Desktop / IM / Inbox 的长期会话在下一轮收到一条很小的隐藏提醒（只说哪些文件、哪些章节变了，不含正文），需要时再重读 |
| **跨助理共享画像** | 最早创建的助理是主助理；它的背景、沟通风格、工作方式、偏好、写作风格会单向同步给其他助理（保留各自的基本信息和近期观察，改动前留备份；`config.json` 的 `shareUserProfile` 可关闭） |
| **主动提醒（默认关闭）** | 在 `memory/PROACTIVE_RULES.md` 写 `enabled: true`，或对助理说「开启主动提醒」。心跳时它判断：立即告诉 / 先准备 / 之后跟进 / 沉默；安静时段、每日上限、同一件事一天只判一次由脚本保证；邮箱和日历只通过你配置的 MCP 工具或本机命令行读取，AgenTeam 不保存任何凭证 |

设计原则：记忆的唯一权威是工作区文件；后台整理复用任务系统；隐藏上下文复用 `system-reminder`。详见[助理画像与记忆](specs/tech_docs/assistant_profile.md)。

## 知识库

把文档、PDF 和图片整理成可检索的资料库：来源在本机转换为规范化 Markdown，建立全文索引，用 `kb://库/来源#片段` 精确引用。给助理绑定知识库后，它会用 `myagents kb` 检索并带引用回答；每个库可指定一个整理 Agent，定期补全元数据、维护 wiki 页面和分类建议，物理整理需要你审批。详见[知识库契约](specs/tech_docs/knowledge_base.md)。

## 快速开始

需要 Node.js 24.14+ 和 npm 11.15+（版本见 [package.json](package.json)）。原生构建还需要 Visual Studio C++ 桌面开发工具与 Windows SDK、Rustup 和 CMake，详见 [Windows 构建指南](specs/guides/windows_build_guide.md)。

```powershell
git clone https://github.com/micsnq2yoma-lgtm/AgenTeam.git
cd AgenTeam
.\setup_windows.ps1     # 准备 Node、依赖和构建资源（首次较慢，之后复用缓存）
npm run tauri:dev       # 启动桌面开发版
```

| 想做什么 | 命令 |
| --- | --- |
| 只调界面 | `preview_windows.cmd`：浏览器预览，热更新；搜索、文件等桌面能力不可用 |
| 调试桌面能力 | `.\setup_windows.ps1`，然后 `npm run tauri:dev` |
| 打安装包 | `.\build_windows.ps1 -Candidate -SkipPortable`，产物在 `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/` |

macOS 用 `./setup.sh`，再 `./start_dev.sh`；Ubuntu 24.04 用 `./setup.sh` 和 `./build_dev_linux.sh`。完整说明见[开发指引](DEVELOPMENT.md)。

装好以后：

1. **接入模型**：设置 → 模型供应商，登录订阅或填写 API，再选 Runtime 与模型。详见[模型与 Runtime 指南](bundled-skills/agenteam-docs/references/models-providers-runtimes.md)。
2. **新建项目**：侧栏「项目管理」旁点 **+**，选文件夹位置，决定是否打开助理模式。
3. **创建新任务**：在项目里新建任务，带上资料说明目标；需要持续处理的事，让 Agent 帮你建自动化任务。

应用数据位于 `~/.myagents`（沿用上游 MyAgents 的目录名，不是改名遗漏，不要手动重命名）。

## 设置里能调什么

- **团队形象**：选择智能体团队的皮肤；支持的系统上还可以开启桌面宠物，在任何时候唤起 AgenTeam。
- **聊天机器人**：把飞书 / 钉钉 / Telegram 接到某个项目。
- **助理记忆**：随时直接修改 `.claude/rules/03-USER.md` 更正画像；关闭跨助理共享用 `config.json` 的 `shareUserProfile: false`；主动提醒在 `memory/PROACTIVE_RULES.md` 开关。
- **外部调用**：授权其他 Agent 通过 CLI 使用 AgenTeam，见[外部 Agent 接入指南](bundled-guides/external-myagents-cli/SKILL.md)。

## 架构

| 目录 | 内容 |
| --- | --- |
| `src/renderer/` | React 19 + Vite 桌面界面；`features/team-office/` 是智能体团队与皮肤 |
| `src/server/` | Node.js Sidecar：SessionEngine 与各 Runtime 适配，每个会话独立进程 |
| `src-tauri/` | Tauri v2 Rust 壳：进程、持久化、HTTP/SSE 代理、自动化任务存储 |
| `src/cli/` | `myagents` CLI |
| `src/shared/` | 前后端共用的类型与纯逻辑 |
| `src-tauri/src/profile_sync.rs` `memory_*.rs` | 画像同步，以及记忆更新与 Gardener / Molt 的调度 |
| `bundled-workspaces/` `bundled-skills/` | 内置小 T 助理模板与系统技能（记忆、画像、知识库、主动提醒等） |
| `specs/` | 架构、设计规范、模块技术文档与构建指南 |

## 文档

- [开发指引](DEVELOPMENT.md)：环境、构建、检查与测试
- [架构文档](specs/ARCHITECTURE.md) · [界面设计规范](specs/DESIGN.md)
- [助理画像与记忆](specs/tech_docs/assistant_profile.md) · [知识库](specs/tech_docs/knowledge_base.md) · [system-reminder 协议](specs/tech_docs/system_reminder_protocol.md)
- [任务与自动化](bundled-skills/agenteam-docs/references/automation.md) · [技能、工具与插件](bundled-skills/agenteam-docs/references/tools-skills-plugins.md) · [Agent 与聊天机器人](bundled-skills/agenteam-docs/references/agents-channels.md) · [设备组网](bundled-skills/agenteam-docs/references/agent-network.md)
- [更新日志](CHANGELOG.md) · [贡献指南](CONTRIBUTING.md)

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=micsnq2yoma-lgtm/AgenTeam&type=Date)](https://star-history.com/#micsnq2yoma-lgtm/AgenTeam&Date)

## 致谢

AgenTeam 站在这些项目的肩膀上，完整的第三方许可见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

- [MyAgents](https://github.com/hAcKlyc/MyAgents)（AGPL-3.0-only）：桌面应用、Runtime、任务系统的上游
- [AgenTeam pixel-office](https://github.com/micsnq2yoma-lgtm/Agent)（MIT）：像素员工与办公室
- [claude-code-pixel-pet](https://github.com/zhizunbao-studio/claude-code-pixel-pet)：DeepSeek 鲸鱼的轮廓
- Cormorant Garamond（SIL OFL 1.1）：字标字体

## 许可

本仓库沿用上游的 [GNU AGPL v3.0](LICENSE)（`AGPL-3.0-only`）。分发修改版或通过网络向用户提供修改版时，需要按 AGPL 提供对应源码。授权细节见 [LICENSING.md](LICENSING.md) 与 [COMMERCIAL-LICENSING.md](COMMERCIAL-LICENSING.md)，第三方组件见上方[致谢](#致谢)。
