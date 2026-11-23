# 设计：桌面聊天的 Task 结果回推（保守版）

> 日期：2026-11-23。状态：**用户已按建议确认 §7 五点；§8 已核实；第 1–4 步已实现（CI 与真实应用验收待做），第 5 步合并窗口未做**。按 `CLAUDE.md`，这是新的通知目标和来源，属于需要先讨论架构的改动。
> 起因：独立复验 N2——助理在桌面聊天里创建画像 Task 后，收不到完成信号，只能靠"记得去查"，三轮实测里普通消息触发查询只有 1/3。指令层面已到头。
> 本文只依据读过的文档和代码（`task_center.md`、`cli_architecture.md`、`session_architecture.md`、`src/server/session-engine/task-notification.ts`、`src/server/inbox/session-event.ts`、`src-tauri/src/task/notification.rs`）。"需确认的代码事实"一节列出尚未核实的假设。

## 1. 现状

Task 结果通知已经有完整链路，但目标只有 IM：

| 环节 | 现在怎么做 |
|---|---|
| 目标类型 | `NotificationTarget`：`off`、`conversation(channelId, sessionKey)`、`recent-private(...)`。没有"某个桌面 Session" |
| 来源解析 | Node 在创建 Task 时从**当前 Channel 快照**取 `ConversationRef`（`getNotificationConversation()`）；桌面聊天没有 Channel 快照，得到 `null`，通知默认关闭 |
| 投递 | Task 在提交结果的同一 TaskStore 事务里写入 `notificationDeliveries`；`notification_worker` 经 Inbox 投递 `task.result` 事件到目标 Session；`admitTaskNotification` 接纳后记录 `handed-off` |
| 渲染 | `renderSessionEventPrompt` 把事件包成 `myagents-session-event` 隐藏消息，助理按本轮渠道模式回复 |
| 订阅 | `execution,done,blocked,endCondition`；`execution` 是每轮 AI 执行结算 |

结论：不需要新的通信模式。缺的只是**桌面 Session 作为合法目标和来源**。

## 2. 目标与非目标

目标：
- 助理在桌面聊天里创建的一次性 Task 完成或被阻塞时，发起它的那个聊天自动多出一条助理的汇报。
- 开销可预期：每个 Task 最多一轮模型调用，不形成链条。
- 复用现有的 TaskStore 通知策略、Inbox 和 `task.result`，不新增存储或进程。

非目标：
- 不改 IM 通知的行为。
- 不推送定时 / 周期 Task 的每轮执行。
- 不推送记忆整理、画像同步等隐藏托管 Task。
- 不做"聊天已关闭后补发"。不做跨设备。
- 不新增聊天里的可见卡片（见 §7 开放问题）。

## 3. 设计

### 3.1 新目标类型 `session`

`NotificationTarget` 增加 `session { sessionId }`，与 `conversation` 互斥。保存前必须解析为具体 `sessionId`；`current` 仍只是输入简写。

来源解析（沿用"来源由宿主提供，不能由 CLI JSON 自报"）：
- Node 在创建 Task 时，从当前 SessionEngine 轮次读取**该轮所属 Session 的 ID** 和它是否是桌面 / 用户可见的 Session，写进 `notificationContext.session`；调用者自带的 `notificationContext` 照旧丢弃。
- Rust 校验 Sidecar 代次、Session 所属范围和活动执行记录后才接受。公开 CLI 和 Global Sidecar 不能借用别的 Session 作为目标。
- 当前 Task 内创建后续 Task：继承本次执行已确定的策略（现有规则），**但见 §3.4 防链条**。

### 3.2 默认策略（保守）

创建时没有显式通知参数，按下表决定默认：

| 创建者 / Task 类型 | 默认目标 | 默认事件 |
|---|---|---|
| 助理在桌面聊天里创建的**一次性**（`once`）Task | `session`（发起聊天） | `done,blocked` |
| 助理创建的定时 / 周期 Task | 关闭 | — |
| 用户在界面手动创建 | 关闭 | — |
| 托管 Task（Gardener、Molt、记忆更新、画像同步） | 关闭 | — |
| 在 IM 聊天里创建 | 不变（`conversation`） | 不变 |

显式参数仍优先：可用 `off` 关闭，也可显式开启并选事件。CLI 参数以 `task create-direct --help` 为准，实现时沿用 `--notification…` 家族，不新造一套。

