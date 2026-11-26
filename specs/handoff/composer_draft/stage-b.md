# 阶段 B 回报：修复方案设计与桌面端环境核查

## 阶段 B 回报
- **本地 HEAD / 分支**：`ae03d6376`（`docs(handoff): add composer draft handoff index and Stage A review`），分支 `fix/composer-draft-preserve`
- **做了什么**：
  1. 核查 `useAttachmentHandling.ts` 中的图片/附件数据形态、`SimpleChatInput` / `BrandSection` / `Launcher` / `Chat` 的发送与清空竞态、`App.tsx` 中已挂载 Chat 的 `sessionId` 切换路径，以及 `handleDeleteSession` 删除会话入口。
  2. 按 `reviews/stage-a.md` 裁决完成阶段 B 修复方案设计（未修改任何功能代码）。
  3. 安装 Rustup 1.29.3，并执行 `scripts/ensure_rust_toolchain.ps1 -Targets @("x86_64-pc-windows-msvc")` 按 `rust-toolchain.toml` 安装精确版本 Rust `1.92.0`（含 `rustfmt`、`clippy`）；运行原生构建前置依赖检查，确认 Windows 桌面端启动仍缺失的前置工具。
- **结论 / 根因**：见阶段 A 报告与 `reviews/stage-a.md`；本阶段产出完整修复方案与环境核查结论如下。
- **改动文件列表（阶段 C 起）**：本阶段仅新增设计文档 `specs/handoff/composer_draft/stage-b.md`，未改动任何源码。
- **diff 或已推送的分支名**：`fix/composer-draft-preserve`（含本文件提交）
- **测试 / lint / typecheck 命令与结果**：阶段 B 未改功能代码，未重跑测试 / lint / typecheck。
- **手动验收结果**：阶段 B 不适用（待阶段 D 在桌面端执行 M1–M7）。
- **偏离方案之处或不确定点**：见第 7 节「桌面端环境阻塞」与第 8 节「请 Claude 确认的细节」。
- **需要 Claude 裁决的问题**：见第 8 节。

---

## 1. 附件 / 图片数据形态调查结论

核查 `src/renderer/components/chat-input/hooks/useAttachmentHandling.ts` 与 `src/renderer/components/chat-input/types.ts`（`ImageAttachment`）：

1. **`source: 'inline_base64'`**（`addImage`，lines 188–221，来源于粘贴、`<input type="file">`、HTML 拖拽）：
   - 持有内存中的 `File` 与 `preview`（`data:...;base64,...`），自包含，不依赖 `attachmentSessionId`。
2. **`source: 'attachment_ref'`**（`addPreparedImageAttachment`，lines 223–243，来源于 Tauri 原生路径拖拽 `processDroppedFilePaths`，lines 428–443）：
   - 调用 `fileService.prepareUserImageAttachments({ sessionId: attachmentSessionId, paths })`，把图片暂存到对应 `attachmentSessionId` 下并返回 `relativePath`。
   - 在 Launcher 上 `attachmentSessionId = createPendingSessionId(tab.id)`（即 `pending-<旧tabId>`）。单页壳切走后旧 Launcher Tab 被销毁，切回时新 Launcher Tab 拥有全新的 `pending-<新tabId>`，旧 `attachmentSessionId` 下暂存的图片引用在跨 Tab 生命周期后不再安全可用。
3. **`@` 提及与工作区文件引用**：
   - `agentMentions: AgentMentionSnapshot[]`、`primaryContext`、`requiredSystemSkill` 均为纯数据结构（见 `src/shared/agentMentions.ts`），无 DOM / `File` / `attachmentSessionId` 依赖，且 `activeAgentSnapshots(text, agentMentions)` 会自动裁剪已从正文删除的 `@Agent-id:...`。
   - `@myagents_files/...` 文件引用本身就是 `inputValue` 中的普通文本，随正文自动保留，并在工作区不一致恢复时由既有 `clearWorkspaceBoundDraft()` 正则清理。

