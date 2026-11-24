# Antigravity CLI 运行环境独立验收

验收日期为 2026-11-23。结论为 **不通过（环境阻塞）**。A1–A13 共 13 项阻塞、0 项通过、0 项失败。

## 环境

| 项目 | 记录 |
|---|---|
| 仓库与验收基线 | `micsnq2yoma-lgtm/AgenTeam`，GitHub `main` 的 `a7fead6695c6b46f73056eedb3960278653625ff` |
| 技能版本 | 78，见该基线的 `SYSTEM_SKILLS_VERSION` |
| 操作系统 | Windows 11 26H2，x64，构建 26300.9974（本轮独立复核） |
| agy 版本 | 无法取得。本机常规命令解析及 Windows App Paths 未发现 `agy` |
| 要求的账号类型 | Gemini 订阅 |
| 登录状态 | 未验证，未启动真实 `agy` |

核对了 [运行环境设计 §5.5](../../../tech_docs/multi_agent_runtime.md#55-antigravity-cli)、[产品技能 Antigravity CLI 一节](../../../../bundled-skills/agenteam-docs/references/models-providers-runtimes.md#antigravity-cliantigravity) 和 [适配器文件开头](../../../../src/server/runtimes/antigravity.ts)。这些材料用于确认用例预期。

## 本轮环境证据 E1

2026-11-23 在 Windows 上建立临时 HOME 和独立的空项目。仅在探测进程内将 `HOME`、`USERPROFILE`、`APPDATA`、`LOCALAPPDATA` 指向临时目录，保留主机 PATH。从新项目目录执行以下检查，仅记录计数、退出码和系统版本。

- 用 `Get-Command -All` 查找 `agy` 及其 `.exe`、`.com`、`.cmd`、`.bat`、`.ps1` 变体，再以 `where.exe agy` 复核。
- 用 `Test-Path -LiteralPath` 检查 HKCU、HKLM 及 HKLM 的 Wow6432Node 下对应的 `App Paths\agy.exe` 注册项。

```text
os=Windows 26H2 X64 (26300.9974)
new_project_entries=0
Get-Command matches=0
where.exe agy exit=1
Windows App Paths registrations=0
```

以上五行按 UTF-8 无 BOM 编码，每行以 LF 结尾，SHA-256 为 `d188f6389c3b17f614b168e1c60baa5f669358dd16349884483da40f5238f2c8`。在该环境中无法以 `agy` 启动真实 CLI，版本、登录状态和订阅有效性均无法验证。探测止于此处，未读取日常账号数据目录。A1–A13 均未执行，没有本轮产品截图或会话事件日志。

## 历史环境证据 E0（前次验收）

在 Windows PowerShell 用 `Get-Command -All` 查找 `agy` 和 `.exe`、`.com`、`.cmd`、`.bat`、`.ps1` 变体，用 `where.exe agy` 复核命令查找，并用 Windows 注册表 API 检查当前用户、系统和 32 位系统视图的 `App Paths\agy.exe` 登记。以下是去除个人路径后的记录。

```text
os=Windows 11 家庭版 中文版 25H2 X64 (26200.9974)
Get-Command matches=0
where.exe agy exit=1
Windows App Paths registrations=0
```

这四行按 UTF-8 无 BOM 编码、以 LF 分隔并保留末尾 LF，其 SHA-256 为 `2e2560698aa40cd66c3506e55ba294b5bb99f7dc0efab6f2bb11fa8e57f83f2c`。

前次验收在环境检查阶段停止。未启动 AgenTeam 或 agy，未创建验收 HOME 或项目，未访问日常账号的数据目录。没有产品截图、会话事件日志或实际凭证扫描结果。

## 用例结果

| 用例 | 结果 | 证据 | 备注 |
|---|---|---|---|
| A1 检测 | 阻塞 | E1，未取产品截图 | 启动页工具栏和 Agent 设置两个入口均未验证。「安装」、就绪或「升级」、检测失败后的「重新检测」均待采证。 |
| A2 基础对话 | 阻塞 | E1，未生成会话事件 | 待用新项目发普通问题，确认真实 agy 完成回答，且会话显示 Antigravity。 |
| A3 不存凭证 | 阻塞 | E1，未执行搜索 | 待在对话前后扫描隔离的 `~/.agenteam`，包括 `config.json`、会话目录和日志。搜索方法见下文。 |
| A4 模型与推理 | 阻塞 | E1，未生成目录或会话事件 | 待比较真实 `agy models` 目录与界面选项，观察首次加载、已有列表和后台刷新，核对切换模型及推理强度后的有效配置。 |
| A5 三种权限 | 阻塞 | E1，未生成审批或问答事件 | 待在三种权限下各读文件、改文件、跑命令，验证原生审批选项及批准、拒绝、完全访问免询问，并验证聊天内结构化问答能继续。 |
| A6 工具卡 | 阻塞 | E1，未生成工具事件 | 待读文件、编辑、搜索、终端、子 Agent 各触发一次，核对分组和展开详情，包括读取原文、命令输出、文件变更、子 Agent 过程与报告。 |
| A7 子 Agent 与关闭标签页 | 阻塞 | E1，未生成后台会话事件 | 待在子 Agent 回传前关闭标签页，等待后重开，核对汇总、子 Agent 记录和主 Agent 后续回复。 |
| A8 会话 MCP | 阻塞 | E1，未生成 MCP 状态或调用事件 | 待在 agy 会话启用 MCP 并实调，停用后验证下一轮不可用，再制造启动失败并验证可读状态和「重试」。 |
| A9 排队消息 | 阻塞 | E1，未生成队列事件 | 待在回答期间追加两条消息，分别验证按序处理及取消待处理项。 |
| A10 日志脱敏 | 阻塞 | E1，未执行搜索 | 待触发终端、文件读取及带合成名称的自定义工具，扫描隔离环境的性能日志与工具统计事件。搜索方法见下文。 |
| A11 独立代理 | 阻塞 | E1，未采集代理记录 | 待比较开启和关闭 Antigravity 代理范围时的本地代理记录，以通用请求和模型供应商流量检验代理范围是否独立。 |
| A12 反代供应商隐藏 | 阻塞 | E1，未取产品截图或配置哈希 | 模型供应商设置、聊天模型选择器和 IM 模型选择均待验证。用隔离 HOME 写入合成的旧 `antigravity-sub` 配置，比较升级前后该配置子树的哈希。 |
| A13 真实账号自动化 | 阻塞 | E1，未运行测试 | 待在已登录的隔离环境设 `MYAGENTS_TEST_AGY=1`，执行 `npm run test:credentialed -- src/server/runtimes/antigravity.credentialed.test.ts`，记录退出码和实际执行、通过、失败、跳过数。 |

## A3 与 A10 待执行的搜索方法

- **A3** 在隔离环境的进程内载入该次登录可取得的 token、Cookie 值和邮箱前缀。对对话前后两份 AgenTeam 数据分别做字面量匹配，覆盖可读取的原始文本及 JSON 转义表示。证据仅记扫描文件数、跳过或无法读取的数量、各类标记的命中数和脱敏后的证据哈希。无法取得标记或无法扫描的存储单独记录为覆盖缺口。匹配值不进入命令参数、终端输出或提交文件。
- **A10** 给合成命令行、读取路径和自定义工具名各用一个唯一标记。确认操作确实触发后，在隔离 `~/.agenteam` 的性能日志和工具统计事件中搜索完整值及标记，覆盖原文和 JSON 转义表示。记录产生和扫描的日志及事件数量、逐类命中数和证据哈希；若没有产生相应日志或事件，该覆盖项仍为阻塞。

## 新缺陷与阻塞项

本轮没有观察到可复现的产品缺陷，因此没有产品复现记录或严重程度评级。A1–A13 的共同阻塞项是缺少可在隔离验收环境发现并运行、且已用有效 Gemini 订阅登录的真实 `agy`。解除后须使用独立 HOME 和全新合成项目重跑整组，并采集脱敏截图、会话事件及证据哈希。

## 2026-11-23 第三轮 · 当前 Windows 用户 · AgenTeam 0.7.0

### E2 先决条件与隔离方式

| 项目 | 本轮实测 |
| --- | --- |
| Windows 与 App | Windows 11 x64，构建 26200。当前运行的原生 AgenTeam 安装文件版本为 `0.7.0`，对应发布标签 `v0.7.0` 的提交 `5c533ea28f095690810c1a11b0da924c9cd5f251`。 [正式发布](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/tag/v0.7.0) |
| 官方 Antigravity CLI | 在 Google 文档所列默认安装目录找到 `%LOCALAPPDATA%\agy\bin\agy.exe`。Windows Authenticode 为 `Valid`，签名发布者为 Google。沿用已有官方二进制。 `agy --version` 输出 `1.5.15`，退出码 0。 `agy acp --help` 包含 `--process-per-session`。 [Google 安装指南](https://antigravity.google/docs/cli/tutorial/) |
| `where.exe agy` | 原终端 PATH 查找退出 1，用户级 PATH 已有安装目录。仅向验收子进程补入该目录后，退出 0，输出 `%LOCALAPPDATA%\agy\bin\agy.exe`（用户路径简写）。 |
| 真实 Gemini 订阅 | 预检的 ACP 初始化与 `session/new` 成功，`agy models` 退出 0。两项均未报告订阅档位。用户随后确认已自行使用 Gemini 订阅登录。本轮真实回复验证了连接可用，未采集账号标识。 [Google ACP 登录说明](https://antigravity.google/docs/ide/extensions/acp/) |
| 数据范围 | 按补充指示使用当前 Windows 用户和当前 AgenTeam，原生 `dirs::home_dir()` 所定位的数据目录与当前 App 共用。通过 App 新建 `QA-Antigravity-A1-A13-Synthetic` 空白项目，关闭助理模板。首条消息前，项目目录 0 个条目。只读取两个已知合成会话文件与一份合成文件，未遍历账号目录、日常会话、App 全局配置或日志。 [Windows 数据目录实现](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src-tauri/src/app_dirs.rs#L113) |

本轮选的是运行环境选择器的外部 **Antigravity CLI**。隐藏的反代模型提供方 `antigravity-sub` 属于已决定的产品行为，未作为执行路径或缺陷。

### A1–A13 结果

| 用例 | 结果 | 证据及与预期的差异 |
| --- | --- | --- |
| A1 检测 | 通过 | 启动页「外部 CLI」列出可选择的 Antigravity CLI。「重新检测」后仍可选，选中后选择器显示 Antigravity CLI。合成项目的「Agent 设置 → 基础设置 → 运行环境」也显示 Antigravity CLI。实测覆盖本机已安装且可用状态，未构造未安装或过低版本。 [启动页](evidence-070-agy-1515/A1-launcher-runtime.png) |
| A2 基础对话 | 通过 | 合成会话在 Gemini 3.8 Flash (Medium) 下准确回复 `AGY-QA-OK-117`。任务详情显示 Antigravity CLI，合成会话 ID 为 `a2227672-6a58-4369-b724-84490affe9e9`。 [会话](evidence-070-agy-1515/A2-real-session.png) |
| A3 不存凭证（🔒） | 阻塞 | 对本轮确切的三个合成文件主动查找通用凭证格式，四类各 0 命中。共用当前用户资料，未扫描全局 `config.json`、日常会话或日志。真实账号标识、令牌及其 JSON 转义形式未做字面量对照。见下表。 |
| A4 模型与推理 | 通过 | 首次打开即列出 Gemini 3.8 Flash 的 High、Medium、Low，另有 Claude Sonnet 4.6、Claude Opus 4.6、GPT-OSS 120B，与 `agy models` 中对应的名称和档位一致。重开会话后菜单仍可用。选择 Medium 后改为 High，生效的模型 ID `gemini-3.8-flash-medium`、`gemini-3.8-flash-high` 均在本轮会话记录中。未单独测量后台刷新的触发时点。 [首次模型菜单](evidence-070-agy-1515/A4-first-model-menu.png) · [High 档](evidence-070-agy-1515/A9-ordered-replies.png) |
| A5 三种权限 | 阻塞 | 菜单有「请求批准」「帮我批准」「完全访问权限」。请求批准模式下，原生文件工具自动写入并读回工作区内的一行合成标记，与菜单对工作区内编辑的说明一致。真实账号测试中的完全访问编辑路径通过。三模式的逐项读、写、终端矩阵、批准与拒绝、聊天内问题接续未完整实测。A13 缺少预期审批事件另有记录。 [菜单](evidence-070-agy-1515/A5-permission-menu.png) |
| A6 工具卡 | 阻塞 | 编辑卡显示 +1 与文件 diff，读取卡展开展示原文，子 Agent 卡保留步骤和报告。搜索轮展开后实际是三条失败的 Read 卡，没有搜索结果卡。终端工具卡未触发。 [编辑及读取](evidence-070-agy-1515/A6-read-edit-cards.png) · [子 Agent](evidence-070-agy-1515/A7-child-after-close-reopen.png) |
| A7 子 Agent 与关闭标签页 | 通过 | 在 File Verifier 子 Agent 已出现且父轮仍运行时，用 Ctrl+W 关闭合成会话标签。侧栏仍显示运行，随后更新为后台完成。重开原会话后，子 Agent 卡显示完成、用时 10 秒、5 次工具调用，能展开报告。主 Agent 汇总保留合成文件标记。 [重开后证据](evidence-070-agy-1515/A7-child-after-close-reopen.png) |
| A8 会话 MCP | 失败 | 「工具（在此任务中启用）」启用 Playwright，输入框显示工具数 1。请求用它访问 `about:blank` 后，回复 `AGY-QA-MCP-UNAVAILABLE`，没有调用卡或可读的启动失败／重试状态。随后关回开关，工具数回到未启用状态。没有可用的首次调用可供确认禁用后的撤销，未另行注入启动故障。真实账号适配器测试的 stdio 和 HTTP MCP 也未取得预期回执。 [开关已启用](evidence-070-agy-1515/A8-playwright-enabled.png) · [工具不可用](evidence-070-agy-1515/A8-enabled-mcp-unavailable.png) |
| A9 排队消息 | 通过 | 观察到生成期间消息出现「当前轮结束后发送」和「取消排队」。顺序测试的 `645`、`646` 依次得到对应真实回复。一次发送的桌面驱动超时后仍迟到入列，出现重复基准提示，该轮不能用于取消判定。独立合成会话 `70c07fc2-c88f-44c8-a071-19326ffbbe25` 重做取消，点「取消排队」后消息回草稿。基准轮完成后，合成会话文件中取消标记出现 0 次。 [回复顺序](evidence-070-agy-1515/A9-ordered-replies.png) · [取消前](evidence-070-agy-1515/A9-pending-before-cancel.png) · [取消后](evidence-070-agy-1515/A9-cancelled-to-draft.png) |
| A10 日志脱敏（🔒） | 阻塞 | 给合成文件和消息使用唯一标记，已验证读写实际发生。检查了本轮生成的会话文件。没有独占的性能日志和工具统计，未扫描当前用户的共用日志，也缺终端和自定义工具统计样本。见下表。 |
| A11 独立代理 | 阻塞 | 在合成项目的外部运行环境设置看到 AgenTeam 代理策略选项。未改当前用户的代理、系统网络或相关设置，未比较本地捕获流量和两种独立路由。 |
| A12 旧反代配置回归 | 阻塞 | 本轮按要求排除反代提供方。没有向当前用户的共用配置写入合成旧 `antigravity-sub` 配置，也没有运行其配置保留或三处入口的回归。 |
| A13 真实订阅多轮与审批 | 失败 | 桌面同一合成会话完成普通问答、工作区文件编辑读取和子 Agent 多轮。原生编辑按当前模式自动放行，未取得一条用户审批回执。真实账号自动化两次均退出 1，4 个执行、1 通过、3 失败、0 跳过，审批用例在等待 `permission_request` 时超时。见下面的逐项记录。 |

### A3 与 A10 凭证、日志检查

首次发送前确认合成项目为空。对话后仅打开以下确切的新建文件，查找电子邮件格式、Google OAuth token 常见前缀、带值的 Bearer 字段及常见凭证键。只记录文件名和计数。OAuth 签入由用户完成，没有采集可用于精确匹配的实际邮箱、token 或 Cookie。命中数只反映这些通用模式在指定文件中的结果。

| 阶段及文件 | 扫描范围 | 邮箱格式 | Google OAuth 格式 | Bearer 格式 | 带值凭证字段 |
| --- | --- | ---: | ---: | ---: | ---: |
| 对话前，空白合成项目 | 0 个条目，仅文件元数据 | 未扫描 | 未扫描 | 未扫描 | 未扫描 |
| 对话后，`a2227672-6a58-4369-b724-84490affe9e9.jsonl` | 159 行 | 0 | 0 | 0 | 0 |
| 对话后，`70c07fc2-c88f-44c8-a071-19326ffbbe25.jsonl` | 82 行 | 0 | 0 | 0 | 0 |
| 对话后，`agy-qa-A5.txt` | 1 行 | 0 | 0 | 0 | 0 |
| 共用 `config.json`、日常会话、日志、原生账号目录 | 未读取 | 未扫描 | 未扫描 | 未扫描 | 未扫描 |

A5、A6、A10 的桌面终端子场景受 **computer-use** 技能限制。该技能禁止通过 Windows UI 直接或间接执行终端命令，本轮按原生文件工具及授权的仓库测试取证。真实账号自动化使用自己的临时夹具。没有从共享日志中提取其他任务的记录。

### A13 真实账号自动化与新缺陷

在已有依赖的本机检出 `d655adecda61baac622ffaafc101cde897e299a5` 运行。Antigravity adapter、credentialed 用例文件和 Vitest 配置与 `v0.7.0` 对照无差异。测试命令为 `MYAGENTS_TEST_AGY=1 npm run test:credentialed -- src/server/runtimes/antigravity.credentialed.test.ts --reporter=json`。输出只保留结果、断言位置和安全分类。

| 测试 | 两次汇总与复跑分项 |
| --- | --- |
| 汇总 | 两次均为退出 1，共 4 个执行，1 通过、3 失败、0 跳过。 |
| 完全访问编辑及以普通权限恢复 | 通过。 |
| 一次批准、取消审批、换进程恢复 | 失败。首次原生文件编辑后等待 90 秒，未观察到 `permission_request`。 [断言](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.credentialed.test.ts#L76) |
| 会话级 stdio MCP 实调与撤销 | 失败。轮次已完成，合成调用回执文件不存在（`ENOENT`）。 [断言](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.credentialed.test.ts#L190) |
| 原生问答及会话级 HTTP MCP | 失败。到达 HTTP 工具回执断言，回复不含本轮合成回执标记。 [断言](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.credentialed.test.ts#L332) |

1. **中 · A8 会话工具显示启用，真实轮次不可调用。** 复现为在 0.7.0 的 Antigravity CLI 会话选择 Gemini 3.8 Flash Medium，从会话工具菜单打开 Playwright，再请它调用 `browser_navigate` 访问 `about:blank` 并查看快照。预期出现真实工具卡，或在启动失败时显示可读状态和重试入口。实测工具数显示 1，回复不可用，没有调用卡或失败重试入口。两个协议级 MCP 回执断言也失败。尚未定位桌面预置工具和两个协议夹具是否有共同根因。 [桌面复现](evidence-070-agy-1515/A8-enabled-mcp-unavailable.png)
2. **中 · A13 真实账号审批回归未产生所等待的事件。** 用上述命令执行审批、拒绝和恢复用例。预期首次合成编辑触发 `permission_request`，经批准后完成，再走取消和进程恢复。实测复跑在首次审批等待处超时，后两段未到达。桌面菜单明确说明工作区内文件编辑可以自动批准，因此现有证据确认的是验收断言失败，实际终端权限策略和是否存在审批旁路仍待独立验证。 [审批断言](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.credentialed.test.ts#L76)

### 结论

**整组未通过。5 项通过、2 项失败、6 项阻塞。** Google CLI 安装和 Gemini 订阅先决条件已满足。当前 Windows App 验证了运行环境检测、真实对话、模型档位、子 Agent 在关标签后的后台完成和排队取消。A8 与 A13 有失败证据。A3、A5、A6、A10、A11、A12 的缺口分别是共享资料不能全量查验、权限和工具覆盖不足、性能日志与代理对照未采、反代配置回归排除。提交的截图已裁切到合成界面，工具卡的用户目录路径已遮盖。

## MCP 与审批诊断

2026-11-23（太平洋时间）。AgenTeam Windows 正式版 **0.7.0**，发布提交 `5c533ea28f095690810c1a11b0da924c9cd5f251`。用用户已登录的 Gemini 订阅，探针只读写新建的临时空项目和合成数据。安装中的 `agy` 在诊断期间更新到 1.5.16。随后从官方更新器备份复制出签名有效、与验收时记录的 SHA-512 一致的 **1.5.15**，所有版本敏感对照都用该版本验证。桌面重开得到授权。转发器通过本次启动的进程 PATH 接入，只记录指定合成项目的协议字段，捕获后已撤销启用开关。全局 PATH、已安装 CLI、账号与产品代码均未修改。

### 1. ACP 原始能力

运行 `agy acp --process-per-session`，桌面授权模式未加 `--dangerously-skip-permissions`。用 stdio NDJSON 手工发送 `initialize`，协议版本 1，`clientInfo={name:"MyAgents",version:"1"}`，`clientCapabilities={elicitation:{form:{}}}`。随后发 `session/new`，传入合成 `cwd`、`mcpServers` 和 `_meta.antigravity.systemPrompt.append=""`。这是 0.7.0 适配器的启动形状。 [适配器](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.ts#L929)

`initialize` 的 `protocolVersion=1`、`agentInfo.version="1.5.15"`。以下是返回的 `agentCapabilities` 原字段和值：

```json
{
  "loadSession": true,
  "load_session": true,
  "promptCapabilities": {"image": true, "audio": false, "embeddedContext": true},
  "sessionCapabilities": {"resume": {}, "close": {}, "load": {}, "list": {}, "additionalDirectories": {}},
  "session_capabilities": {"resume": {}, "close": {}, "load": {}, "list": {}, "additionalDirectories": {}},
  "mcpCapabilities": {"http": true},
  "mcp_capabilities": {"http": true},
  "auth": {"logout": {}},
  "_meta": {"antigravity": {"tier": true, "systemPrompt": {"append": true}}}
}
```

HTTP 明确宣告支持，两个 MCP 能力对象均未返回 `sse` 或明确的 `stdio` 字段。stdio 是否可用由下面的进程启动、枚举和调用实证确认。SSE 未实测。 [ACP v1 会话设置规范](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/session-setup.mdx)

### 2–4. 直接注入与原生配置

stdio 的注入项为 `{name:"fixture_stdio",command:"<Node.exe>",args:["<fixture.cjs>","<合成回执文件>","<合成回执>"],env:[]}`。HTTP 的注入项为 `{type:"http",name:"fixture_http",url:"http://127.0.0.1:<port>/mcp",headers:[]}`。成功调用的唯一回执不写进提示，工具实际调用时才返回。ACP 发出审批时，直连探针对本轮已知的合成工具选 `allow_once`。

| 步骤 | 边界观察 | ACP 事件与原话 |
| --- | --- | --- |
| 2. stdio 注入 | 夹具启动写出 PID 和启动行，依次观察到 `initialize`、`tools/list`、一次 `tools/call echo_probe`。 | `tool_call` 及更新到 `completed`。回复带两个时间字段，最后一行为 `SYNTHETIC_RECEIPT_acy-stdio_94217`。没有单独的 MCP 错误通知。 |
| 3. 回环 HTTP 注入 | 首轮和串行复测均建立 TCP 连接，收到 `initialize`、`tools/list` 和一次 `tools/call echo_probe`。 | 首轮工具已 `completed`，但 `session/prompt` 90 秒内未结束，无文本。串行复测为 `end_turn`，最后一行为 `SYNTHETIC_RECEIPT_retry-http_94217`。没有单独的 MCP 错误通知。 |
| 4. 原生工作区 stdio 配置 | 在全新项目的 `.agents/mcp_config.json` 写 `mcpServers.fixture_native={command,args,env:{}}`，ACP 传空数组。夹具已启动并收到 `initialize`、`tools/list`。 | 两次均无 `tool_call`、无回执。回复分别为 `MCP_UNAVAILABLE` 和 `NATIVE_MCP_UNAVAILABLE`。第二次明确提示服务器名、工具名及命名空间。 |
| 4. 原生工作区 HTTP 配置 | 同一路径用 `mcpServers.fixture_native={serverUrl}` 注册回环服务器。观察到 TCP、`initialize` 及 `tools/list`。 | 两次均无 `tool_call`、无回执，分别回复 `MCP_UNAVAILABLE` 和 `NATIVE_MCP_UNAVAILABLE`。 |
| A13 传输形状重放 | stdio 使用真实捕获的五个 env 字段名、单一脚本参数和 `fresh_receipt`。HTTP 增加 `Authorization` 头，在回环服务器只记录是否匹配。两条都收到一次 `tools/call`，HTTP 的调用头匹配。 | 原始工具名分别是 `mcp_receipt_fresh_receipt` 和 `mcp_ticket_issue_ticket`。允许一次后，末行分别是 `SYNTHETIC_RECEIPT_wire-stdio_94217` 和 `SYNTHETIC_RECEIPT_wire-http-auth_94217`。 |

原生工作区配置的观察只覆盖 ACP 会话中这两个夹具。启动并能枚举工具仍未带来模型调用，无法由此判定原生 CLI 的其他入口也不可用。未读取或写入日常 `~/.gemini` 配置。 [Google 原生 MCP 配置](https://antigravity.google/docs/cli/mcp/)

### 5. 桌面 A8 与实际请求

用全新空白项目 `QA-AGY-MCP-Diagnostic-Synthetic`，在**首轮发送前**打开 Playwright，模型为 Gemini 3.8 Flash Medium，请求只访问 `about:blank`。普通启动时已复现。获准重开后，用探针确认桌面对端 `initialize` 返回 **1.5.15**，同样出现工具数 1，但助手回复原话 `MCP_DIAG_UNAVAILABLE`。只捕获了 `session/new`，没有 Playwright `tool_call` 或 `session/request_permission`。合成会话 ID 为 `51d0993c-4540-4ce4-9e48-8f84f0b2c310`。 [桌面截图](evidence-mcp-approval-diagnostic/A8-desktop-pinned-1.5.15.png)

对该会话，只从预记文件偏移后的 App 日志取关联行并投影白名单字段。常规日志没有原始请求体：

```text
2026-11-23 23:22:56.085 [external-session] Starting antigravity session for <session>, model=gemini-3.8-flash-medium, permissionMode=accept-edits, scenario=desktop, resume=false
2026-11-23 23:23:09.020 [perf] trace=turn phase=final durationMs=4529.402 status=ok runtime=antigravity
2026-11-23 23:23:09.022 [perf] trace=turn phase=persist_done durationMs=1.012 status=ok runtime=antigravity detail.toolCount=0
```

以下数组来自透明转发器采集的**真实桌面 `session/new.params.mcpServers`**。路径和环境变量值已遮盖，字段和参数顺序保留：

```json
[
  {
    "name": "playwright",
    "command": "<path>/node.exe",
    "args": ["<path>/npx-cli.js", "-y", "@playwright/mcp@0.0.68", "--isolated"],
    "env": [
      {"name": "NO_PROXY", "value": "<redacted>"},
      {"name": "no_proxy", "value": "<redacted>"},
      {"name": "PATH", "value": "<redacted>"}
    ]
  }
]
```

与第 2 步一样，桌面使用 ACP v1 stdio 项，省略 `type`，传 `name/command/args/env`。差异集中在 `npx-cli.js` 启动的实际包和环境。用随包 Node 直接启动上述 npx 参数及隔离 npm 缓存，`initialize` 与 `tools/list` 成功，列出 22 个工具，包括 `browser_navigate`。随包 Playwright 的 `cli.js --isolated` 直接注入 agy 并在子进程边界观察时，两次均在提示前列出 22 个工具，均收到 `browser_navigate`，但 ACP 更新为 `failed`。其中一次助手原话包含 `DevTools remote debugging requires a non-default data directory. Specify this using --user-data-dir.`，另一次回复 `PLAYWRIGHT_DIRECT_UNAVAILABLE`。换全新显式资料目录并用无头模式的对照已发出 `tools/call`，90 秒内未收到工具结果，未验证是否能完成导航。

用同一个 `npx-cli.js -y @playwright/mcp@0.0.68 --isolated` 在 agy 1.5.15 下重放，并将观测点放到 MCP 子进程边界：

| npx 启动对照 | 从 `server/discover` 开始的相对时序 | 一轮内的结果 |
| --- | --- | --- |
| 冷的临时 npm 缓存 | 子进程启动，约 **4.91 秒**后收到 `notifications/cancelled`，该次没有进入 `initialize` 或 `tools/list`。 | agy 后续重试启动，模型回复 `PLAYWRIGHT_DIRECT_UNAVAILABLE`，全轮 0 个 `tool_call`，与桌面失败一致。 |
| 本轮预热缓存，仅对探针进程设 `npm_config_offline=true` | 约 **2.02 秒**完成 22 个工具的枚举。Playwright 对可选 `server/discover` 返回 `-32601` 后仍正常完成标准 MCP 初始化。 | 一次 `browser_navigate` 已发出，批准后进入 Chrome 启动，才因默认资料目录的远程调试限制变为 `failed`。 |

这组对照仅改变探针的缓存条件。桌面抓包没有记录原会话的 npm 缓存状态或它内部的 stdio 发现时序，冷缓存的取消链路是对同一启动参数和同一症状的独立复现。

A13 原样复跑在相同适配器代码、同一 1.5.15 和临时测试资料下，捕获到以下实际请求，两个用例仍失败：

| 原 A13 路径 | 实际 `mcpServers` 形状 | 原始事件及影响 |
| --- | --- | --- |
| stdio | 一个 `receipt` 项，`command=<path>/node.exe`，`args=["<path>/receipt.cjs"]`，`env` 是 `NO_PROXY`、`no_proxy`、`AGY_TEST_RECEIPT`、`AGY_TEST_CALLS`、`PATH` 的 `[{name,value}]` 数组。 | 原始 `mcp_receipt_fresh_receipt` 进入 `tool_call` 和 `session/request_permission`。实际回 `reject_once`，工具变为 `failed`，回执文件缺失。 |
| HTTP | 首会话发送一个 `{type:"http",name:"ticket",url:"http://127.0.0.1:<port>/mcp",headers:[{name:"Authorization",value:"<redacted>"}]}` 项，对照会话发送 `[]`。 | 原断言缺回执，带审批跟踪的复跑在等待该轮完成处超时。该轮未捕获 HTTP MCP 工具调用或其审批，不能确认是否有同一拒绝过程。 |

### 6. 请求批准矩阵

复用同样的 ACP 启动方式，每项独立新建合成项目，设置 `configId="mode"`。请求批准对应 `accept-edits`，回包确认生效。工作区外只使用测试目录下的已知兄弟文件，未访问其他路径。原始 `session/request_permission` 对应适配器发出的 `permission_request`。

| 操作 | 模式 | 审批请求 | 实际执行 |
| --- | --- | ---: | --- |
| a) 编辑工作区内 `inside.txt` | `accept-edits` | 0 | `edit completed`，文件改成合成标记。 |
| b) 编辑工作区外的合成兄弟文件 `../outside.txt` | `accept-edits` | 0 | `edit completed`，已核实兄弟文件写入。这一结果只适用于本轮的测试路径。 |
| c) 终端命令 `cmd.exe /d /c echo SYNTHETIC_TERMINAL_OK` | `accept-edits` | 1 | 原生给出 `allow_once` / `reject_once`，探针允许一次，工具完成并收到合成 stdout。 |
| 原 A13 模式对照，新建工作区内文件 | `default` | 0 | `edit completed`，合成文件确实建成。另一次替换已存在文件也为 0。 |

### 根因判断

- **A13 stdio 已定位到适配器工具名称映射与测试审批条件。** `agy 1.5.15` 用单下划线名称 `mcp_receipt_fresh_receipt`。产品的 `presentAntigravityTool` 只识别双下划线 MCP 形式，把该事件归为 `Tool`。将同样的工具事件输入未修改的函数，得到 `toolName="Tool"`。A13 只对名称匹配 `/receipt/i` 的请求批准，线上记录到它实际拒绝了已到达的调用。允许一次的直连重放获得回执。HTTP 的单会话重放也能正常调用，若运行时发出同样命名形式的审批，现有映射与 `/ticket/i` 也会拒绝。原样 HTTP 复跑没有观察到该事件，因此 HTTP 缺回执的根因仍未证实。 [工具名称映射](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity-tool.ts#L34) · [A13 测试](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.credentialed.test.ts#L153)
- **A8 已排除通用传输不支持、请求漏传或 stdio 数组结构错误。** 桌面实际发送了 Playwright，MCP 是否真正对模型可用仍未确认。适配器组装请求后把该条目标记为 `unreported`、`toolCount:0`，以 `status_read_failed` 放行。开关计数给出的只是用户启用意图，不能证明 Playwright 已就绪。桌面故障发生于注入以后、可见工具调用之前。直连观察到的 Chrome 启动错误发生于 `tools/call` 之后，属于另一条已复现的失败，不能解释该桌面轮次的零调用。 [会话组装及状态](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.ts#L977)
- **A13 审批断言所用动作不可靠。** 原测试等待工作区内的新文件编辑触发审批。直接采用测试的 `default` 仍观察到自动写入，采用桌面的 `accept-edits` 也一样。固定的无副作用终端命令给出稳定的正例。工作区外兄弟文件未请求审批的实测已单列，未由一条路径外推至其他目录。 [原等待断言](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5c533ea28f095690810c1a11b0da924c9cd5f251/src/server/runtimes/antigravity.credentialed.test.ts#L76)

**诊断结论。** 会话级 stdio 和 HTTP 在 1.5.15 均实调成功。A13 的 stdio 缺回执及文件编辑审批断言已有直接证据。A8 的实际请求和缺失调用已复现，相同桌面 npx 参数的冷缓存重放复现了发现阶段取消，预热离线对照恢复了枚举，支持 npx 启动耗时与 agy 发现窗口的竞争是 A8 的原因。实际桌面轮没有子进程时序，仍无法证明那一轮的具体缓存状态。原样 HTTP 复跑的缺回执原因仍待确认。

## 0.7.1 复验

2026-11-24（太平洋时间）。本轮在 AgenTeam **0.7.1** 与 **agy 1.5.16** 上完成 MCP 预热、工具名映射、审批与回归的完整复验（含获授权重启桌面端后的真实 WebView2 桌面 UI 端到端实测、运行时层对照实测与真实账号自动化测试）。

### 环境与隔离方式

| 项目 | 记录 |
| --- | --- |
| App 版本 | AgenTeam Windows 正式版 **0.7.1**（`ProductVersion=0.7.1`，`FileVersion=0.7.1`，已核实安装目录 `server-dist.js` 包含 `[antigravity] MCP warmup for` 预热逻辑）；源码基线 `origin/main` 的 `288225db9e35c05a6410d7d3e58588f63dff59a9`（含 `677ca1c1` 与 `288225db`）。 |
| agy 版本 | **1.5.16**（`%LOCALAPPDATA%\agy\bin\agy.exe --version`；`runtime.detect()` 返回 `installed=true, version="1.5.16"`）。用户已登录 Gemini 订阅，本轮真实模型调用通过，未采集任何令牌或账号标识。 |
| 隔离方式 | 1) 在临时目录创建独立 detached git worktree（基线 `288225db`）运行 M4 自动化与运行时探针，未修改主工作区、产品代码或 `main`。<br>2) 获用户授权暂时关闭日常运行中的 `AgenTeam.exe`，以进程级隔离的全新空 `LOCALAPPDATA`（保证 `npm-cache` 100% 冷缓存，绝不触碰日常 `%LOCALAPPDATA%\npm-cache`）、进程级 `PATH` 补入 `agy` 目录与 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=19222` 启动真实 0.7.1 桌面端进行 UI 端到端实测；M2 单独使用全新空 `LOCALAPPDATA` 与进程级 `HTTP_PROXY`/`HTTPS_PROXY=http://127.0.0.1:1` 启动。测试结束后已恢复启动正常无调试参数的 `AgenTeam.exe`。<br>3) 在桌面端新建空白合成项目 `QA-071-AGY-MCP-Synthetic`（关闭助理模板），所有桌面 UI 实测与截图均限定在该合成项目与右侧主视图区（裁除侧栏，不触碰、不读取、不展示日常项目与日常会话）；应用日志仅从各轮启动前预记的文件字节偏移之后读取并过滤本轮关联行。 |

### M1–M5 结果表

| 用例 | 结果 | 证据与说明 |
| --- | --- | --- |
| M1 冷缓存下的 Playwright（核心） | 失败 | **真实桌面端 0/3（1 次冷缓存 + 2 次热缓存均返回 `PLAYWRIGHT_UNAVAILABLE`，未出现 Playwright 工具调用卡）。**<br>- **桌面冷缓存**（`COLD_CACHE_EXISTS=False`，会话 `3f95eeb6-de8d-42a9-a039-18956c64d53d`）：在 `QA-071-AGY-MCP-Synthetic` 会话工具菜单启用 Playwright（显示 `工具 1`）后发送访问 `about:blank` 并读快照。`Starting antigravity session`（`03:23:36.054`）到 `turn_start`（`03:24:47.754`）耗时 **71.7 s**（期间冷缓存预热确实将隔离 `npm-cache` 从无填充至 4 个条目），但随后仍回复 **`PLAYWRIGHT_UNAVAILABLE`**（`detail.toolCount=0`），应用日志无预热成功行。[启用开关](evidence-071-agy-1516/M1-playwright-enabled.png) · [冷缓存不可用](evidence-071-agy-1516/M1-cold-desktop-unavailable.png)<br>- **桌面热缓存复跑 ×2**（同一项目点「新任务」，`NPM_CACHE_EXISTS=True`）：第 1 次（会话 `de53c62d-f271-47c6-94cf-78eae46d633d`）启动缩至 **9.2 s**（`03:28:07.957` → `03:28:17.178`，无额外下载等待），但仍回复 `PLAYWRIGHT_UNAVAILABLE 当前会话中没有可用的 Playwright MCP 工具`（`detail.toolCount=0`）；第 2 次复跑同样回复 `PLAYWRIGHT_UNAVAILABLE`。工具菜单显示「已配置，是否可用待确认」。[菜单状态](evidence-071-agy-1516/M1-tools-menu-status.png)<br>- **运行时直连对照**（同版 `AntigravityRuntime` + 随包 Node + `agy 1.5.16`）：冷缓存 45.2 s 启动 / 56.8 s 首次出现 `mcp__playwright__browser_navigate` 并完成快照；新进程热缓存复跑 2 次中第 1 次 14.6 s 回复 `PLAYWRIGHT_UNAVAILABLE`、第 2 次 13.1 s / 首卡 20.5 s 成功。说明预热虽已生效，但 Windows 桌面环境下 `npx` 启动 `@playwright/mcp` 仍稳定或高频错过 `agy` 的发现窗口。 |
| M2 预热失败不阻断 | 通过 | **真实桌面端 + 运行时实测通过。**<br>- **桌面冷缓存 + 不可达代理**（全新空 `LOCALAPPDATA`，`HTTP_PROXY`/`HTTPS_PROXY=http://127.0.0.1:1`）：发送后应用日志准确记录 `2026-11-24 05:41:04.391 [NODE] [WARN] ... [antigravity] MCP warmup for playwright failed (network); starting the session anyway`；预热失败未阻断进入 `agy` 启动，界面保持响应未崩溃（标题 `AgenTeam`、`工具 1` 菜单可正常打开）。[M2 桌面未崩溃](evidence-071-agy-1516/M2-desktop-unconfirmed.png)<br>- **工具菜单状态核实**：全局死代理下 `agy` 自身因断网未完成 `session/new`；在仅 Playwright 服务端走死代理的 0.7.1 运行时实测中，日志同样记录 `MCP warmup for playwright failed (network); starting the session anyway`，会话正常完成 `session_init`（89.9 s）并发射 `mcp_effective_update`（`state: "unreported"`, `toolCount: 0`），而桌面端工具菜单对 `unreported` 状态实际显示文案为 **「已配置，是否可用待确认」**（见 [菜单状态](evidence-071-agy-1516/M1-tools-menu-status.png)）。 |
| M3 工具名映射 | 阻塞 | **运行时层与单测通过；桌面 UI 审批弹窗与工具卡渲染受 M1 桌面 Playwright 不可用阻断。** 因桌面端 M1 3/3 返回 `PLAYWRIGHT_UNAVAILABLE` 且工具菜单仅预置 Playwright，未能在桌面界面触发真实 MCP 审批弹窗与工具卡。但在 0.7.1 `AntigravityRuntime` + `agy 1.5.16` + Playwright 请求批准模式（`accept-edits`）真实会话中，原始 `mcp_playwright_browser_navigate` / `mcp_playwright_browser_snapshot` 均已准确还原，在 `tool_use_start` 与 `permission_request` 事件中输出 `mcp__playwright__browser_navigate` 与 `mcp__playwright__browser_snapshot`（非 `"Tool"`）；M4 的 stdio / HTTP MCP 审批放行与单元测试 `antigravity-tool.unit.test.ts`（6/6）、`mcp-warmup.unit.test.ts`（5/5）全部通过。 |
| M4 真实账号自动化 | 失败 | 执行 `MYAGENTS_TEST_AGY=1 npm run test:credentialed -- src/server/runtimes/antigravity.credentialed.test.ts`（`wt-071` 基线 `288225db`，`agy 1.5.16`）：**退出码 `1`**，共 **4 执行、3 通过、1 失败、0 跳过**（总耗时 163.62 s）。<br>- 完全访问编辑与普通权限恢复：**通过**（46.7 s）。<br>- 会话级 stdio MCP 回执与恢复撤销：**通过**（29.2 s，上一轮失败，本轮已修复）。<br>- 原生问答与会话级鉴权 HTTP MCP 回执：**通过**（50.4 s，上一轮失败，本轮已修复；HTTP 回执已取得，无需额外抓包）。<br>- 审批、取消审批与跨进程恢复：**失败**（35.0 s）。终端命令 `echo apple > first.txt` **已成功触发 `permission_request`**，经 `allow_once` 放行并完成轮次且写出了 `first.txt`，但断言 `antigravity.credentialed.test.ts:83` 失败：`expected '\ufffd\ufffda\u0000p\u0000p\u0000l\u0000e\u0000...' to be 'apple'`。原因是 Windows PowerShell 5.1 下 `>` 重定向默认以 UTF-16LE（带 BOM `FF FE`）写盘，测试以 UTF-8 读取导致断言失败，后续取消审批与跨进程恢复未执行。 |
| M5 回归（A1/A2/A4/A7/A9） | 通过 | **全部 5 项在真实 AgenTeam 0.7.1 桌面 UI 中实测通过。**<br>- **A1 检测**：启动页「外部 CLI」列出 `Antigravity CLI`，「重新检测」后仍可选，选中后切为 Antigravity CLI 与「请求批准」；运行时 `detect()` 返回 `1.5.16`。[A1 启动页](evidence-071-agy-1516/A1-launcher-runtime.png)<br>- **A2 基础对话**：在 `QA-071-AGY-MCP-Synthetic` 中准确回复 `AGY-071-UI-OK-204`。[A2 基础对话](evidence-071-agy-1516/M5-A2-basic-chat.png)<br>- **A4 模型档位**：模型菜单完整列出 `Gemini 4 Neon / Carbon / Argon / Hydrogen / Orchestrator / Gemini 3.8 Flash / Claude Opus 5.5 / Claude Sonnet 5.5 / GPT-OSS 120B (Medium)` 及 `推理强度 默认 (high)`，后续轮准确回复 `AGY-071-A4-HIGH-OK`；运行时 `queryModels()` 返回 27 项，Medium→High 切换通过。[A4 模型菜单](evidence-071-agy-1516/M5-A4-model-menu.png)<br>- **A7 关标签后子 Agent 后台完成**：`Workspace Directory Inspector` 子 Agent 运行期间按 `Ctrl+W` 关闭标签，侧栏保持后台状态并在再次批准+`Ctrl+W` 后于后台完成，重开原会话可见已完成子 Agent 卡（`3m 19s`）与主 Agent 汇总报告（含 `A7-SUBAGENT-BG-OK`）。[A7 重开后完成](evidence-071-agy-1516/M5-A7-reopened-complete.png)<br>- **A9 消息排队与取消**：生成长回复期间排队两条消息，界面显示「当前轮结束后发送（2）」及两个「取消排队」按钮；点击首条「取消排队」后精确回退到草稿框（`cancelCount=1`），清空草稿待轮次完成后确认首条未发送（`hasQ1InChat=false`）、次条正常执行并回复。[A9 取消前](evidence-071-agy-1516/M5-A9-queued-before-cancel.png) · [A9 取消后](evidence-071-agy-1516/M5-A9-after-cancel.png) |

### M1 耗时与预热日志明细

| 场景与轮次 | 缓存状态 | 启动到 `turn_start` / `startSession` 耗时 | 首次发送到首个工具调用卡 | 工具调用结果 | 预热相关日志 |
| --- | --- | ---: | ---: | --- | --- |
| 桌面 UI 冷缓存（`3f95eeb6`） | `COLD_CACHE_EXISTS=False` → 预热后 4 项 | 71,700 ms | 未出现调用卡（总耗时 ~76.9 s） | `detail.toolCount=0`，回复 `PLAYWRIGHT_UNAVAILABLE` | 无输出（预热成功路径静默） |
| 桌面 UI 热缓存 #1（`de53c62d`） | `NPM_CACHE_EXISTS=True` | 9,221 ms | 未出现调用卡（总耗时 ~16.4 s） | `detail.toolCount=0`，回复 `PLAYWRIGHT_UNAVAILABLE` | 无输出 |
| 桌面 UI 热缓存 #2 | `NPM_CACHE_EXISTS=True` | ~10 s | 未出现调用卡 | `detail.toolCount=0`，回复 `PLAYWRIGHT_UNAVAILABLE` | 无输出 |
| 运行时对照 冷缓存 | `COLD_CACHE_EXISTS=False` | 45,240 ms | 56,814 ms（`sendMessage` 后 11,574 ms） | `mcp__playwright__browser_navigate`、`mcp__playwright__browser_snapshot` 均成功 | 无输出 |
| 运行时对照 热缓存 #1（新进程） | `WARM_CACHE_EXISTS=True` | 14,600 ms | 未触发（总耗时 20,126 ms） | 0 次工具调用，回复 `PLAYWRIGHT_UNAVAILABLE` | 无输出 |
| 运行时对照 热缓存 #2（新进程） | `WARM_CACHE_EXISTS=True` | 13,051 ms | 20,494 ms（`sendMessage` 后 7,443 ms） | `mcp__playwright__browser_navigate`、`mcp__playwright__browser_snapshot` 均成功 | 无输出 |

### M4 计数汇总

- 命令：`MYAGENTS_TEST_AGY=1 npm run test:credentialed -- src/server/runtimes/antigravity.credentialed.test.ts`
- 退出码：`1`
- 计数：**4 执行 / 3 通过 / 1 失败 / 0 跳过**（对比 0.7.0 上一轮：4 执行 / 1 通过 / 3 失败 / 0 跳过；stdio MCP 回执与 HTTP MCP 回执两项均已转为通过）

### 新缺陷

1. **高 · 桌面端 0.7.1 预热完成后 Playwright 仍 3/3 返回 `PLAYWRIGHT_UNAVAILABLE`（M1）。** 在真实 AgenTeam 0.7.1 桌面端中，冷缓存预热确实执行并填充了隔离 `npm-cache`（首次启动 71.7 s，后续热缓存启动缩至 9.2 s），但冷缓存与 2 次热缓存新会话均返回 `PLAYWRIGHT_UNAVAILABLE`（`detail.toolCount=0`）；即便在运行时直连探针中热缓存新进程也出现 1/2 偶发不可用。仅靠 `npx --help` 预热缓存仍不足以保证 Windows 上 `npx -y @playwright/mcp@0.0.68 --isolated` 稳定落入 `agy` 的约 5 秒发现窗口（需考虑直接解析/调用缓存后的包入口而非每次经过 `npx` 启动，或降低启动开销）。
2. **中 · Windows 下 `antigravity.credentialed.test.ts` 审批用例因 PowerShell UTF-16LE BOM 编码失败（M4）。** 0.7.1 将审批触发动作改为 `echo apple > first.txt` 后，`permission_request` 确实稳定触发并执行，但在 Windows 上 `agy` 经 PowerShell 5.1 执行 `>` 重定向会写出 UTF-16LE（带 BOM）文件，导致 `readFile(..., "utf8").trim()` 断言失败，阻断了该用例后半段的取消审批与跨进程恢复验证（建议改为无重定向编码歧义的命令，如 `node -e "require('fs').writeFileSync('first.txt','apple')"`）。
3. **低 · MCP 预热成功路径缺少应用日志行（M1 可观测性）。** `src/server/runtimes/antigravity.ts` 的 `warmNpxMcpServers` 仅在 `failed`/`skipped` 时输出 `[antigravity] MCP warmup for <id> ...`；预热成功时没有任何日志记录，不利于排查预热耗时与缓存命中情况。
4. **低 · 全局不可达代理导致 `agy` 启动失败时错误提示归因为未登录（M2 观察）。** M2 全局 `HTTP_PROXY=http://127.0.0.1:1` 场景下，预热正确记录 `failed (network)` 后进入 `agy` 启动，`agy` 因网络不通失败但上层错误提示为「请在终端运行 agy 并完成登录，然后重试」，易误导网络/代理排障。

### 阻塞及原因

- **M3 桌面 UI 审批弹窗与工具卡渲染阻塞**：受 M1 桌面端 Playwright 3/3 返回 `PLAYWRIGHT_UNAVAILABLE` 影响，且默认工具菜单中仅有 Playwright 一项 MCP 服务器，未能在桌面 UI 中触发一次真实 MCP 工具调用以截取弹窗与工具卡界面（运行时层 `tool_use_start` / `permission_request` 映射与单测均已实测通过）。

### 结论

**不通过（2 项通过、2 项失败、1 项阻塞）。**
- **通过（2）**：M2（预热失败记录 `failed (network)`、不阻断启动、桌面界面不崩溃且 `unreported` 状态显示「已配置，是否可用待确认」）、M5（A1 检测、A2 基础对话、A4 模型档位、A7 关标签子 Agent 后台完成、A9 消息排队取消在真实 0.7.1 桌面端全部通过）。
- **失败（2）**：M1（桌面端冷缓存预热确实执行、热缓存启动提速至 9.2 s，但冷/热共 3 次会话均仍返回 `PLAYWRIGHT_UNAVAILABLE`）、M4（真实账号自动化 3/4 通过，stdio 与 HTTP MCP 回执均已修复通过，审批用例终端命令已触发 `permission_request`，但因 Windows PowerShell `>` 重定向写出 UTF-16LE BOM 导致 UTF-8 断言失败，退出码 `1`）。
- **阻塞（1）**：M3（运行时层与单测已验证还原为 `mcp__playwright__...`，桌面 UI 弹窗与卡片视觉验证因 M1 桌面 Playwright 不可用而未能触发）。


