# 侧栏任务状态提示：开发与验收说明

> 日期：2026-11-21。状态：用户已确认方向，待开发。开发和验收由 Codex 协作者负责，规则同 `specs/handoff/knowledge_base/README.md` 的「角色变化」一节（开发与验收分开会话；结论写入本目录 `acceptance.md`；合并需用户授权）。
> 可视化方案（用户选定 C）：https://claude.ai/artifact/BFFTDT1d8bxGgt6S73FxSW

## 1. 问题

左侧栏的"运行中、未读"读自当前打开的 Tab（`GlobalSidebar.tsx` 中 `SessionRow` 的 `tab.isGenerating` / `tab.hasUnread`）。应用同一时间只打开一个页面，切到"新建任务"等页面时原任务的 Tab 按 `replaced` 关闭，状态随之消失；任务本身仍由 `BackgroundCompletion` 在后台保活。用户因此不知道离开的任务在等自己批准。

## 2. 用户已确认的决策

| 项 | 决定 |
|---|---|
| 显示样式 | **方案 C 整行强调** |
| 顶部"待处理 (N)"入口 | **不做** |
| 系统通知（需要处理时） | **暂不做** |
| 状态集合 | 需要你处理、运行中、已完成未查看、出错/中断、空闲 |
| 已完成未查看 | 复用现有未读通知计数（`sessionNotificationBadgeCounts`），不新建未读状态 |
| 60 分钟超时 | 已单独修复：`fix/bg-completion-human-wait`（`f4eaffea`），等待人工期间不计入后台保活预算，`/api/session-state` 新增 `waitingForUser`。本任务基于该分支开发 |

## 3. 架构：Rust 持有全局会话活动状态

这是一个**新的状态 owner**（用户已在讨论中选择此方向）。如实现中发现与现有 owner 冲突，或需要新增通信模式之外的东西，停下来向用户说明。

| 事实 | Owner | 说明 |
|---|---|---|
| 会话当前活动（运行中 / 等待人工 / 最近一次终态出错） | Rust `SessionActivityRegistry`（新增，App 级 managed state，与 SidecarManager 同层） | 按 `sessionId` 保存，带 Sidecar process generation；旧 generation 的上报一律丢弃；Sidecar 停止或被替换时清除该会话的运行类状态 |
| 原始事实 | 各 Runtime adapter 的 `getLiveSessionState()`（已有 `isBusy`、`waitingForUser`）与完成终态（`SessionCompletionStatus::{Complete, Stopped, Error}`） | Registry 只投影，不改写 Session 状态 |
| 已完成未查看 | 现有通知 owner（`notification.rs`） | 不重复建 |
| 侧栏展示 | Renderer app-global store（只读投影） | 不在前端另存权威状态；当前打开的 Tab 不再是状态来源 |

数据流：

```text
Runtime adapter 状态变化（开始、等待人工、恢复、结束、出错）
  → Session Sidecar 经现有 Node→Rust Management API 上报（新增路由，如 POST /api/session/activity）
      携带 sessionId 与调用方 generation 由 Rust 从请求身份解析，不信任 body 自报
  → SessionActivityRegistry 锁内比较 generation 后更新
  → Tauri 事件 session:activity-changed { sessionId, activity }
  → Renderer store → GlobalSidebar SessionRow / 项目行
```

要求：

- **推送为主**：状态在 adapter 层变化时立即上报，不依赖 2 秒轮询。上报失败只记日志，不影响会话执行。
- **启动对账**：Renderer 先注册事件监听，再调用快照命令（如 `cmd_session_activity_snapshot`）建立基线，避免启动窗口丢事件（参照 `session_metadata.rs` 的 observer ready + 权威对账做法）。
- **覆盖所有入口**：桌面对话、IM、定时任务、Goal、Companion 的会话都要上报，因为它们都经 SessionEngine。上报点放在 SessionEngine facade 或 adapter 公共路径，不在各入口重复。
- **出错状态的生命周期**：最近一次终态为 `Error` 时显示出错，直到用户打开该会话或新一轮开始；仅保存在内存，App 重启后清除（在 tech doc 写明）。`Stopped`（用户主动停止）不算出错。
- **事件载荷**：只含 sessionId 与状态枚举，不含正文、路径之外的隐私数据或密钥。
- **新增 SSE 事件**不需要（走 Tauri 事件）；如果实现中确实新增了 SSE JSON 事件，必须加入 `src/renderer/api/SseConnection.ts` 白名单。