**结论（按 `reviews/stage-a.md` 附加要求 #1）**：
- **不恢复 `images`**（避免 `attachment_ref` 指向失效的旧 `pending-tab-<oldTabId>`，也避免粘贴图片与拖拽图片表现不一致、以及在 50 条内存 Map 中持有大量 multi-MB base64 数据）。
- **恢复范围**：正文 `text` + `@` 提及上下文（`agentMentions`、`primaryContext`、`requiredSystemSkill`）。

---

## 2. 草稿存储模块设计

### 新增文件：`src/renderer/components/chat-input/composerDraftStore.ts`

纯 renderer 进程内模块（`Map<string, ComposerDraftRecord>`），不写 `localStorage`、不写 `config.json`、不经过 Sidecar、不新增 IPC、不抬升到父组件 React state。

```ts
import type {
  AgentMentionSnapshot,
  QueryMentionContext,
} from '../../../shared/agentMentions';

export interface ComposerDraftRecord {
  readonly text: string;
  readonly agentMentions?: readonly AgentMentionSnapshot[];
  readonly primaryContext?: QueryMentionContext['primaryContext'];
  readonly requiredSystemSkill?: QueryMentionContext['requiredSystemSkill'];
  /** 保存草稿时的工作区路径（Launcher 恢复时用于检测工作区是否变化）。 */
  readonly workspacePath?: string | null;
}

export const LAUNCHER_DRAFT_KEY = 'launcher';
export const MAX_COMPOSER_DRAFTS = 50;

export function createSessionDraftKey(sessionId: string): string {
  return `session:${sessionId}`;
}

export function getComposerDraft(key: string): ComposerDraftRecord | undefined;

/**
 * 写入草稿：
 * - draft.text.length === 0 等同于 deleteComposerDraft(key)
 * - 写入时 delete + set 刷新 LRU 顺序
 * - 超出 MAX_COMPOSER_DRAFTS (50) 时淘汰最旧一条（Map 迭代首项）
 */
export function setComposerDraft(key: string, draft: ComposerDraftRecord): void;

export function deleteComposerDraft(key: string): void;

/** 仅用于测试隔离清理，不暴露给业务逻辑。 */
export function clearAllComposerDrafts(): void;
```

---

## 3. 各页面 Key 规则与挂载期间 `sessionId` 切换核查

| 页面 / 场景 | `draftKey` | 说明 |
|---|---|---|
| Launcher（首页） | `LAUNCHER_DRAFT_KEY` (`'launcher'`) | 单页壳下全局唯一首页；一并记录 `workspacePath`，不恢复 `stagedCron` 与 `selectedWorkspacePath` |
| Chat（已有真实 `sessionId`） | `createSessionDraftKey(sessionId)` (`session:<realSessionId>`) | `sessionId && !isPendingSessionId(sessionId)` |
| Chat（pending 未发送会话） | `undefined` | 用户与审查意见已定：未发送的 pending Chat 不保留草稿 |
| Chat（pending → real birth） | `undefined` → `session:<realSessionId>` | 挂载期间 `draftKey` 升级；卸载时通过 latest ref 读取最新 `draftKey`，自然写入新真实会话 key |
| 未传 `draftKey` 的其他调用方 | `undefined` | 行为与现有完全一致，不读写 `composerDraftStore` |

### 核查发现：挂载期间的 `sessionId` 切换（按审查意见 #3 如实记录，不自行处理）

核查 `src/renderer/App.tsx` 发现已挂载的 `Chat` 组件在不卸载的情况下存在两种非 birth 的 `sessionId` 切换：
1. **真实 ID → 另一个真实 ID**：用户在空闲的 Chat Tab 内点击「新任务」触发原地 reset（`cmd_new_desktop_session`），`App.tsx:1670`（`updateTabSessionId`）将 `tab.sessionId` 从 `realA` 直接更新为 `realB`（`draftKey` 从 `session:realA` 变为 `session:realB`）。
2. **真实 ID → 新 pending ID**：用户在 AI 运行中的 Chat Tab 内点击「新任务」，`App.tsx:3031`（`handleNewSession`）将 `tab.sessionId` 从 `realA` 更新为 `pending-<tabId>-<uuid>`（`draftKey` 从 `session:realA` 变为 `undefined`）。

按 `reviews/stage-a.md` 第 3 条指示：**不改动 `useChatDraftScope` 现有行为，也不在本次方案中自行添加挂载期间 `draftKey` 变更的存取切换逻辑**；如果 Claude 希望处理原地「新任务」时的旧草稿保存/清空，请在审查意见中明确。

