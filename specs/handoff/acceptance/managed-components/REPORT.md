# 托管组件升级验收

## 验收环境

- 候选基线 `5d3b11fb037413d0cb5537669d3351eac60da65b`。该提交包含 `a7fead6`。旧版为 Windows x64 的 [AgenTeam v0.6.8](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/tag/v0.6.8)，源码提交 `91836a075f48002f9c43c694d24230d35535a6b7`。
- 本机使用两个隔离的 Tauri Windows x64 构建和同一个临时用户目录，分别来自旧版标签和候选提交。已安装的日常程序只用于核对发布版静态资源。两个隔离构建通过专用应用标识和测试用 Windows 已知目录映射保存 App、WebView 和会话数据。此方法验证组件随 App 代码切换的行为，正式安装器覆盖另见下文。
- 按实际运行链路测试 Google Antigravity（`antigravity-sub`）。Grok（`xai-sub`）使用独立链路。
- 下表的凭据证据仅使用匿名条目、文件摘要、时间及比较结果。

## 用例结果

| 用例 | 结果 | 证据与备注 |
| --- | --- | --- |
| C1 旧账号续用 | 失败 | 旧版设置提供 Antigravity（订阅）入口，候选设置与会话的供应商列表根据 `5caec3ab` 隐藏此路由。旧用户无法从候选 UI 按原方式发消息。旧版入口截图 `C1-old-antigravity-entry.jpg` 的 SHA-256 为 `1a60ae4eebb03052f6414d49a64877bf85e37f654f122684d6201893f2770f97`。真实账号文件的摘要、修改时间和格式兼容性只在真实账号写入后可测。 |
| C2 组件切换 | 通过 | 候选 Windows App 在隔离资料中读取 8.0.13 `current`、旧安装目录及一条不含凭据的已登记账号记录；启动了 8.0.79 原生进程后提交 `current=8.0.79`、`previous=8.0.13`、`pending=空`。旧目录保留。 |
| C3 🔒 禁止不兼容回退 | 阻塞 | 隔离故障探针中 8.0.79 的随包归档摘要损坏，已批准的 8.0.13 仍在磁盘。30 秒内没有启动任一版本，指针保持 `current=8.0.79`、`previous=8.0.13`、`pending=空`，App 日志报告 `retry_required`。真实账号下的用户可见提示受候选入口隐藏阻塞。 |
| C4 定期检查与防降级 | 失败 | 启动后于 12:32:53 UTC 检查，12:47:59 UTC 再检查，第二次只收约 8.4 KB 的清单及签名。8.0.79 保持为已选的随包组件。以带 8.0.13 的旧 App 打开已切换的隔离资料，仍运行 8.0.79，指针无变化。同一份未登录资料重启后再次下载约 23.6 MB 的旧版 8.0.13 归档。见缺陷 3。 |
| C5 运行中任务先 drain | 阻塞 | 验收时线上签名清单仍为 8.0.13。没有可在当前组件上执行且满足凭据兼容规则的在线切换。CI 的 `policy_stop_reaches_writer_before_waiting_for_normal_drain_fence` 和 `proxy_replacement_does_not_consult_a_waiting_component_approval` 已通过。 |
| C6 🔒 篡改拒绝安装 | 通过 | 保留签名清单，将 Windows ZIP 偏移 13,559,840 处的一字节翻转。准备程序报 `CLIProxy distribution artifact integrity mismatch`，先前可信归档 SHA 不变。隔离 App 也拒绝安装本地篡改副本，记录 `retry_required`，没有启动该二进制或写出新安装指针。 |
| C7 登出与再登录 | 阻塞 | 候选隐藏 Antigravity 订阅入口，无法通过用户支持路径验证登出、再登录和真实凭据清理。离线原生协议探针通过取消、晚到回调拒绝及无凭据残留。 |
| X1 Codex 运行集下载 | 阻塞 | App 已实际下载、校验并安装 `codex-0.171.0`；旧版也实际安装了 `codex-0.167.0`。0.171.0 的原生进程通过启动探针。首次安装遇到 Windows PowerShell 模块加载错误，干净环境复跑通过。真实订阅发送待旧实例登录和建会话。 |
| X2 历史会话续发 | 阻塞 | 旧版隔离实例已安装并准备登录；尚未产生旧版已登录 Codex 会话。 |
| X3 模型与运行权限 | 通过 | 正式签名 Windows 二进制原生执行 `model/list` 与 `thread/start`；模型清单、`medium` 推理强度、只读权限、按需批准返回正确。应用协议夹具与候选生成的 schema 对齐。 |
| X4 下载失败保留可用版本 | 阻塞 | 上一运行集保留的 Rust 测试覆盖 `verified_stale_runtime_remains_usable_during_update`。本机优先代理失败后，App 的直连回退能成功，尚未在隔离模式下同时中断两条路径并使用真实订阅发送。 |
| R SDK、DSH、MCP、IM | 阻塞 | Claude Agent SDK 通过真实二进制加本地模拟 Provider 完成消息与子 Agent 调用。DSH 0.1.23 的原生流式恢复用例 8 项通过，原生 stdio MCP 探针 1 项通过。IM 消息测试 7 项通过。真实 SDK/DSH 服务和真实 IM 消息未执行。 |

