# 设计：启动页的"工作区基础运行环境"与"本次执行运行环境"

> 日期：2026-11-23。状态：**草案，等待用户确认后才开发**（用户 2026-11-23 同意"先写设计、再单独做"）。
> 来源：上游 MyAgents 0.5.15 的 `8955cfe`（`fix(runtime): preserve workspace runtime selection across provider changes`）。它没有直接移植，因为它与 AgenTeam 启动页的"默认模型继承"在同一处推导运行环境和 provider。本文只依据读过的代码和上游提交，没有运行应用。

## 1. 要修的问题

启动页用"某个订阅模型对应的执行器"去显示工作区的运行环境偏好。症状（上游描述）：先选一个 Codex 订阅模型，再选 DSH，保存的是 DSH，界面却显示 Claude。根因是**一个值同时承担两个含义**：

| 含义 | 谁拥有 | 何时变化 |
|---|---|---|
| **基础选择**：用户为这个 Agent / 工作区选的运行环境偏好（builtin、dsh、claude-code、codex、antigravity） | Agent 配置（`runtime`、`runtimePreference`、`runtimeConfig.source`） | 用户在运行环境选择器里改 |
| **执行身份**：这一次 Session 实际由谁执行（例如选了 `codex-sub` 模型，执行器是托管 Codex；选了 `anthropic-sub`/`xai-sub`，执行器固定是 Claude Agent SDK） | 本次选择的 provider 与模型，在 Session 出生时固化 | 每次换模型 / provider |

CLAUDE.md 的 owner 规则要求 desired state、execution identity 和前端派生状态分开，各有自己的权威；后产生的执行层数据不能反过来覆盖基础选择。

## 2. 现状（两边的差别）

上游 0.5.15：`resolveAgentRuntimeSelection(agent, config.defaultIntegratedRuntime)` 读基础选择；`resolveEffectiveRuntime(基础选择, undefined, undefined, launcherProvider?.id)` 得到执行运行环境；provider 约束（`providerRequiresClaudeSdk`）只作用于执行控件和 Session 出生，不改显示。

AgenTeam：启动页还有一层上游没有的"默认模型继承"：`availableDefault`（由 `config.defaultModelSelection`、Agent 与工作区的默认模型推出）、`resolvedLauncherRuntime`（用 `availableDefault` 的 provider id 参与运行环境推导）、`defaultModel` / `defaultModelProviderId`、`effectiveLauncherModel`。Chat 页已按同样思路收敛成单一的 `currentRuntime`（`fbcb0af` 的移植里做了），启动页还没有。

## 3. 目标设计（用户 2026-11-23 已拍板）

用户的决定：**运行环境选 DSH、模型选 Codex 模型时，自动切换并显示真正执行的运行环境，同时弹出通知提醒。**

两个值仍然分开，但界面显示的是**执行值**，并在切换时告知：

```text
preferredRuntime = resolveAgentRuntimeSelection(selectedAgent, config.defaultIntegratedRuntime)   // 用户保存的偏好，只读 Agent 配置
executionRuntime = resolveEffectiveRuntime(preferredRuntime, …, executionProviderId)               // 本次选的模型/provider 决定的真实执行者
```

- **运行环境选择器显示 `executionRuntime`**（真实执行者）。用户选 Codex 模型后，选择器自动变成 Codex，不再显示一个与实际不符的 DSH。
- **偏好 `preferredRuntime` 不被模型选择改写**：选 Codex 模型不会把 Agent 的偏好改成 Codex。把模型换回 DSH 能用的模型后，运行环境自动回到偏好（DSH）。只有用户**在运行环境选择器里主动选择**才改偏好。这也是上游 8955cfe 要保证的"先 Codex 模型再选 DSH，保存 DSH"。
- **通知**：用户**主动**选了一个需要别的执行者的模型时，弹出 toast（沿用 `useToast`），内容如"该模型只能由 Codex 运行，已切换到 Codex。换回其他模型后会回到 DSH。"。换回时同样提示一次。
- **默认模型继承**：同一规则。继承来的默认模型需要别的执行者时，选择器显示真实执行者，**不弹 toast**（不是用户刚做的操作，每次打开启动页都弹会变成打扰）；选择器本身显示真实执行者，**不再有常驻标注**（用户 2026-11-23 要求移除：只在用户切换模型导致执行者变化的那一次弹顶部通知）。
- 输入框的模型列表、权限列表、推理强度、Session 出生字段一律用 `executionRuntime`。
- 已有 Session 的绑定仍然权威，不被重新推导。

