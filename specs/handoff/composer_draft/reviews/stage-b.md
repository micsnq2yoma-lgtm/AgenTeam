# Claude 审查意见：阶段 B（分支 fix/composer-draft-preserve @ ed25acc）

## 结论：方案有条件批准，可以进入阶段 C（写代码和测试）

已核实的点：
- `clearSubmittedDraft` 的实现（`SimpleChatInput.tsx` 约 1077 行）
- Launcher 切换为 Chat 时 `handleLaunchProject` 里的 `flushSync`（`App.tsx` 约 2075 行）
- 组件挂载期间 Chat 原地换 `sessionId` 的两条路径（`App.tsx` 约 1669 行、约 3031 行）
- 删除会话的唯一入口（`deleteSessionThroughAppOwner` 只在 `App.tsx` 约 4297 行被调用）

附件调查结论和"不恢复 images、只恢复正文 + @提及上下文"的范围：**批准**。

下面 C1–C2 是**必须改**的，C3–C6 是**必须做到**的，阶段 C 报告要逐条说明是怎么落实的。

## 必须修改

### C1. 不能无条件删除 Launcher 草稿（阻塞项）
方案第 4 节写的是：`handleLaunchProject` 在 `flushSync` 后无条件调用 `deleteComposerDraft(LAUNCHER_DRAFT_KEY)`。但 `handleLaunchProject` 不只被首页调用，**还被其他入口调用，而且会带上 `initialMessage`**：`App.tsx` 约 2204、3150、3909、4198、4505 行（侧边栏项目、小助理、任务中心等）。按现在的写法，用户在首页留了草稿，再从其他入口发起一次 launch，首页草稿就会被误删。

要求：
- 只有当**这次 launch 的发起方是首页的发送操作**时才删除 Launcher 草稿。推荐做法：由 Launcher 的发送路径显式表明来源（例如复用 `onLaunchProject` 的 analyticsContext / 新增明确的来源字段，或者由 Launcher 传入一个回调），由 App 在切换完成、确认 `initialMessage` 已被接管的那一点调用。不要用"当前 active tab 是 launcher"之类的推断。
- 删除时机必须在 `flushSync` 之后：卸载时的 cleanup 会先把文本写回，必须在它之后删除。在报告里说明你是怎么保证这个顺序的。
- 新增测试：首页留有草稿 → 从侧边栏点项目新建任务、从其他入口带 initialMessage 发起 launch → 回到首页，草稿仍在。

### C2. `recordingBusy` 分支必须返回 `false`
`BrandSection::handleSend` 第一行是 `if (recordingBusy) return;`。改成 `return onSend(...)` 之后，这个分支返回 `undefined`，`SimpleChatInput` 会把它当作"已接受"，调用 `clearSubmittedDraft()` 把草稿清掉（这个问题现在就存在，你的改动会让它也影响到存储）。改成 `return false`，并逐个检查 `handleSend` / `handleBrandSend` 中所有提前 return 的分支：凡是"没发出去"的，都必须返回 `false`。补一条测试：录音中按发送，草稿保留。

## 必须做到

### C3. 工作区异步加载时不能丢掉"保存时的工作区"
方案用 `lastWorkspacePathRef` 的初值携带保存时的 `workspacePath`。但现有 effect 末尾会无条件执行 `lastWorkspacePathRef.current = next`：如果挂载时 config 还没加载完（`selectedProject` 为 undefined，next 为 null），ref 会被覆盖成 null；之后工作区加载完成，又会因为"上一次是 null"跳过清理，导致**跨工作区的引用没被清掉**。

要求：
- 在恢复的草稿尚未完成工作区比对之前，next 为 null 时不要覆盖 ref。
- 补一条测试：挂载时 selectedProject 为 undefined，随后变成一个和保存时不同的工作区 → 引用被清理并弹出 toast。

### C4. 原地切换 sessionId：维持现状（回答第 8 节问题 2）
- 维持现状：只在挂载时读取草稿、卸载时用最新的 key 写回。点"新任务"原地切换会话时，可见文本按现有行为跟着输入框走，不会补存到旧会话 realA 下。这个行为是可以接受的。
- 在 stage-c 报告里写一句说明这个行为，再补一条测试固定下来：realA 输入 → 原地切到 realB → 卸载 → realA 的 key 下没有草稿，realB 的 key 下有。

### C5. 依赖边界与验证
- `App.tsx` 从 `components/chat-input/composerDraftStore` 导入，要能通过 `.dependency-cruiser.cjs` 的检查。跑仓库现有的依赖检查和 lint，不要 suppress。如果被规则拦下，把 store 移到规则允许的位置（例如 `src/renderer/` 下一个中立的位置），并在报告里说明。
- 阶段 C 必须跑：新增和相关的测试、`useChatDraftScope` / `SimpleChatInput.send` / Launcher 相关的既有测试、App 级测试、lint、typecheck、依赖检查。命令以 `package.json` 为准，结果贴原文。
- App 级测试中"修复前必须失败"的那条：先在没有修复的代码上跑一次，把失败输出贴进报告。

### C6. 小项
- 最终提交里不能留任何 `[DRAFT-DEBUG]` 日志。
- 草稿里如果有图片，恢复时会丢掉，这一点在报告里写明，作为已知限制。
- 功能提交和 `specs/handoff/` 文档分开提交。功能提交要写 Conventional Commits 的 subject 和说明原因的 body，逐个列出文件。

## 第 8 节问题 3（桌面端环境）
安装 CMake 和 VS C++ Build Tools 需要几个 GB 空间和管理员 UAC 权限，Claude 已转给用户确认。**只有用户在给你的指令里明确同意后才能安装**；同意后可以和阶段 C 并行进行。没有得到同意之前，阶段 D 先用 `dev:web` 测能测的项，其余标为"待桌面端"。

## 阶段 C 交付
- 代码和测试推送到本分支
- 报告写在 `specs/handoff/composer_draft/stage-c.md`：用回报模板，逐条对应 C1–C6，贴命令输出原文
- 推送后停下，等待审查
