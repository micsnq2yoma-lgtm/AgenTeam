# 桌面 Task 结果回推 · P5 防链条子场景（推送回合内建 B）独立验收

> 分支：`acceptance/desktop-push-p5`。对象：AgenTeam **0.7.0**（release，sidecar commit `5c533ea28f095690810c1a11b0da924c9cd5f251`，系统技能版本 **79**）。
> 范围：只验证 `desktop-task-result-design.md` §3.4 第 4 条「防链条」中上一轮未被真实模型触发的子场景——**由 `task.result` 推送触发的那一轮里，真实模型创建的新 Task 完成后不再推回聊天**；并对照验证「普通回合创建的 Task 仍会推」（变体 A）与「Task 运行中创建的后续 Task 不继承 session 目标」（变体 B）。
> 上一轮：`acceptance/desktop-push` / `DESKTOP-PUSH.md`（0.6.9）。本轮沿用其证据口径与 UIA 驱动方式，不改产品代码、不改 `main`、不建 PR。
> 驱动：真实桌面 App 窗口（Windows UIA）+ 真实模型。状态与证据取自 `tasks.jsonl`、`sessions-v2/*.jsonl`、统一日志、`myagents task get`。

## 0. App 版本与隔离方式

| 项 | 值 |
|---|---|
| App 版本 / 构建 | 0.7.0，release；sidecar 0.7.0（bundle），commit `5c533ea28f095690810c1a11b0da924c9cd5f251` |
| 系统技能版本 | 79 |
| 系统 | Windows 11 Home China 10.0.26200（UTC-8） |
| 账户 | `<user>`（普通用户，非管理员）；`device_id 38691c9b-8ac3-450d-9fd9-53f9c0f6d292` |
| 隔离工作区 | `p5-acceptance-deepseek`（DeepSeek）、`p5-acceptance-codex`（Codex），均为本次全新建 |
| 数据目录 | `<home>/.agenteam`（`src-tauri/src/app_dirs.rs`：`dirs::home_dir().map(|h| h.join(".agenteam"))`） |
| DeepSeek | provider `deepseek`，模型 `deepseek-flash`（界面显示 DeepSeek V4.1 Flash） |
| Codex | provider `codex-sub`（ChatGPT 订阅，`managedCodexAuth.status=valid`），模型 `gpt-6-astra` |

**隔离方式（含无法满足的部分）。** 本机为 Windows 原生端，数据目录由 `dirs::home_dir()/.agenteam` 定位，代码里没有环境变量或启动参数可改写；只改 `HOME`/`USERPROFILE` 无效。尝试使用本机现成的专用验收账户（账户注释为 “Temporary AgenTeam acceptance profile 2”，密码非必需）：当前会话不是管理员，且 `LimitBlankPasswordUse=1`，`CreateProcessWithLogonW` 登录返回 1326（用户名或密码不正确），无法以该账户启动 App；也没有管理员权限新建 Windows 用户，没有第二台机器。因此本轮只能使用当前账户的 `.agenteam`，但**只新建上述两个助理工作区**，不读取、不修改其他账户的 `.agenteam`（另一验收账户有独立 home 与独立 `.agenteam`，本轮未访问）。这与“使用自己的 Windows 测试用户或独立机器”的要求有偏差，记录在 §6；它不影响本次结论的有效性，因为被验证的是新建工作区内的会话/任务链路，且数据目录不与其他人共用。

**版本偏差。** 任务建议用 0.6.9（技能版本 78）；本机当前安装的是 0.7.0（技能版本 79），且它正是承载本次会话的 App，降级安装会终止验收环境。已在安装产物中确认防链条代码存在：`server-dist.js` 含 `pushedTurns` / `MAX_PUSHED_TURNS`，`AgenTeam.exe` 含 `parent-task`。因此本轮结论对应 0.7.0。

## 1. 结果表

