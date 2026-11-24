# 知识库交接（2026-11-24）

> 交接方：Claude。接收方：新的协作者。从本日起，知识库的**开发与验收**都由接收方负责。
> 合并到 `main`、推送 tag、发布，仍然需要用户明确授权；验收通过只表示可以申请合并。
> 本文是交接说明，不是规范。现行规范在 `specs/tech_docs/knowledge_base.md`，任务规格在 `tasks.md`，设计在 `prd.md`。它们与代码冲突时，先用代码、测试和 git 历史确认现状，再修正文档。

## 1. 一句话现状

P0 的后端和 Agent 侧（K0–K8）已经全部合并进 `main`：建库、导入、切块、BM25 检索、会话命令行、Agent/Session 绑定、两个 Skill、整理 Task 与 wiki 都能用。**界面只做了入口和库列表**（K9 第一步，在分支上，未合并）。K10 引用 chip、K11 端到端与评测还没开始。普通用户目前只能通过 Agent 和命令行使用知识库。

## 2. 任务状态

| 任务 | 内容 | 状态 | 记录 |
|---|---|---|---|
| K0–K4 | 契约与共享类型、KnowledgeStore、摄取管线、切块器、BM25 索引 | 已合并，独立验收通过 | `acceptance/K1.md`–`K4.md` |
| K5 | Management API、Admin API 与 `myagents kb` 命令行 | 已合并，通过 | `acceptance/K5.md` |
| K6 | Agent 默认绑定、Session 快照与提示词；**M1 能检索**实机通过 | 已合并，通过 | `acceptance/K6.md` |
| K7 | `agenteam-knowledge`、`agenteam-knowledge-curator` 两个 Skill 与库规则模板 | 已合并，自检通过 | `acceptance/K7.md` |
| K8 | 整理 Task（摘要、标签、分类）、wiki 写入与日志、lint 报告、分类提议与确认、`kb curation context` | 已合并，自检通过，AC-K8-7/8 实机通过 | `delivery/K8.md` |
| **K9** | 知识库 UI | **进行中**：入口与库列表已做，见 §3 | 无（待写） |
| K10 | 引用 chip | 未开始 | — |
| K11 | 端到端与检索评测 | 未开始 | — |
| K12–K20 | P1：linked folder、物理整理、语义检索、URL 导入、全局搜索 | 未开始；**开工前需用户另行确认方案** | — |

里程碑：

- **M1 能检索**：已通过（K6）。
- **M2 能整理**：K8 的 AC-K8-7 已在真实 App 跑完 M2 的场景（5 份新资料都有摘要、标签、分类；wiki 生成 `index.md` 和带 `kb://` 引用的主题页，引用能解析回原始来源）。但还**没有单独写成 M2 阶段报告**。建议接手后补一份 `acceptance/M2.md`，引用 K8 的证据即可，不必重跑。
- **M3 P0 完成**：K9–K11 之后。

## 3. 进行中：K9 第一步

- 分支 `feat/kb-k9-entry`，提交 `d64292d`，基于 `main` 的 `288225d`（0.7.1）。**未合并。** 推送时 CI 已触发，接手时先看这次运行的结果。
- **入口方案已由用户确认（2026-11-24），取代原规格的"侧栏入口 + 独立 Tab"：** 启动页顶部居中的分段开关"对话 ⇄ 知识库"（参照 MyAgents 启动页的"对话/记录"开关）。切到"知识库"时，在启动页内显示知识库页面；对话输入区只是隐藏、保持挂载，草稿不丢；对话那边在知识库模式下不响应窗口拖拽。`tasks.md` 的 K9 一节已按此改写：不需要 Tab variant、持久化 codec 和侧栏入口，AC-K9-2 的 codec 往返测试不再适用，"关闭页面不取消后台摄取"仍然适用（摄取归 Rust `KnowledgeIngestManager`，与页面生命周期无关）。
- 已完成：
  - `src/renderer/components/launcher/LauncherModeSwitch.tsx`：分段开关（`role="tablist"`）。
  - `src/renderer/features/knowledge/`：`types.ts`（Rust 记录的前端投影）、`knowledgeClient.ts`（`cmd_kb_list`、`cmd_kb_create`、`cmd_kb_delete`、`cmd_kb_list_sources` 的薄封装）、`KnowledgeHome.tsx`（库列表：名称、描述、资料数、整理模式、待确认分类数；新建；删除确认；空状态；读取失败重试）。
  - `src/renderer/pages/Launcher.tsx`：挂上开关和页面。
  - 文案在 `launcher` 命名空间的 `modeSwitch.*`、`knowledge.*`，中英文都有。
  - 测试：`LauncherModeSwitch.test.tsx`、`KnowledgeHome.dom.test.tsx`、`Launcher.runtimeSelection.dom.test.tsx` 里的切换用例。
