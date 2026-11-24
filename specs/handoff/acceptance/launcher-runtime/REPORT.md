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


## 复验记录 · Windows · d4d8615 · 2026-11-23

**订阅前置检查。隔离账号没有 Managed Codex 订阅，L1、L2、L3、L4、L6、L10 阻塞。** DeepSeek API 密钥、Anthropic 订阅和 Grok 订阅也缺失。实测继续覆盖可用的桌面路径。

### 构建和隔离

- 测试提交 `d4d8615f6a14768e1ac621f69965b4a70ff841ed`，从 `main` 单独检出。`git merge-base --is-ancestor d4d8615 d4d8615f6a14768e1ac621f69965b4a70ff841ed` 返回 0。`47ffb9f`、`845f9e2` 对该提交的祖先检查均返回 1，U3、U5 不适用。
- Windows 11 26H2 x64（10.0.26300.9974）。通过 `build_dev_win.ps1` 准备原生资源，运行类型检查及 Web、Sidecar、Bridge 和 CLI 打包。使用仓库锁定的 Rust 1.92.0、Node 24.20.0、npm 11.19.0，以及本轮安装的 MSVC 14.44 和 Windows SDK 10.0.26100。Tauri 以 `--debug --no-bundle --target x86_64-pc-windows-msvc` 编译。前端位于真实检出路径，Cargo 缓存和编译输出使用临时短路径，避开解包及 Opus 的 MSBuild 路径错误。
- E1 隔离使用新建的 Windows 本地标准测试用户，Secondary Logon 加载其 Windows Profile。启动前验证原生 Known Folder 下的 `.agenteam` 不存在，首次正常启动后在同一目录生成配置。原始 Tauri 应用标识的候选进程退出 0，没有独立窗口。为与当时桌面上的另一个实例隔离，最终调试构建使用临时 `--config` 覆盖 Tauri 标识为 `io.github.micsnq2yoma-lgtm.agenteam.launcherqa`。编译源码来自同一提交，最终编译退出 0。候选二进制 SHA-256 为 `c3717c63c5debaca3b8501dc5b2ca25661f867a386baa9bbd5481421b83cc0fa`。
- 候选在真实 Windows 桌面运行，关于页显示 **0.6.8**。这是目标源码内的版本号。验收使用上面的本地调试构建。项目和数据均是本轮新建的合成测试数据。配置证据按 `runtime`、`runtimePreference` 和 `providerId` 投影。[关于页](evidence-d4d8615/U1-about-zh-light.jpg)

| 前置项 | 本轮确认 |
| --- | --- |
| DeepSeek API 密钥 | 缺失。设置卡片显示「输入 API Key」，DSH 启动页的模型菜单提示先设置模型服务。 |
| Managed Codex 订阅 | 缺失。Windows 模型设置显示 ChatGPT（订阅）及「下载 Codex Runtime」。测试 Profile 无可用订阅登录。 |
| Anthropic 订阅 | 缺失。设置显示 Claude 账号未登录。 |
| Grok 订阅 | 缺失。设置显示 Grok 账号未登录。 |
| 宿主 `claude`、`codex` 命令 | 两条均可发现。仅复制程序文件进独立夹具，测试用户也能发现它们，启动页显示两条可选的外部 CLI。未使用宿主登录。 |

[隔离账户的模型设置](evidence-d4d8615/credentials-fresh-profile.jpg) · [DSH 模型菜单](evidence-d4d8615/L5-dsh-no-model.jpg)

### L1–L12 与 U1–U5