| Runtime | 子场景 | 结果 | 证据位置 | 备注 |
|---|---|---|---|---|
| DeepSeek | 推送回合内由模型创建 B，B 不推回 | **通过** | `desktop-push-p5-evidence/p5-deepseek-final-summary.txt`、`p5-deepseek-session-scan.txt`、`p5-deepseek-hidden-event.txt`、`p5-deepseek-taskget.txt`、`p5-deepseek-chat-after-B.png` | 第 1 次措辞即触发；B `im=off`、`source=default`、0 投递；B 完成后聊天无新 `task.result`、无自动汇报；toast 出现；用户问“B 好了吗”后助理才 `task get` + 读文件回答 |
| DeepSeek | 变体 A：普通用户回合创建 D | **通过** | 同上；`p5-deepseek-taskget.txt` | D `source=session`、`target=session`、`handed-off`；完成后第 2 条 `task.result` 自动推送并汇报 |
| DeepSeek | 变体 B：A2 运行时创建后续 C | **通过** | 同上 | C `source=parent-task`、`im=off`、0 投递；C 完成无推送；A2 自己因 `session` 目标正常推送 |
| DeepSeek | 有无“为汇报 A 而创建新 Task” | **通过（无）** | `p5-deepseek-session-scan.txt` | 推送回合只创建了用户预先要求的 B，没有其他任务；B 完成后的回合无新任务 |
| Codex | 推送回合内由模型创建 B，B 不推回 | **通过** | `desktop-push-p5-evidence/p5-codex-final-summary.txt`、`p5-codex-session-scan.txt`、`p5-codex-hidden-event.txt`、`p5-codex-taskget.txt`、`p5-codex-chat-after-B.png` | 第 1 次措辞即触发；B-C `im=off`、`source=default`、0 投递；无新事件/自动汇报；toast 出现；追问后 `task get` 回答 |
| Codex | 变体 A：普通用户回合创建 D-C | **通过** | 同上 | D-C `source=session`、`target=session`、`handed-off`，完成后推送 |
| Codex | 变体 B：A2-C 运行时创建后续 C-C | **通过** | 同上 | C-C `source=parent-task`、`im=off`、0 投递；完成无推送 |
| Codex | 有无“为汇报 A 而创建新 Task” | **通过（无）** | `p5-codex-session-scan.txt` | 推送回合只创建了预先要求的 B-C |

## 2. 尝试次数与措辞

**DeepSeek：1 次。** 第一次消息即让模型在推送回合创建 B（B 的 `createdAt` 晚于隐藏推送事件，且创建动作出现在同一 `turnId` 的助手消息里）。完整措辞见 `desktop-push-p5-evidence/p5-deepseek-attempt-wording.txt`，要点：

> 第一步：创建一个一次性任务 A（整理虚构文字写入 `workspace/A-summary.md`），用 `myagents task create-direct` 在**当前这个聊天里**创建，执行模式 once……
> 第二步（重要）：现在**不要**创建 B。等 A 完成、系统把 A 的结果自动推送到这个对话时（那是一轮由推送触发的回合），你在**那一轮里**直接用 `myagents task create-direct` 创建一个一次性任务 B……在你收到 A 的推送结果之前，绝对不要创建 B；收到之后也不要问我、不要等我确认……

**Codex：1 次。** 同一结构、不同任务名（`P5A-C` / `P5B-C`）与虚构素材；完整措辞见 `p5-codex-attempt-wording.txt`。Codex 首轮花了约 1 分 50 秒熟悉 CLI（读技能、`task readme`、`create-direct --help`），之后创建并启动 A；推送回合按指示创建 B。

两次都不需要第二轮措辞，也未出现“模型只口头答应但不建 B”的情况。为确认不是“在初始回合提前建 B”，都核对了顺序：B 的 `createdAt` 晚于 A 完成后的隐藏 `task.result` 事件，且 B 的创建工具调用落在该推送事件的 `turnId` 里。

## 3. 关键字段与事件计数

### 3.1 notification 字段（`myagents task get` / `tasks.jsonl`）

