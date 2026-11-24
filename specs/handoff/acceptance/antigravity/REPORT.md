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