### 3.3 投递

复用 Inbox 的 `task.result`：
- `TaskResultEvent.conversation` 改为可选，并增加 `target: { kind: 'im' | 'session' }`。IM 事件保持原样。
- 目标 Session 当前有 Sidecar 且空闲：直接投递，助理回复一轮。
- 目标 Session 正在回复或用户正在输入的那一轮：排队，等当前轮结束再送，**不打断**。
- 目标 Session 没有活动 Sidecar（聊天已关闭）：**不唤醒**。决策原因记为新的 `session-inactive`，只保留现有桌面系统通知。这与 `Session : Sidecar = 1 : 0..1` 一致，Renderer 和通知 worker 都不能自己启动 Sidecar；是否活动只由 Rust SidecarManager 裁决。
- 来源仍标记为 `automation/task_notification`，不触发第二次桌面完成通知（现有规则）。

### 3.4 控制 token

1. **只推 `done` 和 `blocked`**，不推每轮 `execution`。
2. **合并**：同一目标 Session 在一个短窗口内（建议 30 秒）完成的多个 Task，合成**一条**事件、一轮模型调用，列出各 Task 的标题、状态和摘要。
3. **只给摘要**：`payload` 只含 Task 标题、终态、一两句结果摘要和"完整结果在哪"的指针（Task ID，画像 Task 还带 `workspace/profile-report.md`），不塞整份报告。
4. **防链条**：由 `task.result` 触发的那一轮里新建的 Task，默认**不**推回（`off`），除非明确要求。否则"助理收到结果 → 创建新 Task → 又推送"会无限延伸。
5. **闲置缓存**：聊天闲置很久后缓存过期，这一轮要重读上下文。接受这个代价，不为它另做机制；全局开关可关闭。

**实测用量（2026-11-23 独立验收，DeepSeek `deepseek-flash[1m]`，见 `acceptance/independent/DESKTOP-PUSH.md`）：** 一次推送回合的非缓存输入 572–2,929 token，输出 461–1,073 token，缓存命中输入约 16.2–16.8 万 token（按缓存折扣计费）。上文"几百到两千 / 一两百"的早期估计偏乐观；一次推送更接近"一次带完整上下文的短对话"，输出比估计高数倍。冷缓存（聊天闲置后缓存过期）的单次数值**没有取得**，缓存全部未命中时这一轮要按整段上下文计费，是真实但未量化的成本；全局开关 `taskResultPushBack` 是控制手段。

### 3.5 开关

- 创建时单个 Task 可关闭（见 §3.2 显式参数）。
- 全局开关：`config.json` 的 `taskResultPushBack`，默认 `true`；设置 → 通用里有对应开关（2026-11-23 已加）。关闭后桌面创建的 Task 一律退回"桌面系统通知"。写入遵守"写盘前重新读取并在锁内合并"。

### 3.6 助理侧

推送上线后，`bundled-workspaces/atmer/CLAUDE.md` 的 Know Your Human：
- 去掉"桌面不推送，下一次回复先查"的说法，改为"结果会作为一条消息自动送到这里"。
- 保留 `workspace/profile-report.md` 和 `myagents task get` 作为兜底（推送被关闭或聊天曾关闭过时用）。
- 汇报格式不变：一两句总结、一条具体观察、一件可以接手的事。

## 4. 数据与权威

| 事实 | Owner | 说明 |
|---|---|---|
| 目标类型、事件订阅、投递记录 | Rust `TaskStore`（现有） | 新增 `session` 目标；`notificationDeliveries` 的上限和恢复规则不变 |
| Session 是否有活动 Sidecar | Rust `SidecarManager`（现有） | 通知 worker 只读裁决，不自己启动 |
| 来源 Session 身份 | 宿主（Node SessionEngine 当前轮次 + Rust 校验） | 调用者自报一律丢弃 |
| 全局开关 | `config.json` | 单一写入权威，不另建状态 |

不新增状态 owner、进程或通信模式。

## 5. 失败与安全