## 版本与文件证据

| 组件 | 升级前 | 候选 | 实测范围 |
| --- | --- | --- | --- |
| CLIProxy | 8.0.13 | 8.0.79 | 发布版资源和候选随包资源，均经现有上游信任根验证 |
| Managed Codex | 0.167.0 | 0.171.0 | 锁文件和实际运行集，Windows 二进制均启动 |
| DeepSeek Harness | 0.1.23 | 0.1.23 | 此机器已安装版和候选资源的 `runtime-artifact/package.json` 均为 0.1.23，文件相同 |

| 文件 | 升级前 SHA-256 | 候选 SHA-256 |
| --- | --- | --- |
| CLIProxy Windows x64 签名清单 | `e0a446d6148f9e063a4ba3d422c43d39699623e79aebf8ab20409d9072d3178f` | `b205e287d4cc5ca7fa101c6a44ebfac94eef6810243c473f4faad1e220645e31` |
| CLIProxy Windows x64 可执行文件 | `5c3f35c5b2bd8e205379e6fe38d8c541b991bebdabdea8bb6b891ad7c630b6b6` | `15c17daf96f56391acb56b51a32b7287aa02610049c6331d1b2a5fa084b52a16` |
| CLIProxy Windows x64 ZIP | `4dfd99a1761a26e1a048f76277949da4108ba9a15cbcb14962375c65a52af136` | `1b00d36cd42eeee6f7ef5680b4a8603750f967b306623ead32035848e96f1a6f` |
| Codex Windows x64 运行集清单 | `04a887a65b9a8cef8ba02959df72a2260f101a84456fe0bc0ced45ca60d1b34b` | `a306eb5c07fa63a6ea7779aa97831416d43599462df904a8ef161ee51ffd41c9` |
| Codex Windows x64 ZIP | `1a28decc161f5ae52998c1b297ef216f6bc7773ede978bfacea55e232600b9ad` | `a5fd9616f7edcf55e9452495be72c6777eca5d2e9d016c2d8af34b3396351301` |
| Codex Windows x64 可执行文件 | `edfdd48f8b458af0906e507b4cb05217b326c3d6e515e9f024e05e3b8d650f70` | `cefea4ffee1fff04983a99c0dda99944d8b380e891e740a28b32f4b815a6e0b7` |
| DSH `runtime-artifact/package.json` | `723623d7cce78cd3fbe8ee43d3696416eae1bb3d48f570621ef429413819f874` | `723623d7cce78cd3fbe8ee43d3696416eae1bb3d48f570621ef429413819f874` |

- DSH 本次保持 **0.1.23**。已提到的 0.1.24 未进入候选运行集。
- Codex 两个版本的清单和归档都从 `download.myagents.io` 下载，使用 `myagents-runtime-manifest-ed25519-2026-06` 对应的仓库信任根校验两层签名。候选归档 193,902,469 字节。
- CLIProxy 随包 8.0.79 的 `credentialCompatibleVersions` 为空。验收时 CDN 在线签名清单仍给出 8.0.13，`policyRevision` 均为 2。启动时在线返回的版本为检查降级规则提供了实际输入。

## 验证记录

