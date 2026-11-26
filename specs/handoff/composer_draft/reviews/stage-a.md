# Claude 审查意见：阶段 A（分支 fix/composer-draft-preserve @ a1e610a）

## 结论：阶段 A 通过，可以进入阶段 B（先交方案，不写代码）

Claude 已核实以下几点：
- `src/renderer/App.tsx:893-903` 确实有一个 effect：`activeTabId` 一变化，就对所有非当前 Tab 调用 `requestCloseTab(tab.id, 'replaced')`。交接文档第 3 节第 2 条"正常导航不会卸载首页"的判断是错的，纠正正确。
- 关闭操作会走 `createChatTabLifecycle`（释放 Session / BackgroundCompletion），launcher 没有关闭钩子，所以首页会被直接卸载。
- 根因定性为"组件被卸载，而非值被清空"，有日志支撑，Claude 认可。
- 报告的问题：
  - 第 2.3 节路径 3（已有真实 sessionId 的历史会话）是从代码推断的，没有实测。阶段 D 必须在桌面端实测。
  - `aeb2c1810` 这个提交在 Claude 的浅克隆里查不到，不影响结论。

## 对第 5 节问题的裁决

### 1. 架构方向：保留单页壳的卸载行为，引入 renderer 内存草稿存储
- 单页壳关闭非当前 Tab 是 Session / Sidecar 生命周期的 owner 决策，**不要**为 Launcher 开例外。开例外会产生用户看不见、但仍然挂载的隐藏标签页，和单页壳的设计相矛盾。
- 既然卸载是合理的，草稿就必须由一个比组件活得更久的地方持有。新增模块：`src/renderer/components/chat-input/composerDraftStore.ts`，用 Map 加 `get / set / delete`。空字符串等同于 delete。设置一个简单的条目上限（例如 50 条，超出时淘汰最旧的），防止无限增长。
- `SimpleChatInput` 新增可选 prop `draftKey?: string`：
  - 挂载时，如果存储中有该 key 的草稿，就用它初始化 inputValue（以及第 5 点允许恢复的部分）。
  - 卸载时，在 `useLayoutEffect` 的 cleanup 中，用**最新的** draftKey 和 inputValueRef 写回存储。
  - 未传 draftKey 时，行为完全不变。
  - 不能每次按键都写存储并触发父组件重渲染。
- key 由页面层决定，SimpleChatInput 不负责推断 key。

### 2. Launcher 的 key：采用方案 A，使用固定 key `'launcher'`
- 存储时一并记录 `workspacePath`。恢复时如果当前工作区和记录的不同，先恢复草稿，再执行与 `clearWorkspaceBoundDraft()` 相同的清理逻辑（只去掉 `@myagents_files/...` 引用和图片，保留正文，弹出相同的 toast），从而复用 M7 的语义，不要另写一套清理逻辑。
- 不恢复 `stagedCron`（定时任务暂存），也不恢复所选工作区（那属于 tab 的 `launcherWorkspacePath` 状态）。只记录观察结果，不在本任务里改。

### 3. 未发送的 pending Chat：不在本次范围内
- M6 只覆盖有真实 sessionId 的 Chat，key 为 `session:<realSessionId>`，包括 pending→real birth 之后的会话。
- 理由：点"在此项目创建新任务"表达的是"新开一个空白任务"，如果按工作区自动带回旧草稿，就和这个意图冲突了。用户已确认：不保留。
- 因此 Chat 只在 sessionId 为真实 ID 时才传 draftKey；pending 状态下传 undefined。sessionId 在组件挂载期间从 pending 变成真实 ID 时，卸载时用最新 key 写回，自然就覆盖了 birth 的情况。
- 不要改变 `useChatDraftScope` 在组件挂载期间切换会话时的现有行为。如果发现挂载期间会发生"真实 ID → 另一个真实 ID"的切换，在阶段 B 方案里说明，不要自己处理。

### 4. 测试 #2 的口径：同意，并补充以下用例
- Launcher 与 Chat 互不串；两个不同 sessionId 的 Chat 互不串；两个不同 draftKey 的 SimpleChatInput 互不串。
- 追加（必须有）：
  - a. **App 级测试**：首页输入 → 打开其他页面（触发单页壳关闭）→ "创建新任务"新建首页 → 草稿恢复。这个测试必须在修复前失败。
  - b. **发送后的竞态**：Launcher 发送成功并被替换为 Chat 后，卸载时的 cleanup 不能把已发送的文本写回存储；Chat 发送后切走再切回，输入框为空。
  - c. 发送失败时草稿保留。
  - d. 恢复时工作区不同：正文保留，引用和图片被清理。
  - e. 删除会话时清掉对应草稿（找到会话删除的现有入口，例如 SessionDeletionContext / handleDeleteSession，在那里调用 delete）。

### 5. 阶段 D 的环境：Gemini 安装 Rust，在桌面端实测
- 见文末"用户已裁决"。

## 附加要求
1. **图片 / 附件恢复**：先查清楚 `useAttachmentHandling.ts` 中 images 的数据形态。
   - 如果是自包含数据（inline_base64），可以恢复。
   - 如果依赖旧的 `attachmentSessionId`（`pending-tab-<旧tabId>`）下暂存的文件（第 429-437 行附近），旧 tab 关闭后可能已失效，这种情况**不恢复**，只恢复正文和 @提及，并在方案里说明。
2. **specs/handoff/ 下的报告**：这个目录不在 .gitignore 里。阶段报告可以继续放在本分支上，方便 Claude 审查，但最终合并前必须从分支中移除（或者由用户决定是否保留）。不要把它们混进功能提交里。
3. **阶段 B 的交付物**：`specs/handoff/composer_draft/stage-b.md`，内容包括：改动文件清单、每个文件改什么、草稿存储的 API 签名、各页面的 key 规则、写入 / 清理时机表、准备新增的测试列表、附件调查结论。推送后停下，等待审查。

## 用户已裁决（最终口径）
- **未发送的 pending Chat 草稿：不保留。** 第 3 条裁决就此定稿，测试里要加一条：pending Chat 输入后切走，再"在此项目创建新任务"，输入框为空（锁定这个行为）。
- **阶段 D 由 Gemini 在本机桌面端测试。** 第 5 条裁决改为：Gemini 安装 Rust 工具链，版本以仓库根目录的 `rust-toolchain.toml` 为准，不要用浮动版本；然后按 `package.json` / `specs/guides/` 中的开发构建方式启动 Tauri 桌面端，M1–M7 全部在桌面端实测（`dev:web` 的结果只能作补充）。安装和构建过程中遇到的问题照实回报，不要为了让构建通过去改仓库配置。安装 Rust 可以和阶段 B 并行，但阶段 C 的代码要等方案批准后再写。