| 用例 | 结果 | 实测和证据 |
| --- | --- | --- |
| L1 DSH → Managed Codex | 阻塞 | 缺少 Managed Codex 订阅。DSH 偏好已建立。未取得由订阅模型触发的执行者、通知、工具栏和磁盘对比。 |
| L2 回到 DSH | 阻塞 | 缺少 L1 状态及 DeepSeek API 密钥。 |
| L3 🔒 三次往返及重启 | 阻塞 | 缺少两个可用模型端点，未执行托管模型往返。已核对外部 Codex CLI 在普通重启后的偏好，见 L8。 |
| L4 点击已保存的 DSH | 阻塞 | 缺少被 Managed Codex 订阅模型限制的执行状态。 |
| L5 主动选择才更新偏好 | 通过 | 桌面上主动选择 DSH、Claude Code CLI、Codex CLI 都改变保存值。两次 CLI 内换模型后，三个字段均保持各自的前值。DSH 模型菜单因缺少密钥无法选模型。[DSH 选择结果](evidence-d4d8615/L5-dsh-after-selector.jpg) |
| L6 默认模型继承 | 阻塞 | 没有可继承的 Managed Codex 订阅模型，未测静默重开、重启及无常驻标注。 |
| L7 固定 Claude 的订阅 | 阻塞 | Anthropic 与 Grok 均未登录，未测订阅模型执行器和权限映射。 |
| L8 外部 Claude Code 和 Codex CLI | 通过 | 两个选择器都按保存值显示。Claude Code 从「默认」选 Sonnet，Codex CLI 从 GPT-6.1-Sol 选 GPT-6-Astra，各观察到 0 条运行环境切换通知。两次模型选择留下的三个设置字段均不变。重启后重新选择该项目，仍显示 Codex CLI、GPT-6-Astra。[Claude Code 前](evidence-d4d8615/L8-claude-cli-default.jpg) · [后](evidence-d4d8615/L8-claude-cli-sonnet.jpg) · [Codex CLI 前](evidence-d4d8615/L8-codex-cli-default.jpg) · [后](evidence-d4d8615/L8-codex-cli-astra.jpg) · [重启后](evidence-d4d8615/L8-codex-cli-after-restart.jpg) |
| L9 Agent 设置显示偏好 | 阻塞 | 可执行部分通过。换到 GPT-6-Astra 后，Agent 设置仍显示磁盘保存的 Codex CLI。改回 DSH 后，设置显示磁盘保存的 DSH。缺少 Managed Codex 订阅，无法测 L-R4 的 DSH 偏好与托管执行者同时不一致的状态。[Codex CLI](evidence-d4d8615/L9-codex-preference-after-model.jpg) · [DSH](evidence-d4d8615/L9-dsh-preference-settings.jpg) |
| L10 🔒 实际执行 | 阻塞 | 缺少 Managed Codex 订阅及 DeepSeek API 密钥。未发送测试消息，既有 Codex Session 的绑定也无可验实例。 |
| L11 命令行写盘一致性 | 阻塞 | 桌面写盘已验证。隔离用户的普通终端两条 `myagents agent set <id> runtime dsh` 和 `myagents agent set <id> providerId deepseek` 均退出 2，`Unknown public command`。公开 CLI 的既有准入清单未开放 `agent set`，本轮没有带 App 注入的内部 CLI 能力的可执行上下文。两次拒绝未改三个字段。 |
| L12 Claude Agent SDK 和普通项目 | 阻塞 | 可执行部分通过。新建助理和关闭助理模式的新建普通项目均显示 Claude Agent SDK，工具栏可操作。普通项目会自动创建基础 Agent 记录，无法通过正常新建保持「无 Agent 记录」以验证兼容路径。[新助理](evidence-d4d8615/L12-assistant-sdk-startup.jpg) · [关闭助理模式](evidence-d4d8615/L12-plain-assistant-off.jpg) · [普通项目](evidence-d4d8615/L12-plain-project-zh-light.jpg) |
| U1 关于页 | 通过 | 中文和英文各看一次。0.6.8 版本旁分别只见「检查更新」和 Check for Updates，未见更新记录按钮。[中文](evidence-d4d8615/U1-about-zh-light.jpg) · [英文](evidence-d4d8615/U1-about-en-light.jpg) |
| U2 设备组网 | 阻塞 | 左侧栏入口已移除，设置分区可达并呈现未禁用的「自建 AgentNet」开关。使用合成的旧 `agentnetwork` 标签文件重启后，进入启动页，未打开该标签或出现可见错误。缺少真实旧 Profile 的已开标签记录，未开启设备间调用或接入网络核对写入。[设置](evidence-d4d8615/U2-device-network-settings.jpg) · [合成状态重启](evidence-d4d8615/U2-launcher-after-legacy-restart.jpg) |
| U3 项目控件 | 不适用 | 目标构建不含 `47ffb9f`。 |
| U4 中英文、浅深主题和通知 | 通过 | 四个外观组合均在真实启动页核对。选择器可辨，输入框周围未见常驻执行标注、遮挡或未替换占位符。用合成项目归档及恢复触发三条真实通知，位于窗口顶部居中，文字完整。托管模型的长通知仍受 L1、L2 阻塞。[中浅](evidence-d4d8615/launcher-zh-light-default.jpg) · [中深](evidence-d4d8615/U4-launcher-zh-dark.jpg) · [英浅](evidence-d4d8615/U4-launcher-en-light.jpg) · [英深](evidence-d4d8615/U4-launcher-en-dark.jpg) |
| U5 新助理介绍页 | 不适用 | 目标构建不含 `845f9e2`。 |

