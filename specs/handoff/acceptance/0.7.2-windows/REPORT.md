# AgenTeam 0.7.2 Windows 真机验收报告

验收日期：2026-11-24（太平洋时间）。本轮在 Windows 真机已安装的 **AgenTeam 0.7.2** 正式版上，对随包 Playwright MCP、旧配置迁移、Antigravity 遗留项（M3/M4/死代理提示）、团队办公室与知识库冒烟以及基础回归进行了独立验收。未修改任何产品代码。

## 环境与隔离

| 项目 | 记录 |
| --- | --- |
| App 版本 | AgenTeam Windows 正式版 **0.7.2**（`ProductVersion=0.7.2`、`FileVersion=0.7.2`，启动日志 `[boot] v=0.7.2 build=release os=windows-x86_64`）；对应源码标签 `v0.7.2`（`f92e1b3a`）。 |
| agy 版本 | **1.5.16**（`%LOCALAPPDATA%\agy\bin\agy.exe --version`）；已登录 Gemini 订阅。Claude 与 Codex 使用已有账号，未采集任何令牌或账号标识。 |
| 隔离方式 | 1) 在临时目录创建独立 detached git worktree（基线 `v0.7.2` / `f92e1b3a`）运行 M4 自动化与 P2 迁移验证，未修改主工作区、产品代码或 `main`。<br>2) 验收开始时本机无运行中的 `AgenTeam.exe`。以全新空白 `LOCALAPPDATA`（保证 `npm-cache` 为冷缓存且不触碰日常 `%LOCALAPPDATA%\npm-cache`）、进程级 `PATH` 补入 `agy` 目录与 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=19222` 启动 0.7.2 桌面端；所有桌面操作限定在新建的空白合成项目 `QA-072-SYNTHETIC`（关闭助理模板）。<br>3) 截图均裁切右侧主视图（团队办公室仅截取放大到 `QA-072-SYNTHETIC` 部门后的画面），不露出日常项目、会话、账号或令牌。应用日志仅从各轮预记字节偏移之后读取并过滤本轮关联行。 |
| 是否恢复日常实例 | **已恢复**。验收结束后已终止带调试端口及测试代理的进程，并以正常无调试参数重新启动 `E:\Applaction\AgenTeam\AgenTeam.exe`。 |

## 用例结果表

| 用例 | 结果 | 证据与说明 |
| --- | --- | --- |
| P1-a 随包 Playwright 文件检查 | 失败（路径差异） | 安装目录下**不存在** `Resources\playwright-control\@playwright\mcp\cli.js` 或 `playwright-control\@playwright\mcp\cli.js`；实际由 `tauri.conf.json` 打包到 `E:\Applaction\AgenTeam\node_modules\@playwright\mcp\cli.js`（**971 字节**），同目录存在 `node_modules\playwright\cli.js`（698 字节）与 `node_modules\playwright-core\cli.js`（713 字节）。`bundledPlaywrightMcpEntry()` 经 `createRequire` 回退解析到该路径并能拉起。 |
| P1-b Antigravity CLI 随包 Playwright（×3） | 失败（2/3） | **第 1 轮（冷启动）失败**：`Starting antigravity session` → `turn_start` 耗时 **13.57 s**，`agy` 自身日志 `acp-worker-20261124_171530-13760.log` 记录 `W1124 17:15:43.226602 session_handlers.go:401] Failed to preload the session's MCP servers: context deadline exceeded`（10.0 s 超时），0 次 Playwright 调用（改走 Bash 失败后回复未加载 Playwright）。<br>**第 2、3 轮（新会话）通过**：进程命令行核实为 `E:\Applaction\AgenTeam\nodejs\node.exe E:\Applaction\AgenTeam\node_modules\@playwright\mcp\cli.js --isolated`（无 `npx`）；第 2 轮启动 4.49 s、首卡 11.7 s、`detail.toolCount=2`、回复页面标题为空；第 3 轮启动 7.3 s、首审批 18.5 s；工具卡与审批弹窗均正确显示 `mcp__playwright__browser_navigate` / `playwright: browser_navigate`。 [P1-b R3](evidence-072/P1-antigravity-r3.png) |
| P1-c Claude 内置 随包 Playwright（×3） | 通过（3/3） | 3 轮全部成功调用随包 `node.exe ...\node_modules\@playwright\mcp\cli.js --isolated`：MCP 分发耗时 2.06–3.57 s，首卡耗时 ~9.1 s，依次执行 `mcp__playwright__browser_navigate` 与 `mcp__playwright__browser_snapshot` 并准确回复空白页无标题。 [P1-c R1](evidence-072/P1-claude-r1.png) |
| P1-d 托管 Codex 随包 Playwright（×3） | 失败（0/3） | 默认配置下先命中两个阻断错误：默认模型 `gpt-6.2-astra` 报 `400 model_not_in_accessible_catalog`（[截图](evidence-072/P1-codex-r1.png)），切 `GPT-6-Sol` 后默认推理强度 `minimal` 报 `400 unsupported_value`（[截图](evidence-072/P1-codex-sol.png)）。显式选择 `GPT-6.1 Terra` + `medium` 后执行 3 轮：第 1 轮调用 `mcp__node_repl__js` 后回复「无法读取页面标题：Playwright 加载失败」；第 2 轮以 `turn/completed status=failed` 失败；第 3 轮虽回复「页面标题为空」，但日志核实调用的是 `mcp__node_repl__js` 与 `Bash`，**3 轮均 0 次调用 Playwright MCP**。 |
| P2 旧配置迁移（P2-a/b/c/d） | 阻塞（桌面端隔离受限；代码层通过） | **阻塞原因与已尝试方法**：已检查 `v0.7.2:src-tauri/src/app_dirs.rs`，Windows 上 `myagents_data_dir()` 通过 `dirs::home_dir()`（Win32 `SHGetKnownFolderPath`）固定指向 `%USERPROFILE%\.agenteam`，不响应 `LOCALAPPDATA` 或 `USERPROFILE` 重定向，因此无法在不触碰日常 `config.json` 的前提下让桌面二进制直接加载隔离的 `~\.agenteam` 副本完成 P2-a/b 写盘。<br>**独立副本代码层验证（通过）**：在临时隔离目录写入题述旧 `config.json`（含全局 `playwright`、Agent `mcpServersJson` 与自建 `my-pw`），调用 `v0.7.2` 生产迁移函数 `migratePlaywrightPresetToBundled()` 与 `buildMcpStdioLaunchConfig()`：首轮返回 `changed=true`，全局 `playwright` 变为 `__bundled_playwright_mcp__` + `["--browser=firefox"]`，Agent 快照变为 `__bundled_playwright_mcp__` + `[]`，`my-pw` 一字未改；二轮返回 `changed=false`（完全幂等）；`src/shared/mcpConfig.test.ts` 8/8 通过。详见下文脱敏片段。 |
| P3-M3 桌面工具名映射与审批 | 通过 | 在真实 0.7.2 桌面端请求批准模式下触发 Playwright 调用，工具卡显示 `Playwright 浏览器` + `mcp__playwright__browser_navigate`（以及 `mcp__playwright__browser_snapshot`），审批弹窗显示 `需授权 playwright: browser_navigate about:blank`，不再显示泛称 `"Tool"`；前端日志记录 `[TabProvider] Queueing pendingPermission for: mcp__playwright__browser_navigate`。 [M3 审批与工具卡](evidence-072/P3-M3-approval-and-card.png) |
| P3-M4 真实账号自动化 | 通过 | 在 `wt-072`（`f92e1b3a`）运行 `MYAGENTS_TEST_AGY=1 npm run test:credentialed -- src/server/runtimes/antigravity.credentialed.test.ts`：**退出码 `0`**，**4 执行 / 4 通过 / 0 失败 / 0 跳过**（总耗时 142.71 s）。0.7.1 遗留的 Windows PowerShell UTF-16LE 编码失败已修复，审批+取消+跨进程恢复、stdio MCP 回执、鉴权 HTTP MCP 回执、完全访问编辑与恢复全部通过。 |
| P3-低 全局不可达代理错误提示 | 通过（已不再误导登录） | 设置进程级 `HTTP_PROXY`/`HTTPS_PROXY=http://127.0.0.1:1` 启动 0.7.2 并发起 Antigravity 会话，界面显示原文：`⚠ failed to construct executor: plan model not specified; you must specify a valid model`（日志 `model_list_done status=error`）。已**不再**误导为「请在终端运行 agy 并完成登录」，但仍未直接提示网络/代理不可达。 [死代理提示](evidence-072/P3-dead-proxy-error.png) |
| P4-a 团队办公室冒烟 | 通过 | 打开「智能体团队」页：在 `1200×829`、`1440×880`、`940×680` 视图下均无水平/垂直滚动条（`overX=false, overY=false`）；点击部门名 `QA-072-SYNTHETIC` 可放大单部门视图，点击「返回全景」恢复；明/暗两种主题渲染正常、标签可读。 [浅色单部门](evidence-072/P4-team-office-zoom-light.png) · [深色单部门](evidence-072/P4-team-office-zoom-dark.png) |
| P4-b 知识库入口冒烟 | 失败（0.7.2 未包含） | 启动页顶部**不存在**「对话 / 知识库」切换入口（DOM 检索 `KB_SWITCHER: []`）。核对 git 历史确认 K9 知识库 UI（`9aafee88`）合并于 `v0.7.2`（`f92e1b3a`）之后，0.7.2 桌面包未包含该入口。 [启动页无切换](evidence-072/P4-launcher-no-kb-switcher.png) |
| P4-c 系统技能版本 | 失败（仍为 v79） | 首次启动日志为 `[system-skills] Synced v79 (complete=true)`（`v0.7.2` 源码 `SYSTEM_SKILLS_VERSION = "79"`），未升级至版本 80；但 `agenteam-knowledge` 与 `agenteam-knowledge-curator` 已在 v79 列表中正常同步。 |
| P5 回归（A1/A2/A4/A7/A9 + Claude/Codex + Managed Codex ultra） | 失败（部分失败） | **通过项**：Antigravity `A1` 检测与「重新检测」正常、`A2` 准确回复 `AGY-072-A2-OK`、`A4` 模型菜单列出 12 项、`A7` 子 Agent 运行期 `Ctrl+W` 关标签后重开会话状态完整保留（[A7 截图](evidence-072/P5-A7-reopened.png)）；Claude 普通/工具对话通过；Codex（显式 `GPT-6.1 Terra` + `medium`）普通对话准确回复 `CODEX-072-OK`。<br>**失败/未覆盖项**：1) 启动页 Codex 推理强度菜单仅含 `minimal / low / medium / high / xhigh`，**无 `ultra` 档位**（[推理强度截图](evidence-072/P5-codex-efforts.png)）；2) `A9` 本轮脚本在生成结束前未捕捉到「取消排队」按钮（`P5_A9_CANCEL_BTNS: 0`），未能完成 0.7.2 下的取消排队闭环验证。 |