- [候选提交 Test](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/runs/45144558749) 的 JS、Linux、Windows、macOS 四个作业成功。macOS CLIProxy 生命周期 54 通过、2 个凭据用例未运行。通过项包含 `an_existing_account_selects_the_bundled_upgrade_offline`、`app_bundled_upgrade_accepts_existing_accounts_without_a_version_list`、`extraction_and_reuse_check_every_file_and_reject_extra_or_modified_payloads`、`immutable_pointer_reuse_does_not_create_a_duplicate_update_and_gc_keeps_current`。
- Windows 原生 Codex 0.171.0 的 `app-server generate-json-schema --experimental` 生成 10 个服务端请求方法、7 个初始化能力，与仓库协议夹具逐项相同。0.167.0 的 `currentTime/read` 在 0.171.0 移除，App 的兼容方法表保留它。实跑 `initialize`、`model/list` 和 `thread/start`，新旧版本分别给出 9 与 11 个模型，两者皆接受只读、按需批准和 medium 推理强度。
- Windows 本地协议测试初次 89 通过、2 失败。Skill 路径分隔符断言在 v0.6.8 同样失败。Playwright preset 的 `npx` 断言在没有可发现的完整 Node/npm 目录时失败，将正式随包的 Node/npm 目录交给系统分发探测后复跑通过。
- 本地回归补充测试覆盖 Claude 适配器、MCP 会话选择、Codex MCP 生命周期、IM Bridge schema 和扩展编译。合计 30 通过、2 失败、2 个 Windows 无法运行的符号链接用例。扩展编译的两个失败在 v0.6.8 可复现，分别是 Skill 路径和插件 MCP 路径断言。该组证据用于定位平台断言，不代替真实消息回归。

- Windows 原生 CLIProxy 的 `verify-cliproxy-contract.mjs` 通过 10 项检查，含健康端点、管理密钥与模型密钥隔离、Antigravity 模型目录、取消授权和无凭据残留。
- 篡改后 ZIP SHA-256 为 `ce5c60e3ff8d714cecb391980e469a5182ca33005f008e9f7c81edade630e373`。原生隔离测试对编译时实际使用的随包路径注入副本，随后恢复校验通过的原件。
- 两个 Windows App 分别通过真实界面安装 0.167.0、0.171.0，安装确认使用签名清单和 Authenticode。旧版签名证书 SHA-256 为 `6fe0b6ee110967a9290d24eefaff1df7323012e67fd03bfe13d6fad7e24b6d22`，候选签名证书 SHA-256 `a492526f4c84338f39252d9b56d249cfb46be1e35cf2b7a0eaa5f21dda7a1a35` 与批准值相同。
- 本地 DSH 离线回归使用官方 v0.1.23 Windows 资源，`MYAGENTS_DSH_NATIVE_SMOKE=1` 后运行 `vitest --config vitest.dsh-native.config.ts -t 'history after'`。将 OpenSSL 进程环境指向现有配置后，Anthropic 和 DeepSeek 模拟服务共 8/8 通过。另一个原生测试 `preflights credential-free stdio MCP through the real credential reverse port` 通过。
- Windows 的 Channel-handoff 集成测试在创建符号链接时因 `EPERM` 停止，尚未走到附件断言；IM 的 7 个单测通过。

## 新缺陷

1. **高，候选功能与验收目标冲突。** 复现：对比旧版与候选版「模型设置」。旧版显示 Antigravity（订阅），候选隐藏。预期是保留旧订阅的消息入口。实际是 `5caec3ab` 为规避账号风险而在所有平台隐藏这一反向代理供应商。Antigravity CLI 及 Grok 属于不同执行路线。
2. **中，Codex 安装受 PowerShell 模块环境影响，错误未转成可读提示。** 复现：在本机启动隔离 App，使用启动器原有环境，通过模型设置下载 Windows 运行集。CDN 归档下载结束后，`Get-AuthenticodeSignature` 报 `CouldNotAutoloadMatchingModule`，App 直接把 `CLIXML` 放进界面，未提交 `installed.json`。预期是签名校验完成并安装，或在失败时给出简短提示。将 App 进程的 `PSModulePath` 指向 Windows PowerShell 的系统模块目录后，另一隔离实例复跑成功，安装 0.171.0。旧版 0.167.0 在同样的干净环境也成功。归档和签名均可用；当前证据覆盖本机启动环境。
   - 截图 `X1-windows-authenticode-error.jpg`，SHA-256 `07c9bb5d3680117603c7ee50e2b00f2ed97e1cc321cd29dcefd6294d088eb69a`。
   - 截图 `X1-windows-installed.jpg`，SHA-256 `6711099b1c7546e01eff783b4f730d0bb0767a34c3df6920e2a9e52b5bac53b0`。