U2 的合成 `open-tabs.json` 使用 `version: 1`、`view: "agentnetwork"`，SHA-256 为 `ccd909cef57390be88db9b69301ce5326bc480391d13e853d0d0082815285708`。重启前让测试实例完整退出，再保留并移开该测试实例的干净退出标记以进入恢复判断路径。代码比对显示清理改动前后 `agentnetwork` 的 tab 模块都未注册持久化 codec。这个合成输入不能证明历史版本曾保存该类标签。[标签模块](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/d4d8615f6a14768e1ac621f69965b4a70ff841ed/src/renderer/features/agent-network/tabModule.tsx)

验收计划 L-R7 只观察 Windows，ChatGPT（订阅）入口可见。Linux 和 macOS 未运行。外部 Antigravity CLI 的安装入口与模型供应商 `antigravity-sub` 属于不同对象。L-R8 的外部 CLI 路径见 L8，未配置 IM 来源测试会话。补充执行的 `npm run test:dom -- src/renderer/pages/Launcher.runtimeSelection.dom.test.tsx` 为 5/5 通过，独立于真实桌面结果。

### config.json 前后对照

「缺字段」表示 JSON 中没有该键。下表只列合成助理的三个字段。普通项目创建后另有自己的基础 Agent，选择 DSH 后写入其自身的 DSH 偏好，选择回 Claude Agent SDK 后恢复 `runtime=builtin` 与 `{"family":"integrated","id":"claude-agent-sdk"}`。两个项目的设置互不覆盖。

| 步骤 | `runtime` | `runtimePreference` | `providerId` |
| --- | --- | --- | --- |
| 新助理保存后，主动改之前 | 缺字段 | 缺字段 | 缺字段 |
| 启动页手动选择 DSH | `dsh` | `{"family":"integrated","id":"dsh"}` | 缺字段 |
| 启动页手动选择 Claude Code CLI | `claude-code` | `{"family":"external","id":"claude-code"}` | 缺字段 |
| 换为 Sonnet 后 | `claude-code` | `{"family":"external","id":"claude-code"}` | 缺字段 |
| 启动页手动选择 Codex CLI | `codex` | `{"family":"external","id":"codex"}` | 缺字段 |
| 换为 GPT-6-Astra 后 | `codex` | `{"family":"external","id":"codex"}` | 缺字段 |
| 两条普通终端命令拒绝后及重启后 | `codex` | `{"family":"external","id":"codex"}` | 缺字段 |
| 再次主动选 DSH，Agent 设置打开后 | `dsh` | `{"family":"integrated","id":"dsh"}` | 缺字段 |

### 通知文案和实测次数

下表沿用上一轮的双语文案，并按当前设计移除常驻标注的预期。运行时实测以窗口中的可读记录计数，模型路径未触发表示 0 次观察，符合性未验证。两次外部 CLI 换模型已触发操作，观察到的 0 是实测结果。

