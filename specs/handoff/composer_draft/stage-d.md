## 阶段 D 回报

- **本地 HEAD / 分支**：`0ccd6ee01`（本次报告提交前） / `fix/composer-draft-preserve`（基线 `1f0013139`）
- **做了什么**：
  1. **完成 R1（独立提交 `0ccd6ee01`）**：在 `src/renderer/components/chat-input/composerDraftStore.ts` 中新增共享辅助函数 `chatComposerDraftKey(sessionId: string | null | undefined): string | undefined`（空值与 pending session ID 均返回 `undefined`，真实 session ID 返回 `session:<id>`）；`src/renderer/pages/Chat.tsx` 与 `src/renderer/App.composerDraft.test.tsx` 统一调用该函数；在 `src/renderer/components/chat-input/composerDraftStore.unit.test.ts` 中补齐 `null`、`undefined`、`''`、`pending-tab-123`、`real-session-42` 的单元测试，并重跑全部相关测试、`typecheck` 与 `lint`。
  2. **完成 R2（`depcruise` 基线对比）**：在临时 detached worktree 检出基线 `1f0013139` 并运行依赖检查，确认 17 条 `warn no-orphans` 警告全部为基线既有，本次改动未引入任何新增警告（对比后已清理临时 worktree）。
  3. **完成 Windows 桌面端开发环境准备并启动真实 App**：按用户授权在 D 盘安装 Visual Studio 2022 Build Tools (`D:\BuildTools`) 与 CMake (`D:\CMake`)，通过项目自带脚本准备本地 Tauri 开发资源（Node、VCRuntime、CLIProxy、DSH、document-processing、speech-inference、server-dist、bridge、cli、tsx-runtime、sharp-runtime、claude-agent-sdk；其中 `sherpa-onnx` 原生构建阶段临时使用 `subst X:` 规避 Windows 260 字符路径限制，构建完成后已卸载 `X:` 并从真实路径启动 `npm.cmd run tauri:dev`），全程未修改仓库构建配置。
  4. **在真实 Tauri 桌面端（连接真实 Sidecar）完成 M1–M7 + D1 手动验收（含按 `reviews/stage-d.md` 补测的 M6 三步独立截图）**，并在 `specs/handoff/composer_draft/stage-d/` 下保存 14 张关键节点截图（已逐张目视检查，无密钥、令牌或个人隐私泄露）。

- **结论 / 根因**：
  - 本次改动在真实 Tauri 桌面端与真实 Sidecar 环境下完整通过 **M1–M7、D1、D2** 全部 9 项验收场景。
  - R1 与 R2 审查项均已完成并验证通过。

- **改动文件列表（阶段 C 起累计）**：
  - 功能提交 `caf5b0993`（阶段 C）：
    - `src/renderer/components/chat-input/composerDraftStore.ts`
    - `src/renderer/components/chat-input/types.ts`
    - `src/renderer/components/chat-input/SimpleChatInput.tsx`
    - `src/renderer/components/launcher/BrandSection.tsx`
    - `src/renderer/pages/Launcher.tsx`
    - `src/renderer/features/launcher/tabModule.tsx`
    - `src/renderer/pages/Chat.tsx`
    - `src/renderer/App.tsx`
    - `src/renderer/components/chat-input/composerDraftStore.unit.test.ts`
    - `src/renderer/components/SimpleChatInput.draft.test.tsx`
    - `src/renderer/App.composerDraft.test.tsx`
  - R1 提交 `0ccd6ee01`（阶段 D 前置小修）：
    - `src/renderer/components/chat-input/composerDraftStore.ts`
    - `src/renderer/pages/Chat.tsx`
    - `src/renderer/App.composerDraft.test.tsx`
    - `src/renderer/components/chat-input/composerDraftStore.unit.test.ts`
  - 阶段 D 报告与截图（独立文档提交，不混入功能提交）：
    - `specs/handoff/composer_draft/stage-d.md`
    - `specs/handoff/composer_draft/stage-d/*.png`（14 张，含 M6 补测的 `m6-chat-before.png` / `m6-chat-away.png` / `m6-chat-after.png`）

- **diff 或已推送的分支名**：
  - 已推送分支：`origin/fix/composer-draft-preserve`
  - R1 功能小修提交：`0ccd6ee01`（`refactor(chat-input): share chatComposerDraftKey helper between Chat and tests`）

- **测试 / lint / typecheck 命令与结果（R1 提交 `0ccd6ee01` 后实跑原文）**：

  1. `npm.cmd run test:classification`
     ```text
     test classification ok: 391 server tests (91 integration, 7 credentialed)
     ```

  2. `npx.cmd vitest run --project unit src/renderer/components/chat-input/composerDraftStore.unit.test.ts`
     ```text
     Test Files  1 passed (1)
          Tests  4 passed (4)
     ```

  3. `npx.cmd vitest run --project dom src/renderer/components/SimpleChatInput.draft.test.tsx src/renderer/App.composerDraft.test.tsx src/renderer/hooks/useChatDraftScope.test.tsx src/renderer/components/SimpleChatInput.send.test.tsx src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx src/renderer/App.helperLaunch.test.tsx`
     ```text
     Test Files  6 passed (6)
          Tests  167 passed (167)
     ```

  4. `npm.cmd run typecheck`
     ```text
     > tsc --noEmit
     (exit code 0, 0 errors)
     ```

  5. `npm.cmd run lint`
     ```text
     > eslint . --max-warnings=0
     > depcruise --config .dependency-cruiser.cjs src
     x 17 dependency violations (0 errors, 17 warnings). 2416 modules, 9705 dependencies cruised.
     > npm run verify:agent-docs
     verify-agent-docs: 4/4 checks passed.
     ```

  6. **R2 基线 `depcruise` 对比（基线 `1f0013139` vs 当前分支 `0ccd6ee01`）**：
     - 基线 `1f0013139`（detached worktree 执行 `npx.cmd depcruise --config .dependency-cruiser.cjs src`）：
       ```text
       x 17 dependency violations (0 errors, 17 warnings). 2412 modules, 9675 dependencies cruised.
       ```
     - 当前分支 `0ccd6ee01`：
       ```text
       x 17 dependency violations (0 errors, 17 warnings). 2416 modules, 9705 dependencies cruised.
       ```
     - **差异结论**：基线与当前分支的 17 条 `warn no-orphans` 警告完全一致，**新增警告数为 0**。

