# 交接：对话输入框未发送草稿在页面切换后丢失

> **Gemini 每次开工前：先 `git pull` 本分支（`fix/composer-draft-preserve`），再读本文件第一节"当前状态与下一步"，照着做。** Claude 每次审查后都会更新这一节和 `reviews/` 下的审查意见，用户给你的指令可能只有一句"继续"。

## 当前状态与下一步

| 项目 | 内容 |
|---|---|
| 当前阶段 | **阶段 C：实现 + 回归测试 + 自动化验证**（阶段 B 方案已有条件批准） |
| 最新审查意见 | [`reviews/stage-b.md`](./reviews/stage-b.md)（**必读**：C1–C6 必须落实，C1 是阻塞项）；历史意见见 [`reviews/stage-a.md`](./reviews/stage-a.md) |
| 你要做的 | 1. 按 stage-b.md 方案 + 审查意见实现并补测试。<br>2. 跑测试 / lint / typecheck / 依赖检查，贴原文。<br>3. 报告写入 `specs/handoff/composer_draft/stage-c.md`，逐条对应 C1–C6。 |
| 停止点 | 推送代码和 stage-c.md 后停下，等待 Claude 审查 |
| 桌面端环境 | 安装 CMake + VS C++ Build Tools：**只有用户在指令里明确同意后才能安装**，否则先不装 |
| 用户已定 | 未发送的 pending Chat 草稿**不保留**；阶段 D 由 Gemini 在本机桌面端实测 M1–M7 |

阶段报告存放位置：今后都放在本目录（`specs/handoff/composer_draft/`），文件名为 `stage-<x>.md`。阶段 A 的报告保留在原位置 `specs/handoff/composer-draft-stage-a.md`。

> 本目录和阶段报告只用于协作审查，合并前由用户决定是否从分支移除，不要混进功能提交。

---


## 0. 协作分工

| 角色 | 负责 | 不负责 |
|------|------|--------|
| Claude（规划 / 检查） | 定方案、定验收标准、审查根因报告和代码 diff、裁决架构取舍 | 不在用户本机运行 App，看不到未推送的本地改动 |
| Gemini（执行 / 本地验收） | 在用户本机复现、加日志定位、写代码和测试、跑测试 / lint / typecheck、在真实 App 里手动验收、按模板回报 | 不自行改变方案范围；遇到下文"必须停下"的情况先回报 |

工作流程分 4 个阶段，每个阶段结束时 Gemini 按第 6 节的模板回报，Claude 审查通过后才进入下一阶段：

1. **阶段 A：复现 + 定位根因**（只加临时日志，不改逻辑）→ 回报 → Claude 审查
2. **阶段 B：Gemini 提出修复方案**（写明改哪些文件、为什么）→ Claude 批准
3. **阶段 C：实现 + 回归测试 + 自动化验证** → 推送分支或贴出完整 diff → Claude 审查代码
4. **阶段 D：本地手动验收 + 提交** → 回报 → Claude 最终确认

Claude 只能审查已推送到远端的分支，或回报里直接贴出的 diff 和日志。

---

## 1. 问题描述

- 用户操作：在首页（Launcher）对话输入框输入文字，例如在项目"Mino"下输入"打算找个人"，不发送；点击左侧边栏的其他页面（智能体团队 / 自动化任务 / 技能与工具 / 聊天机器人 / 设置 / 项目 / 历史任务等）；再切回原来的对话界面。
- 现象：输入框里的草稿消失。
- 期望：切回后草稿原样保留。同一问题在 Chat 标签页上也要检查。

## 2. 基线

- 仓库：AgenTeam（MyAgents 桌面端，Tauri v2 + React 19）
- Claude 分析时的基线提交：`1f00131`（`fix(kb): list only Agents with an active Project as curator choices`）。如果本地 HEAD 不同，在回报里写明。
- 必读：`CLAUDE.md`、`specs/tech_docs/react_stability_rules.md`、`specs/tech_docs/session_architecture.md`（Session ID / pending→real birth / Tab 章节）。只有改 UI 时才需要读 `specs/DESIGN.md`。