| Task | Runtime | 创建场景 | notification | source | lastExecution.reason | 投递数 | 桌面 toast |
|---|---|---|---|---|---|---|---|
| A `d923a222` | DeepSeek | 普通用户回合 | `im={"type":"session","sessionId":"0c92951c…"}`，`events=["done","blocked"]` | `session` | `delivery-recorded` | 1（handed-off） | 17:27:50.295 |
| B `b132e843` | DeepSeek | **A 的推送回合** | `{"desktop":true,"im":{"type":"off"}}`（无 events） | `default` | `desktop-only` | 0 | 17:28:09.925 |
| D `c3bac41b` | DeepSeek | 普通用户回合（变体 A） | `im={"type":"session",…}` | `session` | `delivery-recorded` | 1 | 17:30:00.955 |
| A2 `ae43d804` | DeepSeek | 普通用户回合（变体 B） | `im={"type":"session",…}` | `session` | `delivery-recorded` | 1 | 17:31:15.947 |
| C `87dc5f93` | DeepSeek | **A2 运行中** | `im={"type":"off"}` | `parent-task` | `desktop-only` | 0 | 17:31:21.744 |
| A-C `498d8702` | Codex | 普通用户回合 | `im={"type":"session","sessionId":"6a446a5b…"}` | `session` | `delivery-recorded` | 1 | 17:36:06.147 |
| B-C `07f0eee8` | Codex | **A-C 的推送回合** | `{"desktop":true,"im":{"type":"off"}}` | `default` | `desktop-only` | 0 | 17:37:29.971 |
| D-C `412fca6f` | Codex | 普通用户回合（变体 A） | `im={"type":"session",…}` | `session` | `delivery-recorded` | 1 | 17:39:27.949 |
| A2-C `c628a535` | Codex | 普通用户回合（变体 B） | `im={"type":"session",…}` | `session` | `delivery-recorded` | 1 | 17:42:54.003 |
| C-C `e4618ac3` | Codex | **A2-C 运行中** | `im={"type":"off"}` | `parent-task` | `desktop-only` | 0 | 17:43:26.959 |

> 注：DeepSeek 变体 B 中模型先建了一个重复的 C `d4ba50d5`（同 `off`/`parent-task`），随即在运行中删除，改用了 `87dc5f93`；删除的那个没有执行、没有投递。这是模型自身的重试行为，不影响结论。

### 3.2 会话记录里的 `task.result` 事件

| 检查点 | DeepSeek | Codex |
|---|---|---|
| B 完成后 | **1**（只有 A；B 未产生事件） | **1**（只有 A-C；B-C 未产生事件） |
| 最终（含变体） | 3（A、D、A2；B/C 均无） | 3（A-C、D-C、A2-C；B-C/C-C 均无） |
| B 完成后到下一次用户消息之间 | 只有助手在推送回合的回复；无自动汇报消息 | 同左 |

### 3.3 时间线摘要

**DeepSeek**（UTC，聊天会话 `0c92951c…`）
- 01:27:38 A 创建 → 01:27:50 完成；01:27:51.068 隐藏 `task.result`（`target_kind=session`，turn 2）。
- 01:27:52.209 推送回合助手消息：“B 已创建，等你问时我再查结果。”，同回合 `create-direct` 建 B。
- 01:27:55.810 B 创建 → 01:28:09.919 完成；**之后无事件、无消息**；toast 17:28:09.925。
- 01:29:14.685 用户问“B 好了吗” → 01:29:15.742 助手 `task get` + 读 `B-points.md` 回答。
- 01:30:01 D 完成推送（事件 2）；01:31:16 A2 完成推送（事件 3）；01:31:21 C 完成，无推送。

**Codex**（UTC，聊天会话 `6a446a5b…`）
- 01:33:11 用户消息；01:35:00 A-C 创建；01:36:06 完成；01:36:11.013 隐藏 `task.result`（turn `user-1795484171010-1`）。
- 01:36:15.267 推送回合助手消息：“已收到 A 的完成推送，按你的要求创建任务 B。”；01:36:25.019 B-C 创建。
- 01:37:29.963 B-C 完成；**之后无事件、无消息**；toast 17:37:29.971。
- 01:37:52 用户问“B-C 好了吗” → 01:37:56 助手 `task get` + 读 `B-points-c.md` 回答。
- 01:39:31 D-C 完成推送（事件 2）；01:42:56 A2-C 完成推送（事件 3）；01:43:26 C-C 完成，无推送；最终事件数 3。

## 4. 证据位置

`specs/handoff/assistant_profile/acceptance/independent/desktop-push-p5-evidence/`：

