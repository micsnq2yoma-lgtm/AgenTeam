# 阶段 A 回报：对话输入框未发送草稿在页面切换后丢失

- **本地 HEAD / 分支**：基线 `1f0013139` (`fix(kb): list only Agents with an active Project as curator choices`)，工作分支 `fix/composer-draft-preserve`
- **状态**：阶段 A 复现与根因定位完成（已触发交接文档第 7 节停下回报条件，等待 Claude 审查与裁决后进入阶段 B）

## 1. 做了什么

1. 在 `C:\Users\fanxi\.gemini\antigravity\scratch\AgenTeam` 检出基线 `1f0013139`（工作区干净）。
2. 安装 Node.js v24.22.3（npm 11.19.1）并完成 `npm install`。
3. 通读 `CLAUDE.md`、`specs/tech_docs/react_stability_rules.md`、`specs/tech_docs/session_architecture.md`。
4. 在干净基线上运行 4 个相关 DOM 测试文件确认基线全绿（4 files, 91 tests passed）。
5. 在 `App.tsx`、`useTabWorkspaceController.ts`、`features/launcher/tabModule.tsx`、`Launcher.tsx`、`Chat.tsx`、`SimpleChatInput.tsx` 仅加入临时 `[DRAFT-DEBUG]` 日志（未改业务逻辑），启动 `npm run dev:web`（`http://localhost:5173/`）在浏览器中逐条复现各侧边栏切换路径并采集关键日志；随后已还原全部临时日志以保持分支干净。

## 2. 结论与根因

### 2.1 性质判定：组件被卸载（而非值被主动清空）

- 所有清空路径（`externalValue` sync、`clearWorkspaceBoundDraft`、`clearQueryContext`、`clearSubmittedDraft`、`setValue`、slash action）在页面切换过程中**均未触发**。
- `SimpleChatInput` 卸载瞬间 `inputValueRef.current` 仍完整持有用户输入（`"打算找个人"` / `"聊天页草稿"`）。

### 2.2 真正根因（纠正交接文档第 3 节第 2 条的静态判断）

- 交接文档第 3 节原假设：“正常导航不会卸载首页……非当前标签页由 `BuiltinTabSlot.tsx` 用 `invisible` + `contentVisibility:hidden` 隐藏，组件仍然挂载。”
- **实际代码与运行事实**：提交 `aeb2c1810`（`feat(shell): replace the tab strip with a single-page shell`）已将应用改造为**单页壳**。`src/renderer/App.tsx:893-903` 存在主动关闭非活跃 Tab 的 effect：

```tsx
// Single-page shell: there is no tab strip, so the visible page is the only
// page. When another page becomes active, every other tab leaves through the
// normal close lifecycle (file flush, recording guard, Session release or
// background completion). Keyed on the active id only: launch/fork flows
// append an inactive tab, prepare it, then activate it.
useEffect(() => {
  const snapshot = tabWorkspaceController.getSnapshot();
  for (const tab of snapshot.tabs) {
    if (tab.id !== snapshot.activeTabId) requestCloseTab(tab.id, 'replaced');
  }
}, [activeTabId, requestCloseTab, tabWorkspaceController]);
```

- 每次点击侧边栏任意页面（智能体团队 / 自动化任务 / 技能与工具 / 聊天机器人 / 设置 / 历史任务 / 项目「在此项目创建新任务」），`activeTabId` 切换后该 effect 立即对所有非当前 Tab 调用 `requestCloseTab(tab.id, 'replaced')` → `detach`，将旧的 `Launcher` 或 `Chat` Tab 从 `tabs` 中移除并彻底卸载组件（DOM 实测 `[data-tab-content-workspace] > div` 始终只有 `1` 个 slot；仅正在录音的 `record` Tab 会被 close guard 留在后台）。

### 2.3 触发路径逐条实测

**必现丢失的路径：**

1. **首页（Launcher）→ 智能体团队 / 自动化任务 / 技能与工具 / 聊天机器人 / 设置 → 点「创建新任务」切回**：
   - 切走时 `SinglePageShell` 关闭旧 Launcher Tab（`tab-...-s3eto8`），`Launcher` 和 `SimpleChatInput` 卸载。
   - 切回点「创建新任务」进入 `handleSidebarNewChat`，此时 `currentTabs` 已无 `launcher` Tab（`leftmostLauncherId: null`），走 `handleNewTab()` 新建了**全新 `tab.id`**（`tab-...-3p7xcl`）的 Launcher，新挂载的 `SimpleChatInput` 初始值为 `""`。
2. **Chat 页（通过项目悬停「在此项目创建新任务」打开的未发送 pending Chat）→ 设置 → 再点同一项目「在此项目创建新任务」**：
   - 切走时 `SinglePageShell` 关闭旧 pending Chat Tab（`tab-...-o8rwm3`），该未发送会话不在侧边栏历史列表中。
   - 再点该项目「在此项目创建新任务」走 `handleOpenWorkspaceFromSidebar`，创建全新 `tab.id`（`tab-...-e17t5d`）并 flip 为新 Chat Tab，草稿丢失。