## P1 耗时表（Runtime × 冷/热 × 3 轮）

| Runtime | 轮次与缓存 | 启动到 `turn_start` 耗时 | 发送到首个 Playwright 工具卡 | 工具调用结果 | 备注 |
| --- | --- | ---: | ---: | --- | --- |
| Antigravity CLI | 第 1 轮（冷） | 13,569 ms | 未出现（0 次 Playwright 调用） | **失败** | `agy` 日志 10.0 s `Failed to preload the session's MCP servers: context deadline exceeded` |
| Antigravity CLI | 第 2 轮（新会话） | 4,485 ms | 11,713 ms | **通过**（`toolCount=2`） | `node.exe ...\node_modules\@playwright\mcp\cli.js --isolated`，回复标题为空 |
| Antigravity CLI | 第 3 轮（新会话） | 7,319 ms | ~18,465 ms（首审批入队） | **通过** | 触发 `mcp__playwright__browser_navigate` 审批并完成 |
| Claude 内置 | 第 1 轮（冷） | ~3,500 ms | 9,052 ms | **通过** | `MCP dispatch outcome=ready elapsedMs=2480`，`navigate` + `snapshot` |
| Claude 内置 | 第 2 轮（新会话） | 3,674 ms | ~6,000 ms | **通过** | `MCP dispatch outcome=ready elapsedMs=2058`，回复空白页无标题 |
| Claude 内置 | 第 3 轮（新会话） | 4,096 ms | 9,139 ms | **通过** | `MCP dispatch outcome=ready elapsedMs=3572`，总耗时 15.5 s |
| 托管 Codex | 第 1 轮（`GPT-6.1 Terra`+`medium`） | 1,650 ms | 未出现（0 次 Playwright 调用） | **失败** | 调用 `mcp__node_repl__js`，回复「Playwright 加载失败」 |
| 托管 Codex | 第 2 轮 | 1,207 ms | 未出现 | **失败** | `turn/completed status=failed` |
| 托管 Codex | 第 3 轮 | 6,790 ms | 未出现（0 次 Playwright 调用） | **失败** | 调用 `mcp__node_repl__js` 与 `Bash` 绕过，非 Playwright MCP |