| 场景 | 简体中文 | English | 运行时观察和一致性 |
| --- | --- | --- | --- |
| L1 切到 Managed Codex | 这个模型只能由 Managed Codex 运行，本次将用 Managed Codex 执行；你保存的运行环境仍是 AgenTeam (DeepSeek Harness)。 | This model only runs on Managed Codex, so Managed Codex will run this message. Your saved choice is still AgenTeam (DeepSeek Harness). | 0 次，缺订阅，文案未比较 |
| L2 回到保存的 DSH | 已回到你保存的运行环境：AgenTeam (DeepSeek Harness)。 | Back to AgenTeam (DeepSeek Harness), your saved Runtime. | 0 次，缺订阅和密钥，文案未比较 |
| L4 点已保存的 DSH | 你保存的运行环境已经是 AgenTeam (DeepSeek Harness)，但当前选的模型只能由 Managed Codex 运行。换一个模型即可使用 AgenTeam (DeepSeek Harness)。 | Your saved Runtime is already AgenTeam (DeepSeek Harness), but the selected model only runs on Managed Codex. Pick another model to use AgenTeam (DeepSeek Harness). | 0 次，缺订阅，文案未比较 |
| L6 继承 Codex 默认模型 | 应静默，无常驻文字 | Silent, with no persistent label | 0 次，缺有效默认模型，静默未验证 |
| L7 切到 Claude Agent SDK | 这个模型只能由 AgenTeam (Claude Agent SDK) 运行，本次将用 AgenTeam (Claude Agent SDK) 执行；你保存的运行环境仍是 AgenTeam (DeepSeek Harness)。 | This model only runs on AgenTeam (Claude Agent SDK), so AgenTeam (Claude Agent SDK) will run this message. Your saved choice is still AgenTeam (DeepSeek Harness). | 0 次，缺订阅，文案未比较 |
| 换模型同时重置推理档位 | 原推理档位不适用于此模型，已切换为模型默认。 | The previous reasoning effort is unavailable for this model. Using the model default. | 0 次，两次换模型无可见重置通知，文案未比较 |
| L8 外部 CLI 内换模型 | 运行环境未变，应静默 | Runtime unchanged, no switch toast | Claude Code 0/1、Codex CLI 0/1，符合预期 |

U4 的通用通知另计。深色中文「已归档项目」1 次、「已取消归档」1 次，浅色英文「Project unarchived.」1 次，与各自本地化文案一致。通知在 1203 × 831 的截图中距窗口顶边约 60–65 px，水平居中，与输入框分离，观察到的文字无截断和占位符。这些是布局证据。[中文归档](evidence-d4d8615/U4-toast-zh-dark-archive.jpg) · [中文恢复](evidence-d4d8615/U4-toast-zh-dark-unarchive.jpg) · [英文恢复](evidence-d4d8615/U4-toast-en-light-unarchive.jpg)

### 新缺陷、阻塞和结论

已走通的路径未发现可复现的新产品缺陷。阻塞来源包括四项模型凭据缺失、内部 CLI 执行上下文不可用，以及无 Agent / 历史设备组网会话的真实夹具无法通过新建项目获得。对应的部分验证见结果表。

**验收未完成。4 项通过，0 项失败，11 项阻塞，2 项不适用。** 主动选择写盘、外部 CLI、关于页和双语浅深外观已取得桌面证据。Managed Codex 的实际切换、继承、通知、运行环境与 Session 绑定仍未验证。

## L7 固定 Claude（0.7.0）

