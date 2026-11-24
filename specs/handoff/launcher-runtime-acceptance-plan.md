# 启动页运行环境显示：独立验收计划（真实应用部分）

> 日期：2026-11-23。对象：`main`（含 `a7fead6` 及之后）。设计见 `launcher-runtime-selection-design.md`。
> 规则：隔离 HOME、不改产品代码、🔒 用例找违反证据、不能运行写"阻塞"、证据用截图或日志而不含真实个人信息。

## 已由自动化覆盖（不需要重复）
`src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx`（真实 Launcher 组件 + 模拟后端，jsdom）已验证：
L1 偏好 DSH 选 Codex 模型→选择器显示 Codex、弹一次 toast、**没有常驻标注**、偏好不被改写；
L2 换回 DSH 可用模型→回到 DSH、弹提示；L4 被模型钉住时点已保存的运行环境→说明性 toast、不改配置；
L6 默认模型继承为 Codex→选择器显示执行者、不弹 toast、无标注；L8 外部 CLI 偏好按保存值显示。
`runtimeProviderProjection.test.ts` 覆盖 antigravity-sub 隐藏、`describeExecutionRuntime`。
这些是 jsdom 结果，不代表真实应用。你只需在真实应用里验证下面的项目。

## 真实应用用例
### L-R1 视觉与文案
在真实桌面 App（中文与英文各一次）重做 L1、L2：截图选择器与 toast。检查：toast 出现位置与时长合理、文案无占位符残留（如 `{{runtime}}`）、选择器文字不被截断到无法辨认、深色与浅色主题可读；输入框工具栏上不应再有"由 X 执行"之类的常驻文字。
### L-R2 真实执行者（🔒）
偏好 DSH，选托管 Codex 模型，发送一条消息。预期：Session 由 Codex 执行（看会话详情或日志里的 runtime），而不是 DSH；换回 DeepSeek 模型后新建 Session 由 DSH 执行。偏好在 `config.json` 里始终是 DSH。
### L-R3 已有 Session 不被重新推导（🔒）
打开一个已存在的 Codex Session，回到启动页改偏好与模型，再回到该 Session：它的运行环境不变。
### L-R4 设置页（L9）
工作区设置页的运行环境选择器显示**偏好**（DSH），不随启动页选的模型变化。
### L-R5 默认模型继承（真实配置）
`config.json` 里设 `defaultModelSelection` 指向 Codex 模型，重启后多次打开启动页：选择器显示 Codex，**从不弹 toast**，也没有常驻标注。
### L-R6 管理接口一致
`myagents agent set runtime dsh` 与桌面选择器写出的 Agent 记录一致（对比 `config.json` 里的 runtime、runtimePreference 字段）。
### L-R7 平台差异
Linux 上托管 Codex 订阅不显示（预期）；Windows / macOS 显示。antigravity-sub 所有平台都不显示。
### L-R8 回归
Claude / Codex 外部 CLI 偏好的启动页行为、IM 来源会话、不带 Agent 的普通项目启动页不变。

## 交付
分支 `acceptance/launcher-runtime`，文件 `specs/handoff/acceptance/launcher-runtime/REPORT.md`：结果表（通过 / 失败 / 阻塞、证据、备注）、新缺陷、App 版本与平台、结论。不要改 `main`，不要创建 PR。

## 更正（2026-11-24）：配置字段判据
订阅路径验收曾按"runtime、runtimePreference、providerId 三个字段在模型选择前后都不变"判失败，这个判据本身错了。正确判据：`runtimePreference` 和设置页显示不变；`runtime` 在选 Managed Codex 模型时变为 `builtin`（托管载体），换回后恢复；`providerId` 随所选模型变化。详见 launcher-runtime-selection-design.md 第 7 节。运行环境菜单里被标记的行必须是保存的运行环境（DSH），而不是外部 Codex CLI。