### 3.1 要改的文件

- `src/shared/types/runtime.ts`：新增 `resolveAgentRuntimeSelection`，`resolveEffectiveRuntime` 复用同一个投影函数（上游已有实现，可直接移植）。
- `src/renderer/pages/Launcher.tsx`：用 `preferredRuntime` / `executionRuntime` 替换混合推导；选择器的 `value` 用 `executionRuntime`；记录"上一次的 `executionRuntime`"，变化且由用户模型选择触发时弹 toast；默认模型继承只影响 `executionProviderId`。
- `src/renderer/components/launcher/LauncherRuntimeToolbarControl.tsx` 与 `RuntimeSelector.tsx`：选择器 `value` 显示执行者；i18n 增加 toast 文案（zh-CN、en-US）。
- `src/renderer/components/AgentSettings/WorkspaceBasicsSection.tsx`：设置页显示的是偏好（`preferredRuntime`），不随模型变化。
- `src/shared/agentConfigMutation.ts`：provider 切换按"锁内的 Agent 记录"一次解析（上游做法），固定 Claude 权限的 provider 用 builtin 的权限映射；保留我们的 `getDefaultNewSessionPermissionMode`。
- `src/server/admin-api.ts`：`agent set runtime / providerId` 走同一个锁内的 Agent 变更。

### 3.2 不改什么

- Session 出生后的运行环境、已存在 Session 的绑定。
- 默认模型继承的来源与优先级。
- 运行环境选择器的位置（仍在工具栏）。

## 4. 验收

自动化（沿用并补全上游 `8955cfe` 的测试）：
- 两种选择顺序：先选 Codex 模型再选 DSH，和先选 DSH 再选 Codex 模型：保存值与显示值都是 DSH / 都不变，执行身份是托管 Codex。
- 回到可移植模型：执行身份回到基础选择。
- 固定 Claude 的 provider（`anthropic-sub`、`xai-sub`）：权限列表与默认值按 builtin；基础选择不被改写。
- 选 Codex 模型时：选择器显示 Codex、弹一次 toast；偏好仍是 DSH；换回可移植模型后回到 DSH 并提示一次。
- 用户主动在运行环境选择器选 DSH：偏好改为 DSH，且之后不再被模型选择改写。
- **AgenTeam 特有**：默认模型继承需要别的执行者时，选择器显示真实执行者，**不弹 toast、不显示额外标注**；打开启动页多次不重复提示。
- `admin-api` 的 `agent set runtime / providerId` 与桌面选择器写出同样的 Agent 记录。

需要真实应用（独立验收，不是自检）：
- 在启动页依次点上述几种选择，截图或记录显示值。
- 带着默认模型继承配置（配置里有 `defaultModelSelection`）重复一遍。

## 5. 风险与顺序

- **风险中等**：启动页是最常用入口，且 AgenTeam 与上游的差别正在这里。
- **顺序**：先在 `Launcher.tsx` 之外落地共享函数和 `agentConfigMutation` 的变更（可单测），再改启动页，最后改设置页显示。每步单独 CI 全绿。
- 本机无法验证界面，必须有独立验收。

## 6. 已确认与剩余

已确认（2026-11-23）：模型需要别的执行者时自动切换并显示真实执行者，同时弹出通知。

按此决定我做了两处解释，开发前如不对请告知：
1. **偏好是否被改写**：我解释为**不改写**，选择器显示执行者，偏好留在配置里，换回模型后自动回到偏好。若你希望选 Codex 模型就把偏好永久改成 Codex，需要告诉我。
2. **默认模型继承时是否弹通知**：我解释为**不弹**，选择器显示执行者即可，避免每次打开启动页都打扰（常驻标注已按用户要求移除）。
