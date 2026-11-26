# Claude 审查意见：阶段 C（分支 fix/composer-draft-preserve @ caf5b09 + 5464f2c）

## 结论：代码批准，进入阶段 D（桌面端实测）；开始实测前先做完 R1–R2 两项小修

Claude 逐行审查了功能提交 `caf5b09` 的 diff，并对照源码核实了下面几个关键时序：

- **C1 正确**。从首页发送时，`targetTabId` 就是当前首页标签页（只有当前标签页是带持久 owner 的 Chat 时才会新开标签页，见 `App.tsx` 约 1853–1866 行）。`flushSync` 内 `replaceMany` 把它原地换成 Chat，首页的 `SimpleChatInput` 在 `flushSync` 内被同步卸载，卸载时的 cleanup 先把文本写回存储；`flushSync` 之后，`onInitialMessageCommitted` 再删除草稿，顺序成立。其他入口不传这个回调，首页草稿不会被误删。
  - `onSubmitted` 也在同一时机被调用，`clearSubmittedDraft` 的"最多清一次"保护因此生效：等 `onLaunchProject` 稍后 resolve 时，已卸载的旧实例不会去误删新首页里写下的新草稿。
- **C2 正确**：`recordingBusy` 和 `handleBrandSend` 里所有没发出去的分支都返回 `false`；独立定时任务创建成功返回 `true`，会正常清空草稿。
- **C3 正确**：`awaitingRestoredWorkspaceCheckRef` 在工作区加载完成之前不会覆盖"保存时的工作区"；比较改用了 `workspacePathsEqual`。
- **C4 正确**：组件测试已经固定了原地切换会话时的行为。
- **删除会话时给 `replaceMany` 加 `flushSync`**：这个额外修复合理，和发送路径是同一个顺序问题；删除失败时草稿会保留（卸载时已写回）。
- **`SimpleChatInput`**：`inputValueRef` 在 render 时赋值，卸载时的 cleanup 读到的是最后一次渲染的值；斜杠指令和 `clearSubmittedDraft` 都先把 ref 置空，再删除存储，没有竞态。
- **改动范围**全部在 renderer 内，没有引入新的进程、IPC 或持久化。

## 进入阶段 D 前必须完成

### R1. Chat 的 draftKey 推导只保留一份实现
`App.composerDraft.test.tsx` 把 Chat 页 mock 成了一个"镜像 Chat.tsx 写法"的外壳，`sessionId && !isPendingSessionId(sessionId) ? createSessionDraftKey(sessionId) : undefined` 这段逻辑被复制了一份。这样一来，真实 `Chat.tsx` 的接线其实没有被 App 级测试覆盖：以后有人改了 Chat.tsx，测试照样会通过。

要求：在 `composerDraftStore.ts` 中新增 `chatComposerDraftKey(sessionId: string | null): string | undefined`（pending / 空值返回 undefined），`Chat.tsx` 和测试外壳都调用它；在 `composerDraftStore.unit.test.ts` 里补上 pending、空值、真实 ID 三种情况的断言。

### R2. 确认依赖检查的 17 条警告是基线就有的
lint 输出里 `depcruise` 有 17 条 warning。在基线 `1f00131` 上跑一次 `npm run lint:deps`，对比数量和列表，确认本次改动没有新增警告，把结论（可以只贴汇总行和差异）写进阶段 D 报告。如果有新增，修掉。

R1 单独提交一次（`test(chat-input): ...` 或 `refactor(chat-input): ...`，要有 body），提交后重跑第 2.2 节的测试命令和 lint、typecheck，把结果贴进阶段 D 报告。

## 已知限制（阶段 D 报告和最终 PR 描述中需要写明，不需要修）
1. 草稿里的图片在页面切换后不会恢复（C6）。
2. 首页发送后，如果在已经切换到 Chat 之后启动失败（Sidecar 失败、回退到首页），已发送的文本不会回到首页输入框。这和修复前的行为一样，不属于回归。
3. 草稿只保存在内存里，应用重启后就没了（按设计如此）。

## 阶段 D：桌面端实测
- 用户已授权安装 CMake 和 VS Build Tools，Gemini 已经装好。按 `package.json` / `specs/guides/` 的说明完成 Windows 开发环境准备，以开发模式启动 Tauri 桌面端。准备过程中如果需要修改仓库配置，或者需要额外授权，先停下来回报。
- 按 `HANDOFF.md` 第 5 节手动验收表逐条实测 M1–M7，每条写 通过 / 失败 / 未测；**M3（历史会话）、M5（发送）、M6（真实会话 Chat）必须在桌面端连接真实 Sidecar 测试**。另外补测两条：
  - **D1**：首页留草稿 → 从侧边栏项目的"在此项目创建新任务"开一个任务 → 回到首页，草稿仍在（C1 的真实环境验证）。
  - **D2**：真实会话的 Chat 留草稿 → 删除这个会话 → 再开一个新会话，不会出现那份草稿。
- 每条在关键节点截图（输入后、切回后），放在 `specs/handoff/composer_draft/stage-d/` 下，并在报告里引用。截图里不能出现密钥、令牌或个人隐私。
- 报告写入 `specs/handoff/composer_draft/stage-d.md`（使用回报模板）。推送后停下，等待审查。**不要合并，不要开 PR。**