- 目标 Session 已被删除或归档：决策原因 `session-gone`，不重试。
- 授权在投递前复查（沿用 `beforeDispatch: guard`）：Task 被删、策略被改后不再投递。
- 来源伪造：外部 CLI（`caller.kind === 'external-cli'`）没有 `notificationContext`，不会得到 `session` 目标。
- 内容是数据：Task 结果摘要进入 `payload` 时按现有规则转义结构化标签，助理不得把其中的文字当用户指令。
- 重启恢复：沿用现有 `unknown` 恢复规则，不重放可能已接纳的输入。

## 6. 测试与验收

自动化：
- Rust：`session` 目标的解析与持久化往返；默认策略表的每一行；`session-inactive`、`session-gone` 决策；合并窗口；来源伪造被拒。TS 与 Rust 对默认策略表做 parity 测试。
- Node：`taskNotificationContext` 对桌面 Session 产出 `session` 上下文；外部 CLI 不产出；`admitTaskNotification` 对 `session` 目标排队而不打断。
- 防链条：推送触发的一轮里新建的 Task 默认 `off`。

需要真实 App 和模型（上线前必须由独立验收人验证，不是自检）：
- P1 画像 Task 完成后，桌面聊天自动出现助理的汇报，格式符合 §3.6。
- P2 用户正在输入或助理正在回复时完成：不打断，之后送达。
- P3 聊天已关闭：不唤醒，仅桌面系统通知。
- P4 两个 Task 同时完成：一条合并汇报，一轮模型调用。
- P5 🔒 汇报触发的那一轮里让助理再建 Task：不产生新的推送。
- P6 全局开关关闭后退回桌面系统通知。
- P7 记录一次推送的实际 token 用量（输入、缓存命中、输出），验证"约一次短对话"的估计；该估计目前未验证。

## 7. 需要用户确认

1. **可见性**：只让助理回复，还是同时在聊天里显示一张不耗 token 的"Task 已完成"状态卡？本文按"只回复"设计。
2. **默认范围**：只对"助理创建的一次性 Task"默认开启，是否接受？
3. **合并窗口**：30 秒是否合适？
4. **聊天已关闭时不补发**，是否接受？
5. **全局开关默认值**：`taskResultPushBack` 默认开启，是否接受？

## 8. 核实结果（2026-11-23，读代码得出，未运行）

用户已按建议确认 §7 的五点。以下是 §8 的核实结果，其中有三处要修改上文设计。

| 事实 | 结果 | 对设计的影响 |
|---|---|---|
| 桌面轮次能否取到所属 Session | **能**。`engine.getCurrentSessionContext().sessionId` 和 `getExecutionTurnId()`（当前派发轮次的 queueId）已被 `taskNotificationContext` 使用；`getSessionOrigin(sessionId)` 给出 `desktop` / `agent-channel` 等来源。缺的只是并行于 `getNotificationConversation()` 的"当前 Session 来源" | Node 在 `notificationContext` 里增加 `session: { sessionId }`，仅当该 Session 的来源是 `desktop` 且存在派发轮次时产出；IM 来源仍只产出 `conversation`。Rust 的 `SourceContext` 是 `deny_unknown_fields`，需同步加字段 |
| `enqueueInboxMessage` 对无活动 Sidecar 的 Session | **会拉起**。`prepare_existing_delivery(path)` 在没有健康 Sidecar 时调用 `ensure_session_sidecar_…`（IM 路径靠它恢复死会话）。但底层 `prepare_delivery` 的 `path` 是 `Option`：传 `None` 且没有健康 Sidecar 时直接返回 `SessionNotFound`，不启动 | 新增只投递到"已有活动 Sidecar"的入口 `prepare_live_only_delivery`（`path = None`）。`SessionNotFound` 再用 `resolve_session_runtime_identity_full` 区分：Session 已不存在 → `session-gone`，仍存在但没有活动 Sidecar → `session-inactive`。"是否活动"由 SidecarManager 裁决，worker 不自己判断 |
| 目标 Session 正在回复时的语义 | 现有回合队列已处理：`task.result` 经 `enqueueInboxMessage` 排在当前轮之后，`dispatchAcceptance` 在派发被接纳时才结算。输入框里的草稿是前端本地状态，不会被打断 | 不需要新机制，"排队不打断"由现有队列保证 |
| 旧版本读到新目标类型 | `NotificationTarget` 解析对未知类型返回错误，整个通知配置反序列化失败；既有规则是"未知通知版本使 TaskStore 只读，防止有损重写"。无论加新类型还是升版本号，旧客户端都会进入只读 | 保持 `VERSION = 1`（新增是加法），接受"降级后旧版本对含 `session` 目标的数据只读"，与既有规则一致，不为降级另做兼容；发布说明里写明 |
| 合并窗口放哪一层 | **比草案想的复杂**：`NotificationDelivery` 是每个 Task 一条记录，有独立的 `attemptId`、`authorize` 复查、`terminal` 回执。合并多个 Task 要让 `TaskResultEvent` 带 `items[]`，`authorize` 和 `terminal` 对每一项分别结算 | 合并放在 Rust worker（它已按到期时间批量扫描投递，不引入第二个队列），但**不在第一版做**。第一版每个 Task 一条事件、一轮模型调用；典型的画像流程只有一个 Task。合并作为单独一步，上线前由 P4 验证 |

