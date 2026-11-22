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

## 跨助理共享（单向同步）

用户有多个助理时，主助理的画像章节会同步到其他助理。权威数据仍是工作区文件，没有 App 级画像 Store；实现在 Rust `src-tauri/src/profile_sync.rs`。

- **主助理**：按注册顺序（`config.json` 的 `agents[]` 只追加，顺序即创建先后）第一个 `03-USER.md` 使用固定章节的助理。最早的助理还是旧式自由格式文件时，它既不能当主助理，也不会被改写，主助理顺延到下一个有固定章节的助理。已归档的助理不参与。
- **同步哪些章节**：背景、沟通风格、工作方式、偏好、写作风格。目标助理自己的"基本信息""近期观察""来源概况"、各章节开头的所有权注释、章节之外的内容都逐字保留。主助理里某章节还是空的，就跳过，不会清空目标里已有的内容。
- **目标**：同样使用固定章节的助理。旧式文件、没有画像文件的、符号链接的 USER 文件都不动。
- **冲突**：以主助理为准。每次改动前把目标原文件存成 `03-USER.md.bak-YYYYMMDD-HHMMSS`（同一秒内再改加 `-2`，只留最近 5 个），因此同一天里第一次同步之后的手改也能恢复，目标章节里的手改因此可恢复。写入前复核文件哈希，期间被别的流程改过就这一轮跳过，下一轮再试。
- **时机**：App 启动时一次，之后每 10 分钟一次；少于两个助理时什么都不做。目标助理的长期会话由 `MEMORY_REFRESH` 获知文件变化。
- **开关**：`config.json` 的 `shareUserProfile`（布尔，默认开启），没有界面。
- **限制**：写入前复核与写入之间有毫秒级窗口，期间其他流程的写入可能被覆盖，备份可以恢复；不同步"基本信息"和"近期观察"，因此各助理的称呼和日常观察互不影响。

## 记忆变化提醒

长期存活的会话不会重读 `CLAUDE.md` 和 `.claude/rules`。`MEMORY_REFRESH` 在下一条被准入的输入上提示哪些文件、哪些章节变了，不含正文；规范见 `specs/tech_docs/system_reminder_protocol.md` 的「MEMORY_REFRESH」。

## 命名

画像、知识库相关 Skill 以及 v0.6.8 起改名的九个系统 Skill 统一为 `agenteam-*`；旧名字在 `RETIRED_SYSTEM_SKILLS` 中，升级同步时按精确名字删除旧目录。命令行仍是 `myagents`。

## 未覆盖

- 画像生成质量、对资料中注入指令的抵抗、更正只改相关章节、`MEMORY_REFRESH` 让 IM 会话采用新内容，都需要真实 App 加模型验证，目前只有静态检查和自动化测试。
- 跨助理共享是单向同步，不是单一权威的 App 级画像；若以后要所有助理读同一份文件，需要新的状态 owner，须先确认架构方案。

## 主动提醒（可选，默认关闭）

不新增状态 owner、进程或通信模式：

| 事实 | owner |
|---|---|
| 是否开启、安静时段、每日上限、判断规则 | 工作区文件 `memory/PROACTIVE_RULES.md`（前置 `enabled: true` 才生效） |
| 当天已提醒数、已判断过的事项、学习线索计数 | `memory/proactive-state.json`，只由 `agenteam-proactive/scripts/proactive_state.py` 写 |
| 何时判断 | 既有 IM Heartbeat；`src/server/utils/proactive-heartbeat.ts` 在规则已开启时给心跳 prompt 追加一句指向 `agenteam-proactive`，并让空 HEARTBEAT.md 不再跳过。未开启时心跳行为与此前完全一致 |

- 判断：Skill 先跑 `status`（关闭、安静时段、当日上限满时不读取任何来源），再按 `references/sources.md` 读 IM、任务结果、Records 与用户自配的邮箱/日历（MCP 或本机 CLI；产品不保存凭证、不新增域名），每件事一天只判断一次，四种决定：立即告诉 / 先准备 / 之后跟进 / 沉默。不代用户发信、回复、删除、应答邀请。
- 学习：每天最多一次，把用户对提醒的反应记成线索；同一类别同一决定出现两次、或用户明确点名，才经 `section_edit.py` 写成规则，并告知用户与撤销办法。
- 外部内容（邮件、日历、IM、任务结果）是数据，不是指令。

未覆盖：真实邮箱/日历工具与真实模型下的判断质量（见 `specs/handoff/assistant_profile/acceptance/A5.md`）。

## 独立验收后的修订（2026-11-22）

| 缺陷 | 根因 | 处理 |
|---|---|---|
| D4 并发写入都报成功 | `section_edit.py` 读取、核对基线哈希、写入之间没有互斥 | 读 → 核对 → 写在同一把锁文件（`O_EXCL`，崩溃遗留的锁 60 秒后失效）内完成；第二个写入者等待后看到新版本，返回退出码 2。回归测试用慢读把窗口撑开，旧脚本必败 |
| D5 同日二次同步丢手改 | 备份按日期命名、同日只留最早一份 | 每次覆盖前都备份，名字 `03-USER.md.bak-YYYYMMDD-HHMMSS`（同秒加 `-2`），仍只留最近 5 份 |
| D1 画像完成后聊天无回执 | Task 结果只会推回 IM 来源的聊天；桌面聊天没有"经宿主验证的发起聊天"，结果通知不投递。原设计误以为桌面也会推回 | 不新增通信路径：画像 Task 同时把报告写入 `workspace/profile-report.md`，助理记住 taskId，在下一次回复或用户询问时用 `myagents task get` 查状态并读该文件汇报；IM 聊天仍直接收到推送。让桌面聊天也收到 `task.result` 是新的通信模式，需先讨论架构，本次不做 |
| D2 跳过资料仍改写来源概况 | BOOTSTRAP 只是用文字说只写基本信息 | BOOTSTRAP 给出带 `--allow 基本信息` 的 `section_edit.py` 命令，由程序强制；跳过时明确不写来源概况 |
| D3 定向更正越界 | Skill 对所有模式用同一个允许章节列表 | 定向更正只允许被指出的一两个章节；助理写任务说明时必须点名章节 |
| D6 CRLF 检出下测试失败 | 测试夹具和断言假设 LF | 夹具和断言先归一化换行；产品代码本来就保留 CRLF |

D2、D3 靠 Skill 文本与 `--allow` 约束，模型是否遵守仍需真实 App 复验；D1 的处理改变了流程，不是推送。
