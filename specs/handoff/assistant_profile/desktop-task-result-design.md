# 设计：桌面聊天的 Task 结果回推（保守版）

> 日期：2026-11-23。状态：**草案，等待用户确认后才开发**。按 `CLAUDE.md`，这是新的通知目标和来源，属于需要先讨论架构的改动。
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

### 3.5 开关

- 创建时单个 Task 可关闭（见 §3.2 显式参数）。
- 全局开关：`config.json` 的 `taskResultPushBack`，默认 `true`。关闭后桌面创建的 Task 一律退回"桌面系统通知"。写入遵守"写盘前重新读取并在锁内合并"。

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

## 8. 需确认的代码事实（开发前先核实）

- 桌面 Session 的 SessionEngine 轮次是否能稳定取到"所属 Session ID 和是否用户可见"（`getNotificationConversation()` 在桌面返回 `null`，需要新增并行的取值方法）。
- `enqueueInboxMessage` 对没有活动 Sidecar 的 Session 的现有行为（是否会隐式拉起），以及"目标 Session 正在回复"时的排队语义。
- `NotificationTarget` 的 `deny_unknown_fields` 与旧版本兼容：未知目标类型使 TaskStore 只读（文档已述），新增 `session` 需要版本号策略，避免旧客户端读到新数据后进入只读。
- 合并窗口放在哪一层（Rust worker 或 Node Inbox）才不引入第二个队列。

## 9. 分步

1. 核实 §8，补全本文。
2. Rust：目标类型、来源校验、默认策略、决策原因；Node：来源上下文与 `session` 投递。
3. 合并窗口与防链条。
4. 全局开关与助理模板更新。
5. 独立验收 P1–P7。

每步完成并 CI 全绿后再进下一步；跨 Rust / Node 的步骤按 `session_architecture.md` 的 Inbox 章节和 `task_center.md` 的通知章节同步更新文档。