3. **低，未登录的配置在重启后重复下载较旧的 CLIProxy ZIP。** 复现：以候选版使用新建资料，随包准备 8.0.79，CDN 发布的签名清单只提供 8.0.13。保持未登录运行到下一轮检查，然后以同一资料重启。预期仅下载适用的新归档。实际首轮和重启分别在 12:32:54、12:48:27 UTC 拉取约 23.6 MB 的 8.0.13 归档，日志均指向 `downloading target=8.0.13`。已选的 `pending` 一直为 8.0.79，未发生降级。

## 阻塞项与结论

- 旧版真实订阅尚未建立 Codex 历史会话；因此没有真实账号文件哈希和升级后会话回复证据。
- CLIProxy 真实账号入口在候选被隐藏。新签名组件需满足凭据兼容限制才可进入有活动任务的在线切换。本次用无真实凭据的隔离资料测试本地切换、错误和防回退。
- 未同时阻断 Codex 下载的代理路径与直连路径，未验证真实订阅在下载失败时的会话连续性。真实 IM 回归缺少隔离测试通道。
- 本机通过切换隔离 App 构建验证组件；不同于完整 NSIS 原地升级。[候选 Windows 安装和升级作业](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/runs/45170973065) 是独立的打包验证。提交报告时仍在 `Build NSIS installer` 步骤，正式安装器升级未计入本次结果。

**结论：不通过。** C1 的旧订阅入口在候选版不可用，C4 的重复下载可复现。C2、C6、X3 的组件证据通过，真实 Codex 订阅发送与续发仍待验证。

## X1-E / X2 / X4（0.6.8 → 0.7.0）

验收日期为 2026-11-23（太平洋时间）。按用户补充指示，桌面实测使用当前 Windows 11 x64 用户已运行的 AgenTeam **0.7.0** 和原生 Windows Profile 下的 `.agenteam`。未使用独立 Windows 用户。本轮只新建合成助理 `QA-Codex-Upgrade-Synthetic` 和合成对话。当前 App 版本由运行进程的文件版本确认，ChatGPT（订阅）卡片显示「已验证」，订阅设置显示「已登录」，Codex Runtime 显示 `v 0.171.0`（`win32-x64`）。设置页证据取自界面可访问性文本，提交的截图只包含合成会话。

当前用户已处于升级完成的版本。App 还承载当前用户的其他任务。本轮没有为重建旧基线而退出、降级 App 或改写其运行集指针。没有触发安装器或运行集安装，因此本轮没有 UAC 观察记录。C3、C5 不纳入本节。

### 正式 Windows 安装包

从 GitHub Releases 下载两个 Windows x64 NSIS 安装包及各自的 `SHA256SUMS`。对本地完整包用 `Get-FileHash -Algorithm SHA256` 实算，结果同时与发布清单和 GitHub 资产摘要一致。

| App | 发布提交 | 安装包及字节数 | 本地 SHA-256 | 核对 |
| --- | --- | --- | --- | --- |
| v0.6.8 | `91836a075f48002f9c43c694d24230d35535a6b7` | `AgenTeam_0.6.8_x64-setup.exe`（373,859,305） | `5f6ee3b371e3dfcbeb8d60314bfa730f9905934e7cc5427282b2b04184fd5bcb` | 一致 |
| v0.7.0 | `5c533ea28f095690810c1a11b0da924c9cd5f251` | `AgenTeam_0.7.0_x64-setup.exe`（377,805,261） | `50c20afcfdf7d0a1d298974e445dbd8dd36ff8f78274742c723547c5dc22acbc` | 一致 |