## 3. Claude 已确认的代码事实（只读代码得出，未在真实 App 中运行）

1. **草稿只存在组件内部状态里**
   `src/renderer/components/chat-input/SimpleChatInput.tsx`：
   `const [inputValue, setInputValue] = useState(externalValue ?? '')`。
   项目里没有任何草稿存储（搜索 draftStore / composerDraft / inputDraft 都没有结果），组件卸载或重新挂载后草稿就丢了。@提及、图片、上下文也都是组件内部状态。
2. **~~正常导航不会卸载首页~~（已被阶段 A 推翻）**
   实际情况：`src/renderer/App.tsx:893-903` 是"单页壳"：`activeTabId` 一变化，就对所有非当前 Tab 调用 `requestCloseTab(tab.id, 'replaced')`，旧的 Launcher / Chat 会被关闭并卸载。这是有意设计，用于释放 Session。详见阶段 A 报告和 `reviews/stage-a.md`。
3. **首页渲染链**
   `features/launcher/tabModule.tsx` → `pages/Launcher.tsx` → `components/launcher/BrandSection.tsx` → `SimpleChatInput`。
   - Launcher 的 `initialMount` 是 `'deferred-content'`；`isDeferred` 为 true 时只渲染一个空 div，此时 Launcher 已被卸载。
   - Launcher.tsx 已经注释过"chat 侧保持挂载，以便去知识库页后草稿还在"，说明保留草稿本来就是产品要求。
4. **嫌疑路径清单（阶段 A 要逐一确认或排除）**
   - `App.tsx` 中的 `tabWorkspaceController.replaceWith(...)`：替换后 tab id 不变，但里面是新组件，例如 record 复用 launcher 标签页、pending 会话回退成 launcher
   - `handleSidebarNewChat`（"创建新任务"，聚焦最左边的首页标签页；没有首页标签页时调用 `handleNewTab`）
   - `handleOpenWorkspaceFromSidebar`（点击项目会 `createNewTab` + `openLaunchTabNow`）
   - detach / close 标签页后重新创建 fallback 首页
   - `deferredMountTabIds` 的增删
   - `BrandSection` 中切换工作区时调用的 `clearWorkspaceBoundDraft()`（按设计只去掉工作区相关的引用和图片，不清空正文）
   - `SimpleChatInput` 里 `externalValue` 同步的 useEffect，以及各个 `clear*` 方法
   - `src/renderer/hooks/useChatDraftScope.ts`（Chat 切换会话时只清理 query context）
   - 某个 `key=` 发生变化导致的重新挂载
5. **已由阶段 A 确认**：丢失属于"组件被卸载"，由单页壳关闭非当前 Tab 触发。

## 4. 修复方向（阶段 B 的方案必须符合以下约束）

> **注意**：本节的 key 规则（首页按 tab.id 区分）和覆盖范围已被 `reviews/stage-a.md` 的裁决取代：首页用固定 key `'launcher'`，Chat 只覆盖有真实 sessionId 的会话，pending Chat 不保留。两者冲突时以审查意见为准。

**优先级：先修 owner 错位，再考虑加存储。**

