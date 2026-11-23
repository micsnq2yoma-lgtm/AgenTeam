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

## 3. 目标设计

两个值，单向依赖：

```text
selectedRuntime  = resolveAgentRuntimeSelection(selectedAgent, config.defaultIntegratedRuntime)   // 只读 Agent 偏好，不看 provider
executionRuntime = resolveEffectiveRuntime(selectedRuntime, …, executionProviderId)               // 看本次选中的 provider
```

- **运行环境选择器、"工作区运行环境"显示、保存**一律用 `selectedRuntime`。
- **输入框的模型列表、权限列表、推理强度、Session 出生字段**一律用 `executionRuntime`。
- **默认模型继承只影响 `executionProviderId`**，不参与 `selectedRuntime`。规则：默认模型对应的 provider 在 `selectedRuntime` 下可用（`isProviderModelCompatibleWithRuntime`）才采用；否则忽略，不静默改运行环境。
- **与已有 Session 的关系**：已有 Session 的绑定仍然权威，不被重新推导。

### 3.1 要改的文件

- `src/renderer/pages/Launcher.tsx`：用上面两个值替换 `resolvedLauncherRuntime` / `launcherRuntime` 的混合推导；`launcherProviders`、`launcherProvider`、`effectiveLauncherModel` 改为依赖 `selectedRuntime` 与 provider。
- `src/renderer/components/AgentSettings/WorkspaceBasicsSection.tsx`：显示用 `selectedRuntime`，Antigravity 判断仍用执行身份。
- `src/shared/agentConfigMutation.ts`：provider 切换时按"锁内的 Agent 记录"一次解析（上游做法），保留默认值、托管设置和旧标识；固定 Claude 权限的 provider 用 builtin 的权限映射。我们已有 `getDefaultNewSessionPermissionMode`，保留它，不换回上游的 `getDefaultRuntimePermissionMode`。
- `src/shared/types/runtime.ts`：新增 `resolveAgentRuntimeSelection`，`resolveEffectiveRuntime` 复用同一个投影函数（上游已有实现，可直接移植）。
- `src/server/admin-api.ts`：`agent set runtime / providerId` 走同一个锁内的 Agent 变更（上游把两条分支合成 `resolveAgentConfigMutation`）。

### 3.2 不改什么

- Session 出生后的运行环境、已存在 Session 的绑定。
- 我们的默认模型继承本身（来源与优先级不变）。
- 运行环境选择器的外观和位置（仍在工具栏）。

## 4. 验收

自动化（沿用并补全上游 `8955cfe` 的测试）：
- 两种选择顺序：先选 Codex 模型再选 DSH，和先选 DSH 再选 Codex 模型：保存值与显示值都是 DSH / 都不变，执行身份是托管 Codex。
- 回到可移植模型：执行身份回到基础选择。
- 固定 Claude 的 provider（`anthropic-sub`、`xai-sub`）：权限列表与默认值按 builtin；基础选择不被改写。
- **AgenTeam 特有**：默认模型继承指向一个在当前基础运行环境下不兼容的 provider 时，被忽略且不改变运行环境。
- `admin-api` 的 `agent set runtime / providerId` 与桌面选择器写出同样的 Agent 记录。

需要真实应用（独立验收，不是自检）：
- 在启动页依次点上述几种选择，截图或记录显示值。
- 带着默认模型继承配置（配置里有 `defaultModelSelection`）重复一遍。

## 5. 风险与顺序

- **风险中等**：启动页是最常用入口，且 AgenTeam 与上游的差别正在这里。
- **顺序**：先在 `Launcher.tsx` 之外落地共享函数和 `agentConfigMutation` 的变更（可单测），再改启动页，最后改设置页显示。每步单独 CI 全绿。
- 本机无法验证界面，必须有独立验收。

## 6. 需要用户确认

1. 默认模型继承与运行环境冲突时"忽略默认模型、不改运行环境"，是否接受？（替代方案：默认模型优先，自动切换运行环境——这是现在 AgenTeam 的隐含行为，也是上游要消除的那类不一致。）
2. 是否要把 `executionRuntime` 在界面上显示出来（例如模型选择器旁标注"由 Codex 执行"），还是保持现在不显示？