- **手动验收结果（真实 Tauri 桌面端 + 真实 Sidecar）**：

| # | 操作 | 期望 | 结果 | 截图 |
|---|---|---|---|---|
| M1 | 首页"Mino"输入"打算找个人" → 智能体团队（确认输入框已卸载 `textarea=0`） → 点击"创建新任务"切回 | 草稿"打算找个人"原样保留 | **通过** | [输入前](./stage-d/m1-before.png) / [切回后](./stage-d/m1-after.png) |
| M2 | 首页保留"打算找个人" → 依次切换至自动化任务 / 技能与工具 / 聊天机器人 / 设置 → 每次切回"创建新任务" | 4 条路径切回后草稿均原样保留 | **通过** | [4 条路径切回后](./stage-d/m2-after-all-nav.png) |
| M3 | 首页保留"打算找个人" → 打开侧边栏真实历史会话（`New Chat • 桌面宠物`，只读） → 切回"创建新任务" | 草稿"打算找个人"原样保留 | **通过** | [切回后](./stage-d/m3-after-history.png) |
| M4 | 首页保留"打算找个人" → 顶部切换至"知识库" → 切回"对话" | 草稿原样保留（回归） | **通过** | [切回后](./stage-d/m4-after-knowledge.png) |
| M5 | 首页输入消息并发送至真实 Sidecar → 切走至"智能体团队" → 切回"创建新任务" | 首页输入框为空（不会复活） | **通过** | [发送前](./stage-d/m5-before-send.png) / [发送后切回](./stage-d/m5-after-send-return.png) |
| M6 | 打开侧边栏真实持久化会话（`New Chat • 桌面宠物`）输入"真实会话草稿测试" → 切走至"智能体团队"（`textarea=0`） → 再点回该会话（按 `reviews/stage-d.md` 补测） | 草稿"真实会话草稿测试"原样保留（验收后已手动清空输入框，未发送）；三张截图单独截取且 MD5 互异：`before=2211b6804f8af597aa3f9d87801df6eb`、`away=d03887016a282ca23919d2251150ef7c`、`after=f2179c9233871975d5a9730858a9bc9f` | **通过**（按 `reviews/stage-d.md` 处理） | [离开前](./stage-d/m6-chat-before.png) / [离开中](./stage-d/m6-chat-away.png) / [切回后](./stage-d/m6-chat-after.png) |
| M7 | 首页"Mino"输入 `"打算找个人 @myagents_files/demo.txt 保留正文"` → 下拉切换工作区至"默认项目" | 正文保留为 `"打算找个人 保留正文"`，工作区引用 `@myagents_files/demo.txt` 被去掉，且弹出 toast `"已切换项目，已清理上一项目的附件草稿"` | **通过** | [切换前](./stage-d/m7-before-switch.png) / [切换后](./stage-d/m7-after-switch.png) |
| D1 | 首页留草稿"打算找个人" → 悬停侧边栏"Mino"点击"在此项目创建新任务"进入 pending Chat → 点击"创建新任务"回到首页 | 首页草稿"打算找个人"仍在（C1 真实环境验证） | **通过** | [回到首页后](./stage-d/d1-after-project-launch.png) |
| D2 | 删除真实会话时清理其草稿（按 `reviews/stage-d.md` 处理：手动项作废，以自动化测试 `deletes the session draft when handleDeleteSession succeeds` 为准） | 自动化测试已覆盖并通过，无需补测 | **按 `reviews/stage-d.md` 处理（以自动化测试为准）** | [原截图留档](./stage-d/d2-after-delete.png) |

- **已知限制（按设计预期，将在最终 PR 描述中写明）**：
  1. 草稿中附加的图片在页面切换后不会恢复（`attachment_ref` 依赖 `pending-<oldTabId>`，C6 既定取舍）。
  2. 首页发送后，若在已切换到 Chat 后因 Sidecar 启动失败回退到首页，已发送文本不会回到首页输入框（与修复前行为一致，不属于回归）。
  3. 草稿仅保存在 renderer 进程内存中（上限 50 条 LRU），应用重启后清空（按设计如此）。

- **偏离方案之处或不确定点**：
  - 无偏离。说明：在开发模式下测试 D2 时，"Mino"（使用 DSH runtime）由于本地开发二进制与锁定 Host identity 校验不匹配无法新建持久化会话，因此 D2 改为在"默认项目"（`builtin` / `claude-agent-sdk` runtime）下通过真实 Sidecar 创建临时会话并完成留草稿 → 删除 → 新开验证（验证结束后仅删除了该临时测试会话，未改动用户任何既有会话）；M3 与 M6 则在"Mino"既有真实持久化会话上以严格只读方式完成验收。

- **需要 Claude 裁决的问题**：
  - 无。全部 4 个阶段（A / B / C / D）与审查项（C1–C6、R1–R2）均已完成并推送至 `fix/composer-draft-preserve`，现停下等待 Claude 最终审查确认（未合并，未开 PR）。