- 如果根因是某条路径错误地替换或新建了首页标签页 → 修那条路径（例如应该聚焦已有的首页标签页）。
- 如果根因是值被不该触发的逻辑清空 → 修那段清空逻辑。
- 只有当卸载本身合理、无法避免时，才引入草稿存储：
  - **Owner**：首页草稿按 tab id 区分，可复用 `createPendingSessionId(tab.id)`，tabModule 已经把它用作 attachmentSessionId；Chat 草稿按 sessionId 区分。pending→real birth 要沿用同一份草稿（参考 `useChatDraftScope` 的 sameBirth 逻辑）。
  - **存储位置**：renderer 进程内的一个小模块，用 Map 加读写函数即可。不放进 Launcher / Chat 的 React state（SimpleChatInput 注释明确说过：为了性能，输入值不能每次按键都抬升到父组件）。不写 config.json，不经过 Sidecar，不新增 IPC。
  - **写入时机**：卸载时或 debounce 时写入；挂载时用存储中的草稿初始化 inputValue。
  - **范围**：至少恢复正文；@提及和图片能安全恢复就一起恢复，否则在回报里说明。
  - **清理**：发送成功（`clearSubmittedDraft` 路径）、关闭标签页、删除会话时清掉对应草稿。
  - **默认不跨应用重启持久化**。如果认为需要，先停下来回报。

**禁止**：使用 localStorage 或全局单例草稿；用 cache / retry / flag 打补丁；suppress lint；修改无关文件；在 main 上提交；用 `git add -A` 或 `git add .`。

## 5. 验收标准

### 自动化（阶段 C）
新增回归测试（vitest + Testing Library，可参考 `src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx`、`src/renderer/components/SimpleChatInput.send.test.tsx`、`src/renderer/hooks/useChatDraftScope.test.tsx`），至少覆盖：
1. 阶段 A 找到的那条路径：输入 → 切走 → 切回 → 草稿还在（这个测试必须在修复前失败、修复后通过）
2. 两个首页标签页 / 两个会话之间的草稿互不串
3. 发送后草稿被清空，切走再切回不会复活
4. pending→real birth 后草稿保留
5. 切换工作区时只去掉工作区相关的引用，正文保留

另外要通过相关测试、lint 和 typecheck（命令以 `package.json` 为准）。

### 手动（阶段 D，在真实 App 里）
| # | 操作 | 期望 |
|---|------|------|
| M1 | 首页"Mino"输入"打算找个人" → 智能体团队 → 切回 | 草稿还在 |
| M2 | 同上，分别经过自动化任务 / 技能与工具 / 聊天机器人 / 设置 | 草稿还在 |
| M3 | 同上，经过点击项目、打开历史任务 | 草稿还在 |
| M4 | 输入草稿 → 切到知识库 → 切回对话 | 草稿还在（回归） |
| M5 | 输入后发送 → 切走 → 切回 | 输入框为空 |
| M6 | Chat 标签页中输入草稿 → 切走 → 切回 | 草稿还在 |
| M7 | 切换工作区 | 正文保留，工作区相关的引用被去掉并出现 toast |

## 6. 回报模板（每个阶段结束时使用）

```
## 阶段 X 回报
- 本地 HEAD / 分支：
- 做了什么：
- 结论 / 根因（阶段 A 必填：触发路径 + 是"卸载"还是"被清空" + 关键日志原文）：
- 改动文件列表（阶段 C 起）：
- diff 或已推送的分支名：
- 测试 / lint / typecheck 命令与结果（贴原文，失败也照实贴）：
- 手动验收结果（阶段 D：M1–M7 逐条写 通过 / 失败 / 未测）：
- 偏离方案之处或不确定点：
- 需要 Claude 裁决的问题：
```

## 7. 必须停下来回报的情况

- 根因不在第 3 节的嫌疑清单里，或者涉及多条路径
- 修复需要新增状态 owner、通信方式或进程，或者要改 Rust / Sidecar
- 需要跨应用重启持久化草稿
- 现有测试在改动前就已失败
- 改动范围超出 renderer 的输入框 / 首页 / 标签页相关文件

## 8. Git 约定

- 在新分支上工作，例如 `fix/composer-draft-preserve`，不要在 main 上提交
- 提交时逐个列出文件
- Commit 使用 Conventional Commits，subject 例如 `fix(chat-input): preserve unsent composer draft across page switches`，并附上说明"为什么 / 关键取舍"的非空 body
- 合并、发布等外部动作需要用户明确授权