- **尚未在真实窗口里看过**。接手后第一件事是在桌面 App 里打开，确认开关位置、深浅主题和窄窗口下的样子。

### 待用户确认的设计问题

**详情页结构。** 我向用户提议：在列表里点一个库，进入同一区域内的详情页，顶部有返回按钮，四个页内标签"**资料 / 检索 / Wiki / 设置**"。用户还没回复。开发详情页之前先问清楚。

### K9 剩余范围（按建议顺序）

1. **导入资料**：文件选择与拖拽导入（`cmd_kb_import_files`），进度（`cmd_kb_job_status`）、取消（`cmd_kb_cancel_job`）、失败重试（`cmd_kb_retry_source`）。导入要用 Tauri 文件对话框或拖拽路径，不在 Renderer 读文件内容。
2. **详情 · 资料**：按 `category` 的虚拟文件夹树和资料列表（状态、标签、重复标记、错误与重试）；把资料拖到另一个分类，等于用户写 `category`（`cmd_kb_set_metadata`，用户写会锁定字段）。
3. **资料抽屉**：元数据编辑（锁定字段显示锁标记，可解锁）、切块预览（`prefix` 和页码）。
4. **检索测试**：输入查询，显示结果、分数和 `mode`。
5. **Wiki**：浏览 wiki 页（复用 Markdown 预览）、lint 报告列表（`wiki/lint/`）、待确认的新分类（`cmd_kb_confirm_category`、`cmd_kb_dismiss_category`）。
6. **设置**：名称、用途、整理 Agent、整理模式和间隔、Agent 写权限、编辑 `ORGANIZE_RULES.md` / `WIKI_SCHEMA.md`（复用现有 Markdown 编辑器）、"立即整理"（`cmd_kb_curate_now`）、删除。
7. **Agent 设置里的"知识库"区块**（编辑 Agent 的 `knowledgeBaseIds`）和**对话工具菜单里的会话级开关**（Session 的库绑定）。
8. Store 变化的推送：经 Tauri event 通知页面失效重取，**不在前端另存一份权威状态**。现在列表页是在自己操作后重取，还没有订阅事件。

注意：检索测试、切块预览、lint 报告列表等目前可能没有现成的 `cmd_kb_*`。缺命令时按 owner 规则在 Rust 侧补 Tauri 命令，不要让 Renderer 绕道 Sidecar 或原生 fetch 去读。

## 4. 代码地图

| 层 | 位置 | 说明 |
|---|---|---|
| Rust Store 与摄取 | `src-tauri/src/knowledge/`（`store.rs`、`store/`、`ingest.rs`、`ingest/`、`chunker.rs`、`read.rs`、`curator_task.rs`、`types.rs`） | `KnowledgeStore` 是知识库清单、来源记录、元数据、wiki 的唯一写入权威；`KnowledgeIngestManager` 管导入队列；`KnowledgeIndex`（BM25）是派生索引，可重建，不能写回 Store |
| Tauri 命令 | `src-tauri/src/knowledge/commands.rs`，在 `src-tauri/src/lib.rs` 注册 | 现有 16 个：`cmd_kb_list`、`create`、`update`、`delete`、`list_sources`、`get_source`、`remove_source`、`set_metadata`、`resolve_ref`、`confirm_category`、`dismiss_category`、`curate_now`、`import_files`、`job_status`、`cancel_job`、`retry_source`（都带 `cmd_kb_` 前缀） |
| 命令行 | `src/cli/knowledge.ts`、`src/cli/myagents.ts`；命令与参数契约 `src/shared/knowledge/cli-contract.ts` | `myagents kb list / search / get / read / add / meta set / status / curation context / wiki write|log|lint-report / taxonomy propose`。范围只能是当前 Session 已绑定的库，**不接受调用方自报的 Session 或工作区参数** |
| Admin 转发 | `src/server/admin-api.ts`、`src/server/utils/knowledge-bindings.ts` | 经当前 Session 的 Admin API 转到 Rust Management API |
| 共享类型 | `src/shared/knowledge/`（ID、引用 `kb://`、错误码、CLI 契约、Skill 契约测试） | `KnowledgeBase` 等完整记录只在 Rust 定义，前端投影在 `src/renderer/features/knowledge/types.ts` |
| Skill | `bundled-skills/agenteam-knowledge/`、`bundled-skills/agenteam-knowledge-curator/`（含库规则模板 `references/ORGANIZE_RULES.md`、`WIKI_SCHEMA.md`、`lint-report.md`） | 改 Skill 内容必须同时 bump `SYSTEM_SKILLS_VERSION`（`src/shared/systemSkills.ts` 与 `src-tauri/src/commands.rs` 的常量和测试断言，三处同值；当前为 **79**）。`src/shared/knowledge/skills.test.ts` 校验 Skill 里出现的每条命令和参数都存在 |
| 界面 | `src/renderer/features/knowledge/`、`src/renderer/components/launcher/LauncherModeSwitch.tsx` | 只在 `feat/kb-k9-entry` 分支上 |
| 规范 | `specs/tech_docs/knowledge_base.md` | owner、不变量、限制、陷阱；每个任务都要同步（D7）。"待接入章节"里 K9–K11 一节还是空的 |

