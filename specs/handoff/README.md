# 后续开发计划

> 更新：2026-11-24。**知识库（K9 及以后）的开发与验收交给新的协作者，交接说明见 [`knowledge_base/HANDOFF.md`](./knowledge_base/HANDOFF.md)。** 以下分工说明适用于其余方向。
>
> 原分工（2026-11-22）：**Claude 负责开发与自检，不再使用 Codex 协作者。** 开发与验收是同一会话，验收记录里标注"自检"；需要独立复核时由用户另开会话。合并到 `main`、推 tag、发布仍需要用户明确授权。

## 当前状态

| 方向 | 已完成（在 `main`） | 进行中 |
|---|---|---|
| 知识库 | K0–K8：契约、Store、摄取、切块、BM25 索引、会话 CLI、Agent/Session 绑定、Skill 与库规则模板、整理 Task 与 wiki；**M1 能检索**已通过 | K9 知识库 UI（`feat/kb-k9-entry`），交给新协作者，见 `knowledge_base/HANDOFF.md` |
| 助理记忆 | atmer 模板的记忆使用规则 | `MEMORY_REFRESH`（方案 B1，分支 `feat/memory-refresh-reminder`，CI 分支 `dev/memory-refresh`，待 CI 通过并经用户授权后合并） |
| 侧栏状态、后台超时、Launcher | 均已合并 | — |

> 记忆功能的独立验收见 `assistant_profile/acceptance-plan.md`（真实 App 加模型的未验证项，由用户另行指派验收人）。

## 任务队列（按顺序做，一次一个任务）

| 顺序 | 任务 | 规格 | 说明 |
|---|---|---|---|
| 1 | **K8** 整理 Task（元数据 + wiki）、wiki CLI、`kb curation context` 只读命令 | `knowledge_base/tasks.md` K8 | 完成后补验 **M2 能整理**。K8 要同步修改 `agenteam-knowledge-curator` Skill（K7 遗留，见 `acceptance/K7.md` "偏离"一节）并 bump 系统 Skill 版本 |
| 2 | **A1** 画像模板与 `agenteam-user-profile` Skill | `assistant_profile/tasks.md` A1 | |
| 3 | **A2** BOOTSTRAP 收集资料 | `assistant_profile/tasks.md` A2 | 依赖 A1 |
| 4 | **A3** 记忆 Skill 的章节归属 | `assistant_profile/tasks.md` A3 | 依赖 A1；USER 预算沿用 20 KB，待用户确认 |
| 5 | **K9** 知识库 UI | `knowledge_base/tasks.md` K9 | |
| 6 | **K10** 引用 chip | `knowledge_base/tasks.md` K10 | |
| 7 | **K11** 端到端与检索评测 | `knowledge_base/tasks.md` K11 | 完成后补验 **M3 P0 完成** |

P1（K12 及以后）、跨 Agent 共享画像、主动提醒都不在本轮队列，开始前需要用户另行确认方案。

## 开发流程

以下流程原为交接给协作者写的，现在由 Claude 自己遵守；"交付报告"同时作为自检记录。

1. **开工前**：读根目录 `CLAUDE.md`、`specs/ARCHITECTURE.md` 相关章节，以及任务规格"必读"列出的文档和代码。新增 helper 前先用 `rg` 找现成实现。
2. **分支**：知识库任务用 `feat/kb-k<n>-<短名>`，助理任务用 `dev/assistant-a<n>-<短名>`，都基于最新 `main`。**Test 工作流只在 `main`、`dev/**`、`feat/kb-*` 上自动运行。**
3. **提交**：显式列文件，禁止 `git add -A` / `git add .` / `git add -f`；Conventional Commits，subject 加上说明为什么和关键取舍的非空 body。不提交 `specs/prd/`。
4. **完成定义与红线**：`knowledge_base/tasks.md` §0.3（D1–D9）和 §0.4（八条红线），助理任务另见 `assistant_profile/tasks.md` §0。
5. **交付**：按 `knowledge_base/tasks.md` §0.5 的模板写交付报告，放在 `<方向>/delivery/<任务>.md`，随分支提交。附上正式 CI 运行链接，以及本机 D1–D4 命令的尾部输出。
6. **自检验收**：按 `knowledge_base/tasks.md` §0.6 执行：静态审查 → 复跑 D1–D4 → 对标了 🔒 的项写对抗探针 → 手动项在能运行的环境里验证，不能验证的明确写"未验证"，不写成通过 → 结论写入 `<方向>/acceptance/<任务>.md`。
7. **何时停下来问用户**：规格与代码现状冲突、需要新的状态 owner、进程或通信模式、需要修改红线涉及的边界，或无法满足某条验收项时，停下来说明，不要自行取舍。

## 已知环境问题（不属于回归）

- 以 root 身份运行时，`project-user-config-sync` 的只读目录测试失效；没有 IPv6 的环境中 `DshSafeHttpClient ::1` 测试失败。
- Windows runner 偶发 `scripts/node-runtime.test.mjs` 临时目录 `EPERM`，以及慢速 runner 上知识库测试的 5 秒文件锁超时；重跑通过即可，在报告中注明。
