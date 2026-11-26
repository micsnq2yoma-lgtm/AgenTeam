# 阶段 C 回报：实现 + 回归测试 + 自动化验证

## 阶段 C 回报
- **本地 HEAD / 分支**：基于 `ab7434d17` 在分支 `fix/composer-draft-preserve` 完成功能提交与本报告提交（分开提交，见 C6）
- **做了什么**：
  1. 按 `stage-b.md` 与 `reviews/stage-b.md`（C1–C6）实现 renderer 进程内草稿存储 `composerDraftStore.ts`，并接入 `SimpleChatInput`、`BrandSection`、`Launcher`、`tabModule`、`Chat`、`App`。
  2. 先在未修复代码上运行 `App.composerDraft.test.tsx` 确认 App 级用例复现失败，随后实现修复并补齐单元测试、组件测试与 App 级回归测试。
  3. 同步按用户授权将 CMake（`D:\CMake`）和 Visual Studio 2022 Build Tools（`D:\BuildTools`）安装到 D 盘，并验证 `prepare-native-inference.mjs x86_64-pc-windows-msvc --check-prerequisites` 通过，为阶段 D 桌面端实测就绪。
- **结论 / 根因**：单页壳切换页面时卸载非活跃 `Launcher` / `Chat` Tab；现通过进程内 `composerDraftStore` 在卸载时按显式 `draftKey` 保存、挂载时恢复，并在发送接管与会话删除时清理。
- **改动文件列表（功能提交）**：
  - `src/renderer/components/chat-input/composerDraftStore.ts`（新增）
  - `src/renderer/components/chat-input/types.ts`
  - `src/renderer/components/chat-input/SimpleChatInput.tsx`
  - `src/renderer/components/launcher/BrandSection.tsx`
  - `src/renderer/pages/Launcher.tsx`
  - `src/renderer/features/launcher/tabModule.tsx`
  - `src/renderer/pages/Chat.tsx`
  - `src/renderer/App.tsx`
  - `src/renderer/components/chat-input/composerDraftStore.unit.test.ts`（新增）
  - `src/renderer/components/SimpleChatInput.draft.test.tsx`（新增）
  - `src/renderer/App.composerDraft.test.tsx`（新增）
- **diff 或已推送的分支名**：已推送至 `origin/fix/composer-draft-preserve`（功能提交与 `specs/handoff/composer_draft/stage-c.md` 分开提交）
- **测试 / lint / typecheck 命令与结果**：见下方第 2 节原文
- **手动验收结果**：阶段 C 不适用（待阶段 D 在桌面端执行 M1–M7）
- **偏离方案之处或不确定点**：
  - **额外修复的删除会话卸载顺序竞态**：测试 `deletes the session draft when handleDeleteSession succeeds` 暴露出 `App.tsx::handleDeleteSession` 中 `terminateTabsForSession` 调用 `tabWorkspaceController.replaceMany(plans)` 时未包裹 `flushSync`，导致 `deleteComposerDraft` 先于 React 提交卸载执行，随后卸载 cleanup 又把旧草稿写回 store。现已将 `terminateTabsForSession` 中的 `replaceMany` 用 `flushSync` 包裹（与 `App.tsx` 既有 `replaceMany` 模式一致），确保旧 Chat 在 `deleteComposerDraft` 之前同步卸载完成。
  - **已知限制（按 C6 记录）**：草稿中如附带图片，页面切换恢复时会丢掉（仅恢复正文与 `@` 提及上下文），原因见 `stage-b.md` 第 1 节（Tauri 拖拽图片 `attachment_ref` 绑定旧 `pending-<tabId>`）。
  - **原地切换 `sessionId` 行为（按 C4 记录）**：在已挂载 Chat 内点击「新任务」原地将 `sessionId` 从 `realA` 切到 `realB` 或 `pending-*` 时，维持现有行为——只在挂载时读、卸载时按最新 key 写回，不补存到旧会话 `realA` 下；已由组件测试锁定。
- **需要 Claude 裁决的问题**：无阻塞项；请审查功能提交代码与测试，批准后进入阶段 D 桌面端实测（M1–M7）。

---

## 1. 逐条对应 `reviews/stage-b.md`（C1–C6）