## P2 迁移前后配置片段（隔离副本脱敏）

**迁移前（手工构造旧配置）：**

```json
{
  "mcpServers": [
    { "id": "playwright", "name": "Playwright", "type": "stdio", "command": "npx", "args": ["-y", "@playwright/mcp@latest", "--browser=firefox"], "isBuiltin": true },
    { "id": "my-pw", "name": "My Custom PW", "type": "stdio", "command": "npx", "args": ["@playwright/mcp@latest"] }
  ],
  "agents": [
    { "id": "qa-agent-1", "name": "QA Agent", "mcpServersJson": "[{\"id\":\"playwright\",\"name\":\"Playwright\",\"type\":\"stdio\",\"command\":\"npx\",\"args\":[\"@playwright/mcp@0.0.68\"],\"isBuiltin\":true}]" }
  ]
}
```

**迁移后（`migratePlaywrightPresetToBundled` 首轮 `changed=true`，二轮 `changed=false` 幂等）：**

```json
{
  "mcpServers": [
    { "id": "playwright", "name": "Playwright", "type": "stdio", "command": "__bundled_playwright_mcp__", "args": ["--browser=firefox"], "isBuiltin": true },
    { "id": "my-pw", "name": "My Custom PW", "type": "stdio", "command": "npx", "args": ["@playwright/mcp@latest"] }
  ],
  "agents": [
    { "id": "qa-agent-1", "name": "QA Agent", "mcpServersJson": "[{\"id\":\"playwright\",\"name\":\"Playwright\",\"type\":\"stdio\",\"command\":\"__bundled_playwright_mcp__\",\"args\":[],\"isBuiltin\":true}]" }
  ]
}
```