---

## 4. 写入 / 恢复 / 清理时机表

| 时机 | 位置 | 行为 |
|---|---|---|
| **挂载初始化** | `SimpleChatInput.tsx` | 当 `externalValue === undefined` 且传入 `draftKey` 时，在 `useState` 惰性初始化中读取 `getComposerDraft(draftKey)`，初始化 `inputValue`、`agentMentions`、`primaryContext`、`requiredSystemSkill`（无闪烁、无额外 effect 重渲染）。 |
| **Launcher 恢复时工作区不同** | `BrandSection.tsx` | 将 `lastWorkspacePathRef` 初始化为 `savedLauncherDraft?.workspacePath ?? selectedProject?.path ?? null`（其中 `savedLauncherDraft` 为挂载时读到的 `getComposerDraft(LAUNCHER_DRAFT_KEY)`）。若当前 `selectedProject.path` 与保存时的 `workspacePath` 不同（用 `workspacePathsEqual` 判断，兼容挂载即不同及 Config 异步加载后才就绪两种情况），**直接复用 `BrandSection` 既有的 `useEffect`（lines 326–344）**：调用 `inputRef.current?.clearWorkspaceBoundDraft()` 去掉 `@myagents_files/...` 引用并弹出同一条 `toasts.workspaceAttachmentsCleared` toast，正文原样保留。不另写第二套清理逻辑。 |
| **按键输入期间** | `SimpleChatInput.tsx` | **不写存储、不触发父组件重渲染**。仅在 `useLayoutEffect` 中同步 `draftKeyRef`、`workspacePathRef`、`agentMentionsRef`、`primaryContextRef`、`requiredSystemSkillRef`（遵循 `specs/tech_docs/react_stability_rules.md` Latest Ref 规范）。 |
| **组件卸载** | `SimpleChatInput.tsx` | 在 mount-only `useLayoutEffect(() => () => { ... }, [])` cleanup 中：若 `draftKeyRef.current` 非空，读取最新 `inputValueRef.current` 与各 context ref（经 `activeAgentSnapshots` 裁剪）调用 `setComposerDraft(key, record)`（空串自动删除）。满足 `StrictMode` setup → cleanup → setup 幂等性。 |
| **发送成功（含竞态防护）** | `SimpleChatInput.tsx`、`BrandSection.tsx`、`Launcher.tsx`、`App.tsx` | 1. `SimpleChatInput::clearSubmittedDraft()`：当 `inputValueRef.current === inputValue` 时，**同步更新 `inputValueRef.current = ''`**（防止同帧 `flushSync` 卸载把旧文本写回）并调用 `deleteComposerDraft(draftKeyRef.current)`。<br>2. **修复 Launcher 发送竞态**：当前 `BrandSection::handleSend` 未传递 `onSubmitted` 也未返回 Promise，`Launcher::handleBrandSend` 校验失败时返回 `undefined`（非 `false`）且调用 `onLaunchProject` 未 `return`；而 `App::handleLaunchProject` 在 `await ensureSessionSidecar` 之前会 `flushSync` 把 Launcher 替换为 Chat 并卸载 `SimpleChatInput`。修改如下：<br>&nbsp;&nbsp;- `BrandSection::handleSend` 透传 `onSubmitted` 并 `return onSend(...)`；<br>&nbsp;&nbsp;- `Launcher::handleBrandSend` 在校验失败 / 独立定时任务创建异常时返回 `false`（失败保留草稿），正常启动时 `return onLaunchProject(...)`；<br>&nbsp;&nbsp;- `App::handleLaunchProject` 在 `flushSync` 将 Launcher 翻转为 Chat 且携带 `initialMessage` 后立即调用 `deleteComposerDraft(LAUNCHER_DRAFT_KEY)`（消除 `ensureSessionSidecar` 等待窗口内若切回首页导致已发草稿短暂复活的竞态）。 |
| **发送失败** | `SimpleChatInput.tsx`、`Launcher.tsx`、`Chat.tsx` | `onSend` 返回 `false` → 不调用 `clearSubmittedDraft()` → 输入框正文与草稿存储均保留。 |
| **删除会话** | `App.tsx` (`handleDeleteSession`) | `deleteSessionThroughAppOwner(...)` 返回 `result.deleted === true` 后调用 `deleteComposerDraft(createSessionDraftKey(sessionId))`。仅在 Rust 确认删除成功后清理，避免删除被拒绝时误删草稿。 |
| **斜杠本地客户端指令（如 `/goal`）** | `SimpleChatInput.tsx` | 本地客户端指令清空输入框时同步更新 `inputValueRef.current = ''` 并 `deleteComposerDraft(draftKeyRef.current)`，避免残留 `/goal`。 |