## 4. 显示规则（方案 C）

优先级：**需要你处理 > 出错 > 运行中 > 已完成未查看 > 空闲**，一个会话只显示最高的一种。

| 状态 | 会话行 |
|---|---|
| 需要你处理 | 整行浅暖色底（`--accent-warm-subtle` 一类 token）+ 左侧 2px 暖色细条 + 行尾"待确认"标签（替换时间）+ 标题加粗；标题前暖色圆点带轻微扩散动画 |
| 出错 / 中断 | 行尾警告图标替换时间（`--warning` 语义色），悬停提示"上次运行出错" |
| 运行中 | 标题前小圆点缓慢呼吸（运行中语义色），时间保留 |
| 已完成未查看 | 沿用现有未读通知计数的显示 |
| 空闲 | 与现状一致，只显示时间 |

项目行（折叠时）：有待确认时显示暖色圆点 + 数量；无待确认但有运行中时显示运行中圆点；展开时项目行不显示汇总。

其他要求：

- 当前选中的会话行与"待确认"底色叠加时保持可读，选中态优先表达"当前位置"，待确认细条与标签保留。
- 每种状态提供 `aria-label` / 悬停文字，不只靠颜色；动画遵守 `prefers-reduced-motion`。
- 文案全部走 i18n（zh-CN / en-US）。
- 颜色、字号使用 Theme token 和 DESIGN.md 的字阶，不新增 `text-[Npx]`；浅色与深色主题都要验收。
- 点击会话行照常打开会话；打开后"待确认"卡片自然可见（现有行为），不需要额外跳转逻辑。

## 5. 验收项

`[自动]` 必须有测试；`[审查]` 读代码确认；`[手动]` 真实 App 中验证并截图；`🔒` 需编写对抗探针。

- AC-S-1 `[自动]` Registry：开始 → 等待人工 → 恢复运行 → 完成/出错/停止 的状态投影正确；`Stopped` 不显示出错。
- AC-S-2 `[自动]` 🔒 旧 generation 的迟到上报被丢弃，不覆盖新 generation 的状态；Sidecar 停止后运行类状态清除。
- AC-S-3 `[自动]` 🔒 上报接口不接受 body 自报的 sessionId 越权更新其他会话；外部 token 调用方无权上报。
- AC-S-4 `[自动]` 启动对账：先监听后快照；快照与事件交错时最终状态正确。
- AC-S-5 `[自动]` DOM：五种状态的会话行渲染、优先级、项目折叠汇总、选中态叠加、`aria-label`。
- AC-S-6 `[自动]` 关闭 Tab（`replaced`）后，侧栏状态仍由 Registry 驱动保持正确。
- AC-S-7 `[审查]` 上报点只在 SessionEngine facade / adapter 公共路径；Route handler 没有 Runtime 分支；Renderer 没有第二份权威状态；控制面不绕过 Rust。
- AC-S-8 `[手动]` 核心场景：发起一个会触发 AskUserQuestion 或权限确认的任务 → 切到"新建任务" → 侧栏该任务立即显示"待确认"整行强调 → 项目折叠时显示数量 → 回到该任务完成选择 → 状态变为运行中 → 结束后变为已完成未查看（未读计数）或空闲。浅色、深色主题各截图。
- AC-S-9 `[手动]` 出错场景：制造一次失败（例如断网或无效 Provider）→ 侧栏显示出错图标 → 打开会话后清除。
- AC-S-10 `[手动]` IM 或定时任务触发的会话在侧栏同样显示运行中 / 待确认。
- 全局 D1–D9、红线与 clippy 基线比对规则同 `specs/handoff/knowledge_base/tasks.md`（D4 按基线比对：新增告警 0、改动模块 0 告警）。

## 6. 文档

- 新增或更新 `specs/tech_docs/session_architecture.md`：SessionActivityRegistry 的 owner、generation 规则、上报路径、出错状态生命周期。
- 更新 `specs/ARCHITECTURE.md` 的持久化/owner 表与模块地图（新增 App 级状态 owner）。
- 更新 `specs/DESIGN.md` 或对应设计子文档中侧栏会话行的状态规范（方案 C）。
