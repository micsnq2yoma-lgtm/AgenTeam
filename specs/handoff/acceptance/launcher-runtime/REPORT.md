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