## 新缺陷表

| 严重度 | 模块/用例 | 复现步骤与现象 | 原始日志行 |
| --- | --- | --- | --- |
| **高** | P1-b Antigravity 冷启动 Playwright | 全新隔离 `LOCALAPPDATA` 首次启动 0.7.2，在 `QA-072-SYNTHETIC` 启用 Playwright 并用 Antigravity CLI 发送请求。虽已直连随包 `node_modules\@playwright\mcp\cli.js`，但首次冷启动仍超过 `agy` 的 10 秒 MCP 预加载窗口，导致首轮 0 次 Playwright 调用。 | `W1124 17:15:43.226602 67 session_handlers.go:401] Failed to preload the session's MCP servers: context deadline exceeded` |
| **高** | P1-d 托管 Codex Playwright 与默认模型/档位 | 1) 在启动页切至 Codex 使用默认模型直接发消息，报 `400 model_not_in_accessible_catalog`（`gpt-6.2-astra`）；2) 切 `GPT-6-Sol` 用默认推理强度发消息，报 `400 unsupported_value`（`'minimal'` 不受支持）；3) 切 `GPT-6.1 Terra` + `medium` 后 3 轮均未成功调用 Playwright MCP（0/3），且日志持续报 `Codex MCP transport failed`。 | `[external-runtime] [codex] Codex MCP transport failed detail={"present":true,"chars":345,...}` / `[codex] mcpServer/startupStatus/updated ... status=failed code=MCP_STARTUP_FAILED` |
| **中** | P4-b / P4-c 0.7.2 版本范围偏差 | 0.7.2 安装包不包含启动页「对话 / 知识库」切换入口，且系统技能版本仍为 `v79`（未升级至 `v80`）；K9 知识库入口合并于 `v0.7.2` 之后。 | `2026-11-24 17:13:08.057 [RUST ] [INFO ] [system-skills] Synced v79 (complete=true)` |
| **中** | P5 Managed Codex 缺少 `ultra` 推理强度 | 启动页选择 Codex 与 `GPT-6.1 Terra` 打开推理强度菜单，仅列出 `minimal / low / medium / high / xhigh`，无 `ultra` 选项。 | —（见 `evidence-072/P5-codex-efforts.png`） |
| **低** | P1-a 随包资源路径与文档/预期不一致 | 安装目录下无 `Resources\playwright-control\@playwright\mcp\cli.js`，实际落在 `node_modules\@playwright\mcp\cli.js`（依赖 `createRequire` 回退生效）。 | `tauri.conf.json:53` 映射至 `node_modules/@playwright/mcp` |
| **低** | 单元测试 Windows 兼容 | 在 `v0.7.2` 运行 `npx vitest run src/server/utils/mcp-command.unit.test.ts` 时，`checks custom executables against their configured PATH and workspace` 在 Windows 上失败（145 行 `expected true to be false`）。 | `FAIL src/server/utils/mcp-command.unit.test.ts:145` |