---

## 5. 改动文件清单（阶段 C 计划）

1. **新增** `src/renderer/components/chat-input/composerDraftStore.ts` — renderer 进程内 Map 草稿存储（`get / set / delete / clearAll`，空串即 delete，上限 50 条 LRU 淘汰，`LAUNCHER_DRAFT_KEY`、`createSessionDraftKey`）。
2. **修改** `src/renderer/components/chat-input/types.ts` — `SimpleChatInputProps` 新增可选字段 `draftKey?: string`。
3. **修改** `src/renderer/components/chat-input/SimpleChatInput.tsx` — 挂载从存储初始化、Latest Ref 同步、卸载 `useLayoutEffect` cleanup 写回、`clearSubmittedDraft` 同步清 ref 与删存储。
4. **修改** `src/renderer/components/launcher/BrandSection.tsx` — 传入 `draftKey={LAUNCHER_DRAFT_KEY}`；初始化 `lastWorkspacePathRef` 复用既有 M7 工作区变更清理与 toast；`handleSend` 透传 `onSubmitted` 并返回 `onSend` 结果。
5. **修改** `src/renderer/pages/Launcher.tsx` — `handleBrandSend` 校验/异常路径返回 `false`，正常启动 `return onLaunchProject(...)`。
6. **修改** `src/renderer/pages/Chat.tsx` — 对真实 `sessionId`（`!isPendingSessionId(sessionId)`）传入 `draftKey={createSessionDraftKey(sessionId)}`，pending 状态传 `undefined`。
7. **修改** `src/renderer/App.tsx` — `handleLaunchProject` 在 Launcher→Chat `flushSync` 接管 `initialMessage` 后调用 `deleteComposerDraft(LAUNCHER_DRAFT_KEY)`；`handleDeleteSession` 在 `result.deleted === true` 后调用 `deleteComposerDraft(createSessionDraftKey(sessionId))`。

*(注：改动范围全部在 renderer 的输入框 / 首页 / Chat / App 内，不涉及 Rust、Sidecar、IPC 或 config.json。)*

---

## 6. 准备新增的回归测试列表（阶段 C 计划）

1. **单元测试**：`src/renderer/components/chat-input/composerDraftStore.unit.test.ts`
   - `get / set / delete` 基本读写
   - 空字符串写入等同于 `delete`
   - 超过 50 条上限时按 LRU 顺序淘汰最旧条目，重新写入已存在 key 会刷新顺序
2. **组件 / 首页 DOM 测试**：`src/renderer/components/SimpleChatInput.draft.test.tsx`
   - 传入 `draftKey` 时：输入正文与 `@` 提及 → 卸载 → 重新挂载 → 正文与 `@` 提及恢复
   - 未传 `draftKey` 时：卸载再挂载输入框为空（原有行为不变）
   - 两个不同 `draftKey` 的 `SimpleChatInput` 实例草稿互不串（覆盖 Launcher vs Chat、Session A vs Session B）
   - 发送成功（同步 `onSubmitted` / 异步 resolve `true`）清空草稿，卸载再挂载不复活
   - 发送失败（返回 `false`）草稿保留，卸载再挂载仍在
   - Launcher（`BrandSection`）恢复时工作区与保存时不同：正文保留，`@myagents_files/...` 引用被清理并触发 `toasts.workspaceAttachmentsCleared`（同工作区恢复则保留引用、不弹 toast）
