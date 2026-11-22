# 助理用户画像与记忆维护

适用于助理模式（`bundled-workspaces/atmer`）及任何使用 `.claude/rules/*USER*.md` 的工作区。设计背景和取舍见 `specs/handoff/assistant_profile/proposal.md`；本文只记录当前实现的 owner、不变量和限制。

## 数据与 owner

用户画像是**工作区文件**，不是 App 级状态。Rust 的 `workspace_files::memory_rules` 只在缺失时补齐默认模板（`src/shared/default-user.md`），已有文件一律不改写。文件内容由下面几个流程按章节分工写入，没有任何流程拥有整个文件。

固定章节与顺序（`src/shared/userProfile.test.ts` 校验两份模板一致）：基本信息、背景、沟通风格、工作方式、偏好、写作风格、近期观察、来源概况。章节标题用中文且固定，内容用用户自己的语言写。每个章节开头的 `<!-- 写：… 由：… -->` 注释说明写什么、谁来写。

| 章节 | 谁可以写 | 方式 |
|---|---|---|
| 基本信息 | 用户本人、`/BOOTSTRAP`、用户明确更正 | 对话或 `--allow 基本信息` |
| 背景、沟通风格、工作方式、偏好、写作风格 | 画像 Task（`agenteam-user-profile`）；Gardener 合并"近期观察"时 | `section_edit.py` |
| 近期观察 | 日常对话和记忆更新只追加；Gardener 合并后删除 | `section_edit.py --allow 近期观察` |
| 来源概况 | 画像 Task | `section_edit.py` |

没有"近期观察"章节的旧式 USER 文件保持原有行为：各流程不为它新增章节，lint 不对它产生新检查。

## 流程

- **生成**：`/BOOTSTRAP` 询问用户是否提供资料（简历、网页、文档、文件夹），用户给了才由助理写自包含的 `workspace/profile-task.md`，用 `myagents task create-direct … --executionMode once` 加 `task run` 创建一次性 Task，加载 `agenteam-user-profile`。规则放在助理的 `CLAUDE.md`，因为 BOOTSTRAP 会删除自己，而 Task 结果稍后才回到聊天。只读用户明确给出的资料，画像里只保留理解，不保留原文、联系方式或标识符。
- **更新与更正**：用户说"更新我的画像"或指出写错时走同一套 Task 流程，分别是增量更新和定向更正；用户更正优先于画像和资料。
- **日常学习**：`agenteam-memory-update` 把新认识以带日期的一行追加到"近期观察"。
- **整理**：`agenteam-memory-gardener` 把被明确表达或反复验证的条目并入主体章节并删除原条目，归属不明的留着；"近期观察"超过 20 条时 `memory_lint.py` 给 WARN。USER 文件预算沿用 20 KB（lint 阈值），画像 Skill 把约 6 KB 作为篇幅建议。

## `section_edit.py`

三个 Skill 各带一份字节相同的 `scripts/section_edit.py`（`scripts/section-edit.test.mjs` 校验一致，规范副本在 Gardener 下）。它是各流程写 USER 文件的唯一方式：

- 只改点名的 `## ` 章节（`replace`、`append`、`add`、`delete`），其余内容逐字保留；`replace` 保留章节开头的所有权注释。围栏代码中的 `## ` 是内容；重复标题视为歧义并拒绝；内容里不允许出现 `## `。
- `--base-sha`：文件自读取后被改过则退出码 2 并拒绝写入，调用方必须重读后再提交。这是多个 Session（桌面、IM、定时任务）同时写同一文件时防止丢失更新的机制。
- `--allow`：章节白名单，退出码 3；任何一条编辑被拒绝，整批都不写入。**章节归属由程序强制**，`scripts/section-edit.test.mjs` 校验三个 Skill 声明的白名单与上表一致。
- `--max-bytes`：结果超限退出码 4，不写入。写入用同目录临时文件加 `os.replace`，保留 CRLF。

限制：脚本保证的是"单次写入原子且基于最新读取"，不是跨文件事务；两个流程若各自改**不同**章节且都带了过期的 `--base-sha`，后者会被要求重读再提交，这是有意的保守。

## 记忆变化提醒

长期存活的会话不会重读 `CLAUDE.md` 和 `.claude/rules`。`MEMORY_REFRESH` 在下一条被准入的输入上提示哪些文件、哪些章节变了，不含正文；规范见 `specs/tech_docs/system_reminder_protocol.md` 的「MEMORY_REFRESH」。

## 命名

画像、知识库相关 Skill 以及 v0.6.8 起改名的九个系统 Skill 统一为 `agenteam-*`；旧名字在 `RETIRED_SYSTEM_SKILLS` 中，升级同步时按精确名字删除旧目录。命令行仍是 `myagents`。

## 未覆盖

- 画像生成质量、对资料中注入指令的抵抗、更正只改相关章节、`MEMORY_REFRESH` 让 IM 会话采用新内容，都需要真实 App 加模型验证，目前只有静态检查和自动化测试。
- 跨 Agent 共享的统一画像、主动提醒与规则学习没有实现，需要新的状态 owner，开始前须先确认架构方案。