3. **Chat 页（已有真实 `sessionId` 的历史会话）→ 其他页面 → 再点侧边栏同一历史会话**：
   - 浏览器开发模式无 Sidecar（`GET /sessions` 返回 500），未能直接在浏览器点击历史行；但代码路径一致：切走时 `SinglePageShell` 关闭并卸载该 Chat Tab，切回时 `handleOpenSidebarSession` → `spawnTabForExistingSession` 新建 Chat Tab（新 `tab.id`，但沿用**同一个真实 `sessionId`**），因无草稿存储，重挂载后为空。

**已实测排除（未丢失）的路径：**

- 首页「对话」↔「知识库」（`LauncherModeSwitch`）：同一 `Launcher` 实例内 CSS `hidden` 切换，草稿保留。
- 首页输入框下拉切换工作区（`Mino` → `Beta`）：`clearWorkspaceBoundDraft` 不清空普通正文，草稿保留。
- 点「通知」、Chat 页点「Logs」/「展开项目」、在首页重复点「创建新任务」：未触发 unmount，草稿保留。

## 3. 关键日志原文（来自 `http://localhost:5173/` 实时运行）

### Step 1：首页初始挂载

```text
[DRAFT-DEBUG] SimpleChatInput MOUNT {"mode":"launcher","sessionId":"pending-tab-1795710388546-s3eto8","workspacePath":"/mock/Mino","initialValue":""}
[DRAFT-DEBUG] TabWorkspace commit {"prevActive":"tab-1795710388546-s3eto8","nextActive":"tab-1795710388546-s3eto8","prevTabs":["tab-1795710388546-s3eto8:launcher"],"nextTabs":["tab-1795710388546-s3eto8:launcher"],"prevDeferred":[],"nextDeferred":[]}
[DRAFT-DEBUG] Launcher MOUNT {"attachmentSessionId":"pending-tab-1795710388546-s3eto8"}
```

### Step 2–3：首页输入 `"打算找个人"`（无清空日志）→ 点击侧边栏「智能体团队」

```text
[DRAFT-DEBUG] TabWorkspace commit {"prevActive":"tab-1795710388546-s3eto8","nextActive":"tab-1795710406697-21a8lh","prevTabs":["tab-1795710388546-s3eto8:launcher"],"nextTabs":["tab-1795710388546-s3eto8:launcher","tab-1795710406697-21a8lh:teamoffice"],"prevDeferred":[],"nextDeferred":["tab-1795710406697-21a8lh"]}
[DRAFT-DEBUG] SinglePageShell closing non-active tab {"closingTabId":"tab-1795710388546-s3eto8","closingView":"launcher","activeTabId":"tab-1795710406697-21a8lh","reason":"replaced"}
[DRAFT-DEBUG] TabWorkspace commit {"prevActive":"tab-1795710406697-21a8lh","nextActive":"tab-1795710406697-21a8lh","prevTabs":["tab-1795710388546-s3eto8:launcher","tab-1795710406697-21a8lh:teamoffice"],"nextTabs":["tab-1795710406697-21a8lh:teamoffice"],"prevDeferred":["tab-1795710406697-21a8lh"],"nextDeferred":["tab-1795710406697-21a8lh"]}
[DRAFT-DEBUG] Launcher UNMOUNT {"attachmentSessionId":"pending-tab-1795710388546-s3eto8"}
[DRAFT-DEBUG] SimpleChatInput UNMOUNT {"mode":"launcher","sessionId":"pending-tab-1795710388546-s3eto8","workspacePath":"/mock/Mino","lastValue":"打算找个人"}
```

### Step 4：点击侧边栏「创建新任务」切回首页（textarea 值变为 `""`）

```text
[DRAFT-DEBUG] App.handleSidebarNewChat {"tabs":["tab-1795710406697-21a8lh:teamoffice"],"leftmostLauncherId":null}
[DRAFT-DEBUG] App.handleNewTab {"currentLength":1}
[DRAFT-DEBUG] TabWorkspace commit {"prevActive":"tab-1795710406697-21a8lh","nextActive":"tab-1795710414076-3p7xcl","prevTabs":["tab-1795710406697-21a8lh:teamoffice"],"nextTabs":["tab-1795710406697-21a8lh:teamoffice","tab-1795710414076-3p7xcl:launcher"],"prevDeferred":[],"nextDeferred":["tab-1795710414076-3p7xcl"]}
[DRAFT-DEBUG] SinglePageShell closing non-active tab {"closingTabId":"tab-1795710406697-21a8lh","closingView":"teamoffice","activeTabId":"tab-1795710414076-3p7xcl","reason":"replaced"}
[DRAFT-DEBUG] SimpleChatInput MOUNT {"mode":"launcher","sessionId":"pending-tab-1795710414076-3p7xcl","workspacePath":"/mock/Mino","initialValue":""}
[DRAFT-DEBUG] Launcher MOUNT {"attachmentSessionId":"pending-tab-1795710414076-3p7xcl"}
```

### Step 6–7：Chat 页输入 `"聊天页草稿"` → 点击侧边栏「设置」

