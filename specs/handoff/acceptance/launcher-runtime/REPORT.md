# 启动页运行环境显示独立验收

验收日期 2026-11-23（太平洋时间）。仓库为 `micsnq2yoma-lgtm/AgenTeam`，验收基线为远端 `main` 的 `a7fead6695c6b46f73056eedb3960278653625ff`，含实现提交 `74818bfd`。验收依据为[启动页设计第 3、4 节](../../launcher-runtime-selection-design.md#3-目标设计用户-2026-11-23-已拍板)、[通用验收规则](../../assistant_profile/acceptance-plan.md#0-规则)及本次 L1–L12 的具体预期。产品源码未修改。

## 用例结果

| 用例 | 结果 | 证据 | 备注 |
| --- | --- | --- | --- |
| L1 基础场景 | 阻塞 | 环境证据 E1、E2 | 未能在全新助理上选择 DSH 与有效订阅模型。Managed Codex 名称、通知、常驻标注和偏好均未实测。 |
| L2 回到偏好 | 阻塞 | E1、E2 | 缺少 L1 的桌面状态，未实测切回兼容模型、通知与标注消失。 |
| L3 🔒 往返与重启 | 阻塞 | E1、E2 | 未执行三次往返及重启，无 `runtime`、`runtimePreference` 或 `providerId` 的逐次对比。 |
| L4 点击已保存的 DSH | 阻塞 | E1、E2 | 未在 Managed Codex 执行状态下点选 DSH，无法核对通知和配置无写入。 |
| L5 主动改偏好 | 阻塞 | E1 | 无法在隔离桌面上点击 Claude Agent SDK 或其他运行环境并比较写盘结果。 |
| L6 默认模型继承 | 阻塞 | E1、E2 | 未能配置有效的 Codex 订阅默认模型。重新打开时的标注、静默次数和换回 DSH 的表现未观察。 |
| L7 固定 Claude 的订阅 | 阻塞 | E1、E2 | 无可用的隔离 Anthropic 或 Grok 订阅登录，未实测模型选择、Claude 执行标签与权限菜单。 |
| L8 外部 CLI 偏好 | 阻塞 | E1、E3 | 宿主能发现 Claude Code 和 Codex 命令，隔离 App 内的检测、换模型行为和通知未实测。 |
| L9 设置页 | 阻塞 | E1、E2 | 无法在启动页改模型后打开助理设置，与配置中的 DSH 偏好交叉核对。 |
| L10 实际执行 | 阻塞 | E1、E2 | 未发送测试消息，没有会话运行环境、模型或新聊天页的执行显示证据。 |
| L11 命令行一致性 | 阻塞 | E1、E2 | 没有隔离 App 的临时助理和对应的 CLI 路由，未运行两条 `myagents agent set`，无桌面与 CLI 写盘对比。权限继承的已知差异未计作缺陷。 |
| L12 回归 | 阻塞 | E1、E3 | Claude Agent SDK 与普通项目均未进入隔离桌面验证。宿主 PATH 未找到 `agy`。 |

## App 版本、账号状态和阻塞证据

- **E1 · 桌面构建与隔离。** 目标提交的应用版本为 `0.6.8`。公开的同版本 Windows 安装包对应 `91836a075f48002f9c43c694d24230d35535a6b7`。执行 `git merge-base --is-ancestor 74818bfd 91836a07` 返回 1，GitHub Actions 也未查到 `a7fead66` 的 Windows 构建产物。同版本现有 App 未用于验收。运行 `pwsh -NoProfile -File ./build_dev_win.ps1` 后，固定的 Rust、MSVC 与 Node.js/npm 前置检查通过。构建停留在 `npm install --no-audit --no-fund` 阶段。确认没有可用的全新交互桌面后终止构建，未生成目标调试 exe。[公开发布包](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/tag/v0.6.8)
- **E1 · 隔离入口。** Windows 壳通过 `dirs::home_dir()` 与 Windows Profile Known Folder 定位 `.agenteam`。只覆盖进程的 `HOME` 或 `USERPROFILE` 无法隔离原生端的配置。现有专用 Windows 测试用户已有应用配置，且当前没有它的交互桌面。未登录该用户或复用其配置。WSLg 可用，但现有 Ubuntu 20.04 缺少 `webkit2gtk-4.1` 和 Cargo，Docker 守护进程未运行。即使完成 Linux 构建，目标提交也在 Linux 隐藏 `codex-sub`。未找到可直接启动的全新隔离桌面。[桌面数据目录实现](../../../../src-tauri/src/app_dirs.rs) · [Linux provider 规则](../../../../src/shared/runtimeProviderProjection.ts)
- **E2 · 订阅。** 本次隔离验收没有可用并经验证的订阅登录。Managed Codex（Codex 订阅）、Anthropic 订阅及 Grok 订阅均未验证。未读取宿主的登录凭据，也未用其他 provider 替代。
- **E3 · 外部运行环境。** 宿主 PATH 可找到 `claude` 和 `codex`，未找到 `agy`。这些是命令发现结果，隔离 App 的运行环境检测和 CLI 登录状态均未验证。

未创建验收助理或普通项目，因此没有目标 Agent 的 `config.json` 前后记录，也没有能对应到验收用例的桌面截图。

## 通知文案记录

运行时观察为 **0 次**。下表是目标提交的本地化文案，已代入对应运行环境名称。重复次数、常驻标注的位置、截断和遮挡均未实测。推理档位变化时还有单独的条件通知，实际是否与切换通知同屏未观察。[简体中文](../../../../src/renderer/i18n/locales/zh-CN/launcher.json) · [英文](../../../../src/renderer/i18n/locales/en-US/launcher.json) · [运行环境命名](../../../../src/renderer/components/runtimePresentation.ts)

| 场景 | 简体中文 | English |
| --- | --- | --- |
| L1 切到 Managed Codex | 这个模型只能由 Managed Codex 运行，本次将用 Managed Codex 执行；你保存的运行环境仍是 AgenTeam (DeepSeek Harness)。 | This model only runs on Managed Codex, so Managed Codex will run this message. Your saved choice is still AgenTeam (DeepSeek Harness). |
| L2 回到保存的 DSH | 已回到你保存的运行环境：AgenTeam (DeepSeek Harness)。 | Back to AgenTeam (DeepSeek Harness), your saved Runtime. |
| L4 点已保存的 DSH | 你保存的运行环境已经是 AgenTeam (DeepSeek Harness)，但当前选的模型只能由 Managed Codex 运行。换一个模型即可使用 AgenTeam (DeepSeek Harness)。 | Your saved Runtime is already AgenTeam (DeepSeek Harness), but the selected model only runs on Managed Codex. Pick another model to use AgenTeam (DeepSeek Harness). |
| L7 切到 Claude Agent SDK | 这个模型只能由 AgenTeam (Claude Agent SDK) 运行，本次将用 AgenTeam (Claude Agent SDK) 执行；你保存的运行环境仍是 AgenTeam (DeepSeek Harness)。 | This model only runs on AgenTeam (Claude Agent SDK), so AgenTeam (Claude Agent SDK) will run this message. Your saved choice is still AgenTeam (DeepSeek Harness). |
| L1、L6 常驻标注 | 由 Managed Codex 执行 | Run by Managed Codex |
| 模型切换同时重置推理档位 | 原推理档位不适用于此模型，已切换为模型默认。 | The previous reasoning effort is unavailable for this model. Using the model default. |

## 新发现的缺陷

无可登记的桌面缺陷。未取得可复现的桌面操作、实际结果和截图，严重程度无法判定。

## 结论

**不通过（验收未完成）**。0 项通过、0 项失败、12 项阻塞。启动页的运行环境切换、偏好持久化和 Session 实际执行均缺少真实桌面证据，当前无法给出通过或有条件通过的结论。