| 条目 | 落实情况 |
|---|---|
| **C1（阻塞项）** | `Launcher.tsx::handleBrandSend` 向 `onLaunchProject` 显式传入第 5 参 `{ onInitialMessageCommitted: () => { onSubmitted?.(); deleteComposerDraft(LAUNCHER_DRAFT_KEY); } }`；`App.tsx::handleLaunchProject` 仅在 `flushSync(...)` 替换 Launcher→Chat **之后**且 `initialMessage` 存在时调用 `launchOptions?.onInitialMessageCommitted?.()`。由于 `flushSync` 同步完成 `Launcher`/`SimpleChatInput` 卸载 cleanup，紧随其后的回调保证在其之后清除 `LAUNCHER_DRAFT_KEY`；且其他入口（侧边栏项目、小助理、任务中心等）不传该回调，绝不误删首页草稿。已新增 `App.composerDraft.test.tsx` 中 `C1: preserves Launcher draft when launching from sidebar project (with or without initialMessage)` 与 `C1: clears Launcher draft after Launcher send flushSync replacement and does not resurrect`。 |
| **C2** | `BrandSection.tsx::handleSend` 在 `recordingBusy` 分支改为 `return false`，并透传 `onSubmitted` 且 `return onSend(...)`；`Launcher.tsx::handleBrandSend` 核查了全部提前 return 分支（`!selectedWorkspace`、`runtimeExecutionUnavailable`、`launcherProviderModelIncomplete`、独立定时任务 `catch`）均改为 `return false`，成功分支返回 `true` / `return onLaunchProject(...)`。已新增 `SimpleChatInput.draft.test.tsx` 中 `C2: preserves draft when recordingBusy blocks send`。 |
| **C3** | `BrandSection.tsx` 挂载时读取 `restoredDraftWorkspacePath = getComposerDraft(LAUNCHER_DRAFT_KEY)?.workspacePath ?? null`，并维护 `awaitingRestoredWorkspaceCheckRef`：在恢复草稿尚未与真实加载的工作区完成比对前，若 `selectedProject?.path ?? null` 为 `null`，直接 return 不覆盖 `lastWorkspacePathRef`；加载完成后再以 `workspacePathsEqual` 比对并复用既有 `clearWorkspaceBoundDraft()` + `toasts.workspaceAttachmentsCleared`。已新增 `SimpleChatInput.draft.test.tsx` 中 `C3: strips workspace-bound references and shows toast when selectedProject loads asynchronously to a different workspace`。 |
| **C4** | 维持原地切换 `sessionId` 现有行为（仅挂载读、卸载按最新 key 写回，不补存到旧会话 `realA`），未改动 `useChatDraftScope`。已新增 `SimpleChatInput.draft.test.tsx` 中 `C4: locks in-place realA -> realB draftKey switch behavior on unmount`。 |
| **C5** | 1. 依赖边界：`npm run lint`（含 `depcruise --config .dependency-cruiser.cjs src`）0 error 通过，无需移动 `composerDraftStore.ts`。<br>2. 修复前必失败记录：已先在未修复代码上运行 `App.composerDraft.test.tsx`，失败原文见第 2.1 节。<br>3. 全部测试 / lint / typecheck 原文见第 2.2–2.4 节。 |
| **C6** | 1. `git grep -n "DRAFT-DEBUG" src/` 输出为空（无任何残留临时日志）。<br>2. 图片不恢复的已知限制已在本报告显式记录。<br>3. 功能代码+测试与 `specs/handoff/composer_draft/stage-c.md` 分开两次提交，逐文件 `git add`，使用 Conventional Commits + 非空 body。 |

---

## 2. 命令与验证输出原文

### 2.1 修复前 App 级回归测试失败原文（C5）

命令：
```text
npx.cmd vitest run --project dom src/renderer/App.composerDraft.test.tsx
```

输出原文：
```text
 RUN  v3.2.4 C:/Users/fanxi/.gemini/antigravity/scratch/AgenTeam

 ❯  dom  src/renderer/App.composerDraft.test.tsx (1 test | 1 failed) 506ms
   × App composer draft preservation (single-page shell) > preserves Launcher draft across single-page shell close to TeamOffice and return via New Chat 506ms
     → expected '' to be '打算找个人' // Object.is equality

⎯⎯⎯⎯⎯⎯⎯ Failed Tests 1 ⎯⎯⎯⎯⎯⎯⎯

 FAIL   dom  src/renderer/App.composerDraft.test.tsx > App composer draft preservation (single-page shell) > preserves Launcher draft across single-page shell close to TeamOffice and return via New Chat
AssertionError: expected '' to be '打算找个人' // Object.is equality

- Expected
+ Received

- 打算找个人

 ❯ src/renderer/App.composerDraft.test.tsx:467:61
    465|
    466|     const restoredTextarea = await screen.findByRole('textbox', {}, { …
    467|     expect((restoredTextarea as HTMLTextAreaElement).value).toBe('打算找个…
       |                                                             ^
    468|   });
    469| });

 Test Files  1 failed (1)
      Tests  1 failed (1)
```