验收日期为 2026-11-23（太平洋时间）。目标 App 为 AgenTeam **0.7.0 Windows x64 正式版**。发布标签 `v0.7.0` 和发布目标均指向 `5c533ea28f095690810c1a11b0da924c9cd5f251`，其父提交为 `daf4e223e5fe08db716d319962ca46ae7932c63c`。目标版本来自正式发布记录和源码的 Tauri 版本字段，本轮未在隔离桌面启动该 App，因而没有实测的「关于」页版本。 [正式发布包](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/tag/v0.7.0) · [目标提交](https://github.com/micsnq2yoma-lgtm/AgenTeam/commit/5c533ea28f095690810c1a11b0da924c9cd5f251)

### 隔离及前置条件证据

- **L7-E1 · Windows Profile。** 0.7.0 原生端调用 `dirs::home_dir()` 定位 `.agenteam`，隔离需要独立 Windows 用户的原生 Profile 或独立机器。本轮没有可使用并可确认独占 `.agenteam` 的交互桌面。当前进程没有管理员权限，本机未检出 Windows Sandbox 可执行文件。上一轮报告中的独立用户、0.6.8 构建及截图仅用于历史参照。本轮未启动宿主实例、未读取宿主配置或登录凭据，也未创建合成助理。 [0.7.0 数据目录实现](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src-tauri/src/app_dirs.rs#L113) · [上一轮隔离设置截图](evidence-d4d8615/credentials-fresh-profile.jpg)
- **L7-E2 · Anthropic。** 本轮没有可用于独立测试账号的 Anthropic 订阅登录或 Anthropic API Key。两者任一即可开展主用例。没有对应的 0.7.0 设置页可用状态截图，没有使用其他 provider 代替。
- **L7-E3 · DeepSeek 与 Grok。** 本轮未提供可用于隔离验收的 DeepSeek API Key，S4 单独受此限制。也没有可用于隔离验收的 Grok 订阅，选测 S8 记为不适用。
- **L7-E4 · 源码对照。** 复核了设计第 3、4 节、`projectProvidersForRuntime` 与 `describeExecutionRuntime` 的注释和实现，以及上一轮的通知文案表。0.7.0 的 `providerRequiresClaudeSdk` 只匹配 `anthropic-sub` 和 `anthropic-api`。Grok 订阅 `xai-sub` 使用可移植的 OAuth 与 OpenAI Responses，可在 DSH 偏好下由 DSH 执行。设计第 4 节与 `describeExecutionRuntime` 旁的注释曾把 Grok 一并列入，验收按实现判定。DSH 下的 Anthropic 菜单过滤及已选 provider 例外也已在源码中定位。 [设计文档](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/specs/handoff/launcher-runtime-selection-design.md#3-目标设计用户-2026-11-23-已拍板) · [提供方约束](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/shared/integrated-runtimes/provider-constraints.ts#L37) · [运行环境投影](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/shared/runtimeProviderProjection.ts#L25)

### S1–S9 结果

“未采集”表示没有触发该步骤的桌面操作，因此截图、通知原文及次数均无实测记录。未测次数不能作为 0 条通知的证据。

| 步骤 | 结果 | 证据位置与原因 | 截图 / 通知原文 / 次数 |
| --- | --- | --- | --- |
| S1 Claude Agent SDK 基线 | 阻塞 | L7-E1、E2。未选 Anthropic 模型，未观察选择器、权限菜单、默认值或基线静默。 | 均未采集 |
| S2 主动切到 DSH 偏好 | 阻塞 | L7-E1、E2。缺少 S1 的已选模型状态。各项实际行为见下表。 | 均未采集 |
| S3 再点已保存的 DSH | 阻塞 | L7-E1、E2。S2 未执行，无法判断触发条件是否成立，也未核对配置是否无写入。 | 均未采集 |
| S4 切到 DeepSeek | 阻塞 | L7-E1、E2、E3。缺少 S2 状态和 DeepSeek API Key，未观察回到 DSH、Anthropic 隐藏、通知或 DSH 权限。 | 均未采集 |
| S5 实际执行（🔒） | 阻塞 | L7-E1、E2。两个偏好场景均未发送消息，没有真实执行环境、模型名、会话详情或新聊天页显示的证据。 | 均未采集 |
| S6 权限（🔒） | 阻塞 | L7-E1、E2、E3。未观察 builtin 与 DSH 的菜单差异、默认值、新会话默认权限或 builtin 的无人值守最大权限 `fullAgency`。 | 均未采集 |
| S7 完全退出并重启（🔒） | 阻塞 | L7-E1、E2。没有可恢复的隔离助理与 Anthropic 选择状态，未做重启前后对照。 | 均未采集 |
| S8 Grok 对照（选测） | 不适用 | L7-E3、E4。缺少隔离 Grok 订阅，DSH 显示、执行和静默均无桌面观察。 | 均未采集 |
| S9 已有 Session 绑定 | 阻塞 | L7-E1、E2。没有可重开的隔离 Anthropic 会话，未观察改偏好和模型后其运行环境、模型与权限是否保持。 | 均未采集 |

### S2 实际行为记录

S2 未触发。模型保留和清除均无桌面观察，未就任一分支判失败。

| 观察项 | 实际记录 |
| --- | --- |
| 偏好 | `runtime`、`runtimePreference` 前后均未采集，无法确认有无保存 DSH。 |
| 模型 | `providerId` 和模型未采集，无法判断是否保留、清除或替换。 |
| 选择器与常驻文字 | 未观察选择器显示、实际执行者或是否出现常驻「由 X 执行」。 |
| 通知 | 无实测原文及次数。 |
| 权限 | 未打开菜单，未观察选项或默认值。 |

### config.json 逐步对照

仅记录该测试助理的三个字段。没有生成该助理的配置记录，表中“未采集”也不代表字段缺失、前后相同或未发生写盘。每列依次为步骤前、步骤后。

| 步骤 | `runtime` 前 → 后 | `runtimePreference` 前 → 后 | `providerId` 前 → 后 |
| --- | --- | --- | --- |
| S1 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |
| S2 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |
| S3 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |
| S4 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |
| S5 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |
| S6 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |
| S7 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |
| S8 | 不适用 | 不适用 | 不适用 |
| S9 | 未采集 → 未采集 | 未采集 → 未采集 | 未采集 → 未采集 |

### 通知文案对照

以下简体中文来自 0.7.0 的本地化模板，已代入运行环境名称供对照。实际显示文本、次数及占位符替换均未实测。 [0.7.0 通知模板](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/renderer/i18n/locales/zh-CN/launcher.json#L20)

| 场景 | 文案或预期 |
| --- | --- |
| S1 在 Claude Agent SDK 偏好下选 Anthropic | 预期 0 条运行环境切换通知。 |
| S3 模型保留时再点已保存的 DSH | 预期 1 条。「你保存的运行环境已经是 AgenTeam (DeepSeek Harness)，但当前选的模型只能由 AgenTeam (Claude Agent SDK) 运行。换一个模型即可使用 AgenTeam (DeepSeek Harness)。」 |
| S4 从 Anthropic 切回 DSH 可用模型 | 预期 1 条。「已回到你保存的运行环境：AgenTeam (DeepSeek Harness)。」 |
| S8 在 DSH 偏好下选 Grok | 预期 0 条运行环境切换通知。 |

### 新缺陷、阻塞与结论

暂无可登记的新桌面缺陷。没有实测复现、实际结果或截图支撑缺陷定级。已有的 Grok 文档与注释偏差已在 L7-E4 记录。

**L7 阻塞，验收未完成。0 项通过，0 项失败，8 项阻塞，1 项不适用。** 主用例缺少可确认独占 `.agenteam` 的桌面和可用的 Anthropic 订阅登录或 API Key。S4 另缺 DeepSeek API Key。尤其是 DSH 偏好下保留 Anthropic 模型时的真实执行者、权限和重启持久化，均无实测结论。

## 订阅路径验收（0.7.0）

验收环境为 Windows 11 x64（10.0.26200）上已安装的 AgenTeam 0.7.0。GitHub 最新发布 `v0.7.0` 指向 `5c533ea28f095690810c1a11b0da924c9cd5f251`，父提交为 `daf4e223e5fe08db716d319962ca46ae7932c63c`。本机 exe 的产品版本为 0.7.0，SHA-256 为 `17c909ebbf3544b4e9082329b454fe8cff7bdcba933e2d4b8cae43ac597472bc`。未重新安装或逐字节对比发布包。 [GitHub 发布](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/tag/v0.7.0)

按用户本轮授权，在当前 Windows 用户的真实 App 中新建合成助理 `LR-Subscription-QA`。工作区是新建的，`.agenteam` 与当前用户共用。模型设置显示 Codex Runtime v0.171.0（win32-x64）已登录，Codex 订阅模型可选。DeepSeek 已配置，卡片显示掩码和绿色勾。沿用 App 已配置的服务，未读取或导出登录凭据。常驻标注的判据采用本轮预期。 [Codex 状态](evidence-0.7.0-subscription/codex-modal.png) · [DeepSeek 状态](evidence-0.7.0-subscription/deepseek.png)

### S1–S8 结果

| 步骤 | 结果 | 实测及证据 |
| --- | --- | --- |
| S1 | 通过 | 主动点选后，启动页显示 DeepSeek Harness，Agent 设置显示 AgenTeam (DeepSeek Harness)。三个字段见下表。 [启动页](evidence-0.7.0-subscription/S1-zh-launcher.png) · [设置](evidence-0.7.0-subscription/S1-zh-settings.png) |
| S2 | 失败 | 选择 Codex 订阅组的 GPT-6.2-Sol 后，选择器完整显示 Managed Codex。中文顶部弹一次，打开选择器和设置未重弹，输入框周围无常驻标注。写盘将 `runtime` 改为 `builtin` 并填入 `providerId=codex-sub`。已切换到英文界面，锁屏发生在对合成助理执行英文切换前。 [切换通知](evidence-0.7.0-subscription/S2-zh-toast.png) · [通知消失后](evidence-0.7.0-subscription/S2-zh-after.png) |
| S3 | 失败 | Agent 设置中的运行环境仍为 DSH。打开设置本身无写入，但 S2 留下的两个字段与 S1 基线不一致。 [设置](evidence-0.7.0-subscription/S3-zh-settings.png) |
| S4 | 失败 | 换回 DeepSeek V4.1 Flash 后，选择器回到 DSH，弹一条返回通知。配置写入 `runtime=dsh`、`providerId=deepseek`，仍与 S1 基线不同。 [返回通知](evidence-0.7.0-subscription/S4-zh-toast.png) |
| S5 | 通过 | 重新点选 Codex 后，在受模型限制时点菜单里的 DSH。弹一次说明，选择器仍为 Managed Codex。点击前后三个字段与 `config.json` 修改时间相同。 [准备状态](evidence-0.7.0-subscription/S5-zh-pin.png) · [点 DSH 后](evidence-0.7.0-subscription/S5-zh-toast.png) |
| S6 🔒 | 失败 | 三次往返都复现 `runtime` 切到 `builtin` 又写回 `dsh`，`providerId` 在 `codex-sub` 和 `deepseek` 间反复写入。完全退出后的复验仍阻塞。 [第 1 次去](evidence-0.7.0-subscription/S6-zh-1-codex.png) · [回](evidence-0.7.0-subscription/S6-zh-1-dsh.png) · [第 2 次去](evidence-0.7.0-subscription/S6-zh-2-codex.png) · [回](evidence-0.7.0-subscription/S6-zh-2-dsh.png) · [第 3 次去](evidence-0.7.0-subscription/S6-zh-3-codex.png) · [回](evidence-0.7.0-subscription/S6-zh-3-dsh.png) |
| S7 🔒 | 阻塞 | Windows 会话锁屏，未发送两条验收消息，缺少会话级运行环境、模型及既有绑定的执行证据。 |
| S8 | 阻塞 | Windows 会话锁屏，未进入默认模型配置流程。当前 App 进程托管本次验收会话，完全退出和恢复必须跨会话接续。 |

### 通知原文和次数

| 代号 | 中文实测原文 | 英文原文 |
| --- | --- | --- |
| A | 这个模型只能由 Managed Codex 运行，本次将用 Managed Codex 执行；你保存的运行环境仍是 AgenTeam (DeepSeek Harness)。 | 未采集 |
| B | 已回到你保存的运行环境：AgenTeam (DeepSeek Harness)。 | 未采集 |
| C | 你保存的运行环境已经是 AgenTeam (DeepSeek Harness)，但当前选的模型只能由 Managed Codex 运行。换一个模型即可使用 AgenTeam (DeepSeek Harness)。 | 未采集 |

| 步骤 | 中文实测 | 英文实测 |
| --- | --- | --- |
| S1 | 0 条运行环境通知 | 未执行 |
| S2 | A × 1，后续打开菜单、设置未重弹 | 阻塞，未执行 |
| S3 | 0 条运行环境通知 | 未执行 |
| S4 | B × 1 | 未执行 |
| S5 | A × 1（建立受限状态），C × 1（点击 DSH） | 未执行 |
| S6 | 准备时 B × 1，正式三轮各 A × 1、B × 1 | 未执行 |
| S7 | 未执行 | 未执行 |
| S8 | 未执行 | 未执行 |

已观察到的中文通知均无残留占位符。英文次数留空的步骤未通过本地化文件推定界面结果。

### config.json 逐步对照

「缺字段」表示键不存在。D 表示 `{"family":"integrated","id":"dsh"}`。除 S5 的文件修改时间外，仅记录测试助理的三个字段。

| 步骤 | `runtime` 前 → 后 | `runtimePreference` 前 → 后 | `providerId` 前 → 后 |
| --- | --- | --- | --- |
| S1 主动选择 DSH | 缺字段 → `dsh` | 缺字段 → D | 缺字段 → 缺字段 |
| S2 中文选择 Codex | `dsh` → `builtin` | D → D | 缺字段 → `codex-sub` |
| S2 英文 | 未采集 | 未采集 | 未采集 |
| S3 打开 Agent 设置 | `builtin` → `builtin` | D → D | `codex-sub` → `codex-sub` |
| S4 选择 DeepSeek | `builtin` → `dsh` | D → D | `codex-sub` → `deepseek` |
| S5 再次用 Codex 模型建立状态 | `dsh` → `builtin` | D → D | `deepseek` → `codex-sub` |
| S5 在选择器点击 DSH | `builtin` → `builtin` | D → D | `codex-sub` → `codex-sub` |
| S6 准备，换回 DeepSeek | `builtin` → `dsh` | D → D | `codex-sub` → `deepseek` |
| S6 第 1 次去 | `dsh` → `builtin` | D → D | `deepseek` → `codex-sub` |
| S6 第 1 次回 | `builtin` → `dsh` | D → D | `codex-sub` → `deepseek` |
| S6 第 2 次去 | `dsh` → `builtin` | D → D | `deepseek` → `codex-sub` |
| S6 第 2 次回 | `builtin` → `dsh` | D → D | `codex-sub` → `deepseek` |
| S6 第 3 次去 | `dsh` → `builtin` | D → D | `deepseek` → `codex-sub` |
| S6 第 3 次回 | `builtin` → `dsh` | D → D | `codex-sub` → `deepseek` |
| S6 重启 | 未采集 | 未采集 | 未采集 |
| S7 | 未采集 | 未采集 | 未采集 |
| S8 | 未采集 | 未采集 | 未采集 |

S5 实际点击 DSH 的前后，`config.json` 修改时间均为 Unix 纳秒 `1795492238662123000`。

### 新缺陷

- **高严重度 · 启动页选择模型会改写助理运行环境字段及供应商。** 在新助理的启动页主动选 DSH，随后选 Codex 订阅组的 GPT-6.2-Sol，再换回 DeepSeek V4.1 Flash。预期只有运行环境选择器的主动选择能改变三个字段。实际 `runtime` 在 `dsh` 与 `builtin` 间改写，`providerId` 从缺字段开始写入 `codex-sub`，换回时写入 `deepseek`。`runtimePreference` 保持 DSH，设置页仍显示 DSH。第一次及之后三次往返均复现。影响为配置持久化验收失败。重启后的影响仍待核对。 [S2](evidence-0.7.0-subscription/S2-zh-toast.png) · [S3](evidence-0.7.0-subscription/S3-zh-settings.png) · [S4](evidence-0.7.0-subscription/S4-zh-toast.png)

- **中严重度 · 展开选择器时突出显示外部 Codex CLI。** 在 S2 状态打开运行环境选择器，收起时显示 Managed Codex，展开后突出显示「外部 CLI」组的 Codex CLI。预期选择器应明确显示当前为托管 Codex。该截图只证明菜单状态差异，真实执行仍待 S7 核对。 [运行环境菜单](evidence-0.7.0-subscription/S2-zh-runtime-menu.png)

### 阻塞与结论

Windows 锁屏阻塞英文复验及 S7–S8。当前 AgenTeam 进程是验收会话的上游进程，完整重启将中断本轮工具和后续证据采集，重启用例需跨会话接续。相关步骤未计作通过。锁屏时界面语言已切至英文，App 全局默认模型未更动。

**不通过。2 项通过，4 项失败，2 项阻塞。** S6 已取得违反配置不变要求的证据，重启复验、真实会话执行、既有 Session 绑定及默认模型继承尚未完成。