## 总结论

**不通过（4 项通过、7 项失败、1 项阻塞）。P1 未达到 9/9（实测 5/9：Antigravity 2/3、Claude 3/3、Codex 0/3）。**

- **已验证转好**：随包直启路径（`node.exe ...\node_modules\@playwright\mcp\cli.js --isolated`，无 `npx`）在 Claude 内置（3/3）与 Antigravity 热启动（2/3）上表现稳定且速度大幅提升；0.7.1 遗留的 **M3**（桌面审批弹窗与工具卡名还原为 `mcp__playwright__...`）与 **M4**（真实账号自动化 **4/4 全过**、退出码 `0`）均已完全修复；团队办公室单部门放大、无滚动条与明暗主题冒烟通过；死代理提示已不再误导为登录问题。
- **未通过原因**：P1 未达 9/9（Antigravity 首次冷启动仍命中 `agy` 10 s preload 超时，Codex 3 轮 0 次 Playwright MCP 调用且默认模型/推理强度触发 400）；0.7.2 未包含知识库启动页入口且系统技能仍为 v79；Codex 启动页未见 `ultra` 推理强度。
- **阻塞说明**：P2 桌面端真实启动写盘因 Windows `dirs::home_dir()` 无法经 `LOCALAPPDATA` 重定向隔离目录而阻塞（已通过隔离配置文件实测 `v0.7.2` 生产迁移函数，确认 P2-b/c/d 逻辑与幂等性正确）。