发布记录及清单见 [v0.6.8 发布页](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/tag/v0.6.8) · [v0.6.8 SHA256SUMS](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/download/v0.6.8/SHA256SUMS) · [v0.7.0 发布页](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/tag/v0.7.0) · [v0.7.0 SHA256SUMS](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/download/v0.7.0/SHA256SUMS)。

### 用例结果

| 用例 | 结果 | 证据与范围 |
| --- | --- | --- |
| X1-E 完整下载安装 | 阻塞 | 当前 App 已安装 0.171.0，订阅设置只提供「刷新」。缺少干净的首次下载安装起点。正常和污染环境的 Authenticode 探针通过，篡改副本摘要不符。见下方 X1-E 逐步记录。 |
| X2 历史会话续发（🔒） | 阻塞 | 当前版本真实订阅续发通过。没有在 0.167.0 创建会话并原地覆盖至 0.171.0 的证据。见 [M1 与执行者](evidence-codex-070/current-070-m1-managed-codex.png)、[重开后 M2](evidence-codex-070/current-070-m2-reopened-history.png)、[同会话 M3](evidence-codex-070/current-070-m3-follow-up.png)。 |
| X4 下载失败保留可用版本（🔒） | 阻塞 | 0.171.0 已是当前指针，未进入 0.167.0 已验证且待更新的状态。两条失败路径、错误提示、旧版执行及恢复重试均未触发。见下方 X4 逐步记录。 |

### X1-E 逐步记录