3. **App 级集成 DOM 测试**：`src/renderer/App.composerDraft.test.tsx`
   - **修复前必失败 / 修复后必通过**：首页输入 `"打算找个人"` → 打开侧边栏其他页面（如智能体团队 / 设置，触发单页壳 `requestCloseTab(..., 'replaced')` 卸载旧首页）→ 点「创建新任务」新建首页（新 `tab.id`）→ 草稿 `"打算找个人"` 恢复
   - **未发送 pending Chat 锁定行为**：通过侧边栏项目「在此项目创建新任务」打开 pending Chat → 输入草稿 → 切去设置 → 再点同一项目「在此项目创建新任务」→ 输入框为空
   - **真实 Chat 会话**：打开真实 `sessionId` 的 Chat → 输入草稿 → 切去设置 → 重新打开同一真实会话 → 草稿恢复；两个不同真实会话之间草稿互不串
   - **pending → real birth**：pending Chat 输入草稿 → 期间 `sessionId` 升级为真实 ID → 切去设置再打开该真实会话 → 草稿保留
   - **Launcher 发送后竞态**：首页输入 → 发送（`flushSync` 替换为 Chat）→ 点「创建新任务」回首页 → 输入框为空，已发文本不复活
   - **删除会话清理**：真实 Chat 留有草稿 → 触发 `handleDeleteSession` 删除成功 → 对应 `session:<id>` 草稿被清除

---

## 7. Windows 桌面端开发环境核查结果（供阶段 D 评估）

已按要求完成 Rust 工具链安装（严格遵循 `rust-toolchain.toml`）：
```text
rustc 1.92.0 (ded5c06cf 2025-12-08)
cargo 1.92.0 (344c4567c 2025-10-21)
rustfmt 1.8.0-stable (ded5c06cf2 2025-12-08)
clippy 0.1.92 (ded5c06cf2 2025-12-08)
target: x86_64-pc-windows-msvc
```

随后执行 `setup_windows.ps1` 中的前置检查命令：
```text
node scripts/prepare-native-inference.mjs x86_64-pc-windows-msvc --check-prerequisites
```
返回报错原文：
```text
Error: Speech-inference build prerequisites are missing for x86_64-pc-windows-msvc:
- CMake >= 3.28.0 (install from https://cmake.org/download/)
- MSVC C++ Build Tools (run from a Developer PowerShell)
Install the missing tools and rerun the same command.
```

说明：
- 本机当前已就绪：Node.js v24.22.3、npm 11.19.1、`node_modules`、Rustup 1.29.3、Rust `1.92.0-x86_64-pc-windows-msvc`。
- 本机尚缺失（导致目前无法直接启动 `npm run tauri:dev`）：
  1. **CMake >= 3.28.0**（可用 `winget install Kitware.CMake` 安装）；
  2. **Visual Studio C++ Build Tools + Windows SDK**（`vswhere.exe` 未安装，安装包数 GB，需要用户授权与管理员 UAC 确认；此外之后还需运行 `setup_windows.ps1` 下载内置 Node、CLIProxy、原生推理与 DSH 资源）。
- 未做任何仓库配置修改。

---

## 8. 请 Claude 确认的问题

1. **方案整体与 API 签名**：第 1–6 节的改动文件清单、store API（仅恢复 `text` + `@` 提及上下文、不恢复 `images`）、Launcher 工作区不一致时复用 `BrandSection` 既有 `lastWorkspacePathRef` + `clearWorkspaceBoundDraft()` 机制、以及 Launcher 发送 `flushSync` 竞态清理方案是否批准？
2. **挂载期间 `real → real` / `real → pending` 切换**：如第 3 节记录，Chat 内点击「新任务」会在不卸载组件的情况下把 `sessionId` 从 `realA` 改成 `realB` 或 `pending-*`。按审查意见当前不做额外处理，请确认维持不变还是需要额外处理？
3. **阶段 D 桌面端环境准备**：本机已装好 Rust 1.92.0，但启动 `npm run tauri:dev` 还需要安装 CMake 与 Visual Studio C++ Build Tools（数 GB，需用户 UAC 授权）并执行 `setup_windows.ps1`。请确认阶段 C 审查通过后（或现在经用户同意后）是否继续在本机安装 CMake + VS Build Tools 完成完整 `setup_windows.ps1` + `npm run tauri:dev`。