### 2.2 修复后：测试分类检查 + 单元测试 + DOM 测试

命令：
```text
npm.cmd run test:classification
npx.cmd vitest run --project unit src/renderer/components/chat-input/composerDraftStore.unit.test.ts
npx.cmd vitest run --project dom src/renderer/components/SimpleChatInput.draft.test.tsx src/renderer/App.composerDraft.test.tsx src/renderer/hooks/useChatDraftScope.test.tsx src/renderer/components/SimpleChatInput.send.test.tsx src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx src/renderer/App.helperLaunch.test.tsx
```

输出原文（摘要与汇总）：
```text
> agenteam@0.7.3 test:classification
> node scripts/check-test-classification.mjs

test classification ok: 391 server tests (91 integration, 7 credentialed)

 RUN  v3.2.4 C:/Users/fanxi/.gemini/antigravity/scratch/AgenTeam

 ✓  unit  src/renderer/components/chat-input/composerDraftStore.unit.test.ts (3 tests) 3ms

 Test Files  1 passed (1)
      Tests  3 passed (3)

 RUN  v3.2.4 C:/Users/fanxi/.gemini/antigravity/scratch/AgenTeam

 ✓  dom  src/renderer/hooks/useChatDraftScope.test.tsx (1 test) 14ms
 ✓  dom  src/renderer/components/SimpleChatInput.draft.test.tsx (9 tests) 539ms
 ✓  dom  src/renderer/App.composerDraft.test.tsx (6 tests)
 ✓  dom  src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx (7 tests)
 ✓  dom  src/renderer/App.helperLaunch.test.tsx (66 tests)
 ✓  dom  src/renderer/components/SimpleChatInput.send.test.tsx (78 tests) 8681ms

 Test Files  6 passed (6)
      Tests  167 passed (167)
   Start at  01:30:21
   Duration  13.52s
```

### 2.3 Typecheck 与 Lint（含 ESLint / dependency-cruiser / verify:agent-docs）

命令：
```text
npm.cmd run typecheck
npm.cmd run lint
```

输出原文：
```text
> agenteam@0.7.3 pretypecheck
> node scripts/verify-agent-network-protocol.mjs && node scripts/patch-codemirror-view.mjs --check && node scripts/patch-react-virtuoso.mjs --check && node scripts/patch-tauri-events.mjs --check

Verified AgentNet protocol 1.2.0: 240dc3b654ab221db2904ba41dcf68d61d04492d6b3a10c370c59609c926720a

> agenteam@0.7.3 typecheck
> tsc --noEmit

> agenteam@0.7.3 lint
> eslint . --cache --cache-location .eslintcache --max-warnings=0 && npm run lint:deps && npm run verify:agent-docs

> agenteam@0.7.3 lint:deps
> depcruise --config .dependency-cruiser.cjs src

x 17 dependency violations (0 errors, 17 warnings). 2416 modules, 9704 dependencies cruised.

> agenteam@0.7.3 verify:agent-docs
> node --test scripts/doc-links.test.mjs && node scripts/verify-agent-docs.mjs

✔ finds broken inline, reference and image links with source locations (9.4086ms)
✔ resolves encoded relative paths and ignores code examples and external scopes (4.7932ms)
✔ reports malformed URL escapes instead of crashing the audit (2.2097ms)
✔ uses the first reference definition, matching Markdown rendering (2.8975ms)
ℹ tests 4
ℹ pass 4
ℹ fail 0
Agent docs verified: CLAUDE.md 131 lines / 13728 bytes; 39 routed docs exist; local file links checked in 58 documents.
```

### 2.4 桌面端构建前置环境核查（已按用户指示安装到 D 盘）

- Visual Studio 2022 Build Tools `17.14.45`（含 VCTools）已安装到 `D:\BuildTools`
- CMake `4.4.4` 已安装到 `D:\CMake`（MSVC 环境内置 `3.31.6-msvc6`）
- 前置校验输出原文：
```text
  Initialize MSVC x64 environment: D:\BuildTools\VC\Auxiliary\Build\vcvarsall.bat
  OK - MSVC x64 environment loaded
cmake version 3.31.6-msvc6
rustc 1.92.0 (ded5c06cf 2025-12-08)
Speech-inference build prerequisites ready for x86_64-pc-windows-msvc
```