## 5. 规则（不变）

- **完成定义 D1–D9、八条红线、交付报告模板、验收五步**：都在 `tasks.md` §0。红线里与 K9 最相关的是：AI 可调用的路径不能批准、执行或撤销物理整理；不删除或覆盖用户原文件；Renderer 不用原生 fetch 访问 Sidecar 普通 API；不复制一份路径安全、原子写、文件锁、进程启动逻辑；不 suppress lint、不跳过或删除测试。
- **开发与验收分开**：开发完成并提交后，换一个新会话（或清空上下文）作为验收者，只依据 `tasks.md`、diff 和测试结果判断。验收结论写 `acceptance/K<n>.md`，交付报告写 `delivery/K<n>.md`，随分支提交。
- **界面要求**：`specs/DESIGN.md` 相关章节、`specs/tech_docs/react_stability_rules.md`；不用 `window.confirm`（用 `ConfirmDialog`）、不直接调用 `navigator.clipboard.writeText`、文案全部走 i18n 中英文。
- **分支与提交**：`feat/kb-k<n>-<短名>`，基于最新 `main`；显式列文件，禁止 `git add -A` / `git add .` / `git add -f`；Conventional Commits，subject 加上说明原因和取舍的非空 body。不提交 `specs/prd/`。
- **何时停下来问用户**：规格与代码冲突、需要新的状态 owner / 进程 / 通信模式、要动红线涉及的边界、某条验收项做不到，都先停下来说明，不要自行取舍。

## 6. 环境与 CI 的坑

- **CI 触发范围**：Test 工作流只在 `main`、`dev/**`、`feat/kb-*` 的推送上运行。分支名不符合就不会跑 CI。
- **CI 的四个作业**：TypeScript 检查与测试与构建、Linux Rust 与 Clippy、Windows Rust 原生（含知识库与工作区 IO）、macOS CLIProxy 生命周期。知识库分支额外比较 Clippy 告警（`baseline` 与 `candidate` 必须相等，`knowledge` 下为 0）。
- **已知的外部失败，重跑即可，在报告里注明**：
  - Windows 作业第 4 步"Verify bundled Node replacement with native NSIS"在约 10 秒内失败：`choco install nsis` 从 chocolatey 社区源拿到 503。只重跑失败的作业。
  - macOS 作业偶尔排不上机器，15 分钟后被取消，没有执行任何步骤。
  - Windows runner 偶发 `scripts/node-runtime.test.mjs` 临时目录 `EPERM`；慢速 runner 上知识库测试的 5 秒文件锁超时；Linux 偶发 `im::reply_router` 回执等待超时。
- **本地 Node 版本**：`package.json` 要求 Node ≥ 24.14。在 Node 22 上 `npm run <脚本>` 会因 `devEngines` 直接报错；可以直接调用 `node_modules/.bin/tsc`、`eslint`、`vitest`、`depcruise`，或换成 Node 24。
- **以 root 运行的 Linux 容器**里，`workspace_files::memory_rules` 等测试会报"Access denied: protected system directory"，属于环境问题，CI 上正常。
- 依赖扫描现有 17 条既有 warning、0 error，新代码不应增加。

## 7. 已知限制与遗留

- 库规则模板（`ORGANIZE_RULES.md`、`WIKI_SCHEMA.md`）只有中文版。规范里写明"多语言模板留待 K9 的知识库界面一并确定"，这件事需要在 K9 设置页里决定。
- 索引版本已升到 2（K8 加了 `source_kind = wiki`），旧索引会按派生索引重建机制自动重建。
- 助理模板的介绍页（`bundled-workspaces/atmer/INTRODUCTION.md`）已经把"知识库"列为能力。K9 的导入页上线前，用户能建库但不能在界面上导入资料，这句说法略超前；导入做完就名副其实。
- 验收报告都留在各自的 `acceptance/*` 分支上，没有合并进 `main`。

## 8. 接手后的建议顺序

1. 看 `feat/kb-k9-entry` 的 CI 结果；在桌面 App 里打开启动页，确认"对话 ⇄ 知识库"开关和库列表的实际效果（深浅主题、中英文、窄窗口），有问题先修。
2. 向用户确认详情页结构（§3"待用户确认的设计问题"）。
3. 按 §3 的顺序继续 K9，每一步一个提交、跑 CI；全部做完后写 `delivery/K9.md`，另开会话做验收写 `acceptance/K9.md`，再申请合并。
4. 补 `acceptance/M2.md`（引用 K8 的实机证据）。
5. K10、K11，然后做 M3 阶段验收。
6. P1 开工前先和用户确认方案。