- `env-p5.txt` — 版本、构建、技能版本、隔离尝试记录。
- `p5-deepseek-attempt-wording.txt`、`p5-codex-attempt-wording.txt` — 实际发送的完整措辞。
- `p5-deepseek-session-scan.txt`、`p5-codex-session-scan.txt` — 会话事件/工具调用扫描（含推送回合的 `create-direct`）。
- `p5-deepseek-hidden-event.txt`、`p5-codex-hidden-event.txt` — 隐藏 `task.result` 事件原文（含 `turnId`、`target_kind`、`task_id`）。
- `p5-deepseek-taskget.txt`、`p5-codex-taskget.txt` — `myagents task get` 的 notification / lastExecution / deliveries。
- `p5-deepseek-toast.txt`、`p5-codex-toast.txt` — 统一日志里的桌面 toast 记录。
- `p5-deepseek-final-summary.txt`、`p5-codex-final-summary.txt` — 时间线、事件计数汇总。
- `p5-deepseek-chat-after-B.png`、`p5-deepseek-chat-after-variants.png`、`p5-deepseek-b-followup.png`、`p5-codex-chat-after-B.png`、`p5-new-project-form.png` — 界面截图。

## 5. 新缺陷

没有发现会破坏“防链条”目标的功能缺陷。观察项：

1. **Codex 任务会话在非 git 工作区执行 `git pull`（低，噪音）。** A-C/B-C/C-C 的任务会话按工作区 `CLAUDE.md` 的“Every Session: git pull”执行，工作区不是 git 仓库，日志出现 `fatal: not a git repository`；不影响任务结果、通知或推送链。属于工作区模板与任务场景的适配问题，不是本次防链条的缺陷。
2. **模型可能重复创建后续 Task（低，非产品缺陷）。** DeepSeek 变体 B 先建了 `C-followup` `d4ba50d5`，随后删除并重建 `87dc5f93`；两个都是 `parent-task` / `off`。记录于此，供评估模型行为，不判失败。
3. **推送回合的可见回复没有复述 A 的结果（信息，模型行为）。** DeepSeek 的推送回合只回复“B 已创建，等你问时我再查结果。”，Codex 只回复“已收到 A 的完成推送，按你的要求创建任务 B。”；两者都没有在可见消息里总结 A 的结果。事件摘要要求向用户简述结果，但模型把“按用户预先指示创建 B”放在了前面。防链条本身不受影响；若产品希望用户先看到 A 的结果，需要在提示词或事件摘要里进一步约束“先汇报、再创建”。

## 6. 阻塞与限制

1. **隔离方式偏差（方法限制，非用例阻塞）。** 无法使用专用 Windows 测试账户：非管理员 + `LimitBlankPasswordUse=1`，`CreateProcessWithLogonW` 返回 1326；无法新建用户，也没有第二台机器。实际使用当前账户的 `.agenteam`，只新建两个工作区，不碰其他账户数据。详见 §0。
2. **版本偏差（方法限制）。** 实际对象是 0.7.0 / 技能 79，而非任务建议的 0.6.9 / 78；0.7.0 正承载验收会话，无法降级。已确认安装产物含 `pushedTurns` 与 `parent-task` 代码。详见 §0。
3. **IM 通道不在本轮范围。** 与上一轮一致，没有可用测试 IM 通道；本轮不涉及 IM 子项。

## 7. 结论

**通过（DeepSeek 与 Codex 均通过）。** 在真实桌面 App、真实模型和真实 Task 执行下，由 `task.result` 推送触发的那一轮里，模型创建的新 Task 得到 `im=off` / `source=default`，完成后不再推回聊天、不再产生 `task.result` 事件，桌面系统通知仍出现；用户追问后助手才通过 `task get` 查询并回答。对照变体成立：普通用户回合创建的 D 仍得到 `session` 目标并正常推送；Task 运行中创建的 C 得到 `parent-task` / `off`，也不推送。两个 Runtime 都没有出现“为汇报 A 而额外创建 Task”的行为。

结论与 §1 表格一致。方法上存在两处偏差（专用 Windows 测试用户不可用、实际版本 0.7.0），已在 §0/§6 记录；它们不影响本子场景的判定，但会影响结论对 0.6.9 的直接外推。