1. 在当前 0.7.0 的「设置 → 模型 → ChatGPT（订阅）→ Codex 订阅设置」观察到已安装 0.171.0，且界面没有待下载安装入口。磁盘对照见下表。未观察首次下载、App 内校验及新写入 `installed.json`。
2. 从公开 CDN 下载 0.171.0 Windows x64 运行集归档。归档长 193,902,469 字节，SHA-256 为 `a5fd9616f7edcf55e9452495be72c6777eca5d2e9d016c2d8af34b3396351301`，与运行集清单一致。只向临时目录解出签名检查目标 `codex.exe`，未启动该副本。分别在默认子进程环境及初始 `PSModulePath` 只指向 PowerShell 7 模块目录的环境，调用 0.7.0 源码中的 Authenticode 脚本。两次 `powershell.exe` 均退出 0，脚本返回 `Valid`，证书 SHA-256 均为 `a492526f4c84338f39252d9b56d249cfb46be1e35cf2b7a0eaa5f21dda7a1a35`。两次子进程 stderr 均含 CLIXML，签名成功结果由脚本的 stdout 返回。没有本轮 App 失败提示的显示证据。 [校验脚本](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src-tauri/src/managed_codex.rs#L1060)
3. 对临时归档副本翻转偏移 `96951234` 的一字节。副本 SHA-256 变为 `abb0ff585cb952b7b1b90f5a17f7f024ece2378f5cd8efd5d98fa32005d9882e`，与清单不符。原件摘要保持一致。损坏副本只做文件校验，没有交给 App，因此没有拒装时的提示截图或安装指针写入证据。

### X2 逐步记录

| 步骤 | 实测及缺口 |
| --- | --- |
| 1. 旧版安装及登录 | 已核对 v0.6.8 安装包。当前桌面从 0.7.0 / 0.171.0 起步。保留的 0.167.0 目录只证明目录存在。没有本轮的旧版安装指针或登录前快照。 |
| 2. 旧版 M1、退出 | 未在旧版发消息或退出 App。补充实测为当前 0.7.0 新建助理并选择 Codex（订阅）的 GPT-6.2-Sol。合成会话 ID 为 `699c3749-e387-4f77-bc7d-1b3839c45667`。任务菜单显示 Managed Codex，原生 `app-server` 进程路径中的版本是 `0.171.0`。M1 的最终回复是 `ok`，此前还显示了一段前言和两条工具记录。 [M1 截图](evidence-codex-070/current-070-m1-managed-codex.png) |
| 3. 正式覆盖升级 | v0.7.0 安装包摘要已核对。本轮未运行覆盖安装器。没有升级首启时的状态文案、旧运行集可用性或登录凭据摘要差异。 |
| 4. 同会话 M2 | 在同一 0.7.0 App 返回启动页，再从侧栏重开合成会话。M1 和 `ok` 仍完整显示。发送 M2 得到 `蓝鲸-4417`，没有重新登录提示。 [M2 截图](evidence-codex-070/current-070-m2-reopened-history.png) |
| 5. 更新完成后 M3 | 未触发版本更新。在同一当前版本会话发 M3，仍收到 `蓝鲸-4417`，没有重新登录提示。烟测结束后 `installed.json` 仍指向 0.171.0，旧目录仍存在。 [M3 截图](evidence-codex-070/current-070-m3-follow-up.png) |

🔒 主动检查了当前会话重开后的历史、回复和登录提示，也对照了实际进程及安装指针。已观察的续发轨迹未出现历史丢失或重新登录提示。覆盖升级期间的这些失败判据仍待实测。

### X4 逐步记录

1. 缺少「旧版已验证 0.167.0 为当前指针、已登录、有旧会话」的起点。当前已登录状态和保留的旧目录无法建立这个起点。
2. 当前会话无管理员权限。未改 Windows 防火墙、hosts、App 代理或网络配置，也未启动本地拒绝代理。没有在当前可用版本上人为制造故障。
3. 未触发更新。没有「需要更新且仍可用」的界面截图、失败短提示、故障时的 `installed.json`、下载日志或从 0.167.0 发出的回复。0.7.0 源码有 `verified_stale_runtime_remains_usable_during_update` 覆盖旧运行集判定。这是代码对照。 [状态测试](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src-tauri/src/managed_codex.rs#L3141)
4. 本轮没有阻断可解除，未实测网络恢复后的重试、更新成功或更新后的同会话发送。🔒 未取得失败瞬间旧指针、旧版本执行及凭据是否保留的证据，也未观察失败提示能否排除 CLIXML、堆栈和令牌。

| 下载路径 | 本轮 App 实测结果 |
| --- | --- |
| 配置代理 | 未触发。无 CONNECT 403、失败原因或重试日志。 |
| 直连回退 | 未触发。无回退时序和结果。上一轮候选构建曾在代理失败后直连成功，已记录在前文 X4。 |

### 文件和凭据前后对照

以下读取仅投影运行集 `installed.json` 指定字段。未打开既有账号的凭据文件。凭据记录用应用内的登录观察，文件名、大小、修改时间及 SHA-256 均无本轮快照。

| 观测点 | `version` | `sha256` | `manifestSignature` 存在 | 运行集版本目录 |
| --- | --- | --- | --- | --- |
| 旧版基线或 X4 阻断前 | 未采集 | 未采集 | 未采集 | 未采集 |
| 当前 App，合成发送前 | `0.171.0` | `a5fd9616f7edcf55e9452495be72c6777eca5d2e9d016c2d8af34b3396351301` | 是 | `0.167.0`、`0.171.0` |
| 当前 App，M3 完成后 | `0.171.0` | `a5fd9616f7edcf55e9452495be72c6777eca5d2e9d016c2d8af34b3396351301` | 是 | `0.167.0`、`0.171.0` |
| X2 覆盖升级后或 X4 失败、恢复后 | 未采集 | 未采集 | 未采集 | 未采集 |

| 凭据对照点 | 文件名 / 大小 / 修改时间 / SHA-256 | 登录观察 |
| --- | --- | --- |
| 当前 App，发送前 | 均未采集 | 卡片「已验证」，订阅设置「已登录」。 |
| 当前 App，M1–M3 完成 | 均未采集 | 三轮均返回内容，没有要求重新登录。 |
| 覆盖升级前后、下载失败和恢复 | 均未采集 | 场景未触发。 |

### 新缺陷及结论

本轮没有可登记的目标用例新缺陷。上一轮的 CLIXML 界面缺陷未取得本轮桌面复现。子进程的 CLIXML stderr 是补充探针结果，用户界面的失败提示仍待实测。

**结论为阻塞，验收未完成。0 项通过，0 项失败，3 项阻塞。** 已验证两个正式安装包的 SHA-256，并取得当前 0.7.0 / 0.171.0 的真实订阅续发和两种进程环境的签名证据。X1-E 缺干净安装起点，X2、X4 缺可用于覆盖升级的旧版已登录状态。正式覆盖、下载故障时保留并运行旧版本以及恢复后的更新均未验收。