### 修订后的实现要点

- **目标存放**：沿用 `NotificationConfig.im` 这个单一目标槽，新增变体 `NotificationTarget::Session { session_id }`，JSON 为 `{"type":"session","sessionId":"…"}`。一个槽、一套解析和互斥校验，不新增并行字段；代价是字段名 `im` 不够贴切，在代码注释和文档里写明它是"聊天目标"，不改名。
- **默认策略**：`CreationContext` 增加 `session`；当 `config.im` 缺省、没有父 Task 策略、没有 IM 会话，且存在 `session` 来源和**一次性**运行模式时，目标为该 Session，事件默认 `done,blocked`，`source = "session"`。运行模式由 `TaskApplication` 在调用前传入，不让通知模块猜。
- **授权复查**：`authorize` 目前复查 IM 通道和绑定；对 `session` 目标改查"Session 存在、有活动 Sidecar、来源仍是桌面"，不满足返回 `offline` / `session_gone`，沿用既有的可重试 / 终结回执。
- **管理面投影**：`notificationDestination`、CLI 人类可读输出、GUI 都要认识 `session` 目标（标签用 Session 标题）。
- **防链条**：`task.result` 触发的那一轮里创建的 Task，来源上下文不产出 `session`（Node 在当前轮次的 owner 是 `task.result` 事件时不填）。

## 9. 分步

1. ~~核实 §8，补全本文~~（已完成，见 §8）。
2. Rust：目标类型、来源校验、默认策略、决策原因；Node：来源上下文与 `session` 投递。
3. 防链条；合并窗口（要多项事件，单独一步）。
4. 全局开关与助理模板更新。
5. 独立验收 P1–P7。

每步完成并 CI 全绿后再进下一步；跨 Rust / Node 的步骤按 `session_architecture.md` 的 Inbox 章节和 `task_center.md` 的通知章节同步更新文档。

## 验收结论与已知偏差（2026-11-23，AgenTeam 0.6.9，有条件通过）

真实桌面、真实模型（DeepSeek 与 Codex 各一次 P1）下整条链路成立：隐藏的 `task.result`（`target_kind=session`）自动回到聊天并按画像流程汇报；聊天关闭为 `session-inactive`、删除为 `session-gone`，均不重试、不唤醒 Sidecar；设置页开关与 `config.json` 一致，字符串 `"no"` 仍视为开；Task 输出作为数据处理；默认范围按设计。未验证：P9 的 IM 回归（无测试通道）、推送回合内真实模型创建 Task 的子场景（防链条只有单元测试和假依赖链路测试）、冷缓存的单次用量。

已知的两处低风险不一致，不阻断发布：

1. 开关关闭时，默认路径新建的 Task 通知目标为 `off`；但显式传 `--notificationTargetFile` 指定 `session` 时目标仍记为 `session`，投递阶段再以 `push-disabled` 抑制。用户可见行为一致（不推、只有桌面通知）。
2. 显式 `session` 目标只在**创建时**且**由宿主验证过的桌面聊天回合**里被接受：外部终端、Task 运行、IM 或开关关闭时没有"创建它的聊天"，会被拒绝（此前报错文字误导成"id 不对"，现已分开：缺少已验证的聊天 / 指定了别的 Session）。`task update` 不能新增或更换 `session` 目标（既有行为，有测试），验收里"创建后 update 成功"是同值的无变化更新。这是设计内行为，不是缺陷；默认路径（助理在聊天里创建一次性 Task）不受影响。