```text
[DRAFT-DEBUG] TabWorkspace commit {"prevActive":"tab-1795710421421-o8rwm3","nextActive":"tab-1795710436203-skr28r","prevTabs":["tab-1795710421421-o8rwm3:chat"],"nextTabs":["tab-1795710421421-o8rwm3:chat","tab-1795710436203-skr28r:settings"],"prevDeferred":[],"nextDeferred":["tab-1795710436203-skr28r"]}
[DRAFT-DEBUG] SinglePageShell closing non-active tab {"closingTabId":"tab-1795710421421-o8rwm3","closingView":"chat","activeTabId":"tab-1795710436203-skr28r","reason":"replaced"}
[DRAFT-DEBUG] Chat UNMOUNT {"sessionId":"pending-tab-1795710421421-o8rwm3","agentDir":"/mock/Mino"}
[DRAFT-DEBUG] SimpleChatInput UNMOUNT {"mode":"chat","sessionId":"pending-tab-1795710421421-o8rwm3","workspacePath":"/mock/Mino","lastValue":"聊天页草稿"}
```

### Step 8：再次点击 `Mino`「在此项目创建新任务」（新建 `tab-1795710448026-e17t5d`，textarea 值变为 `""`）

```text
[DRAFT-DEBUG] SimpleChatInput MOUNT {"mode":"chat","sessionId":"pending-tab-1795710448026-e17t5d","workspacePath":"/mock/Mino","initialValue":""}
[DRAFT-DEBUG] Chat MOUNT {"sessionId":"pending-tab-1795710448026-e17t5d","agentDir":"/mock/Mino"}
```

## 4. 基线测试结果（干净基线）

命令：
```text
npx.cmd vitest run --project dom src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx src/renderer/components/SimpleChatInput.send.test.tsx src/renderer/hooks/useChatDraftScope.test.tsx src/renderer/tab-workspace/useTabWorkspaceController.test.tsx
```

输出原文：
```text
 ✓  dom  src/renderer/hooks/useChatDraftScope.test.tsx (1 test) 16ms
 ✓  dom  src/renderer/tab-workspace/useTabWorkspaceController.test.tsx (5 tests) 25ms
 ✓  dom  src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx (7 tests)
 ✓  dom  src/renderer/components/SimpleChatInput.send.test.tsx (78 tests) 7900ms

 Test Files  4 passed (4)
      Tests  91 passed (91)
```

`lint` / `typecheck`：阶段 A 未测。

## 5. 需要 Claude 裁决的问题（触发第 7 节第 1 条）

1. **架构方向（卸载 + 草稿存储 vs 保留 Launcher 挂载）**：
   - `App.tsx:898-903` 的单页壳主动关闭非活跃 Tab 是有意设计（用于释放 Chat Sidecar / 交接 BackgroundCompletion，且 commit `aeb2c1810` 的 App 集成测试断言单页结果）。
   - 请裁决：**保持单页壳卸载行为不变，统一引入 renderer 内存草稿存储（Map）**，还是将 Launcher 例外排除在 `replaced` 关闭外 + 仅对 Chat 使用草稿存储？
2. **Launcher 草稿的 Owner Key（原第 4 节方案与单页壳冲突）**：
   - 原第 4 节建议首页草稿按 `tab.id`（`createPendingSessionId(tab.id)`）区分。但单页壳切走即销毁旧 Launcher Tab，切回时 `handleSidebarNewChat` 创建全新 `tab.id`，按 `tab.id` 存会 **100% miss**。
   - 请裁决 Launcher key：
     - **方案 A（推荐）**：单页壳下只有一个首页，使用固定 scope key（如 `'launcher'`），与 Chat 的 `session:<sessionId>` 隔离，同时完整保留 M7（切换工作区保留正文、仅清工作区绑定引用/图片）的语义。
     - **方案 B**：按工作区区分（`launcher:<workspacePath>`），但这会改变 M7（切换工作区会切到另一工作区草稿）。
3. **未发送的 pending Chat（从项目「在此项目创建新任务」进入、未发送即切走）的边界**：
   - 请确认 M6 是**仅覆盖已有真实 `sessionId` 的 Chat 会话（含 pending→real birth）**，还是也要覆盖未发送的空 pending Chat（以及若覆盖，如何与“用户明确想在该项目新开空白任务”区分）。
4. **第 5 节验收测试 #2 在单页壳下的对应口径**：
   - 确认测试 #2 调整为验证：**Launcher 与 Chat 互不串 + 两个不同 `sessionId` 的 Chat 会话互不串 + 不同 `draftKey` 的两个 `SimpleChatInput` 实例互不串**。
5. **阶段 D 真实桌面 App 手测环境**：
   - 本机目前仅安装 Node.js v24（未安装 Rust / Cargo），可完整执行 `dev:web` 和全部前端自动化测试/lint/typecheck；阶段 D 是否需要继续安装 Rust 工具链构建 Tauri 桌面端，还是由用户/协作者在真实桌面端补测涉及 Sidecar 的 M3/M5/M6 项？
