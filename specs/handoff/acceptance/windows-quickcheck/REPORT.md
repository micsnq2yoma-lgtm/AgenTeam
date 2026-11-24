# Windows 三项快速验收

验收日期为 2026-11-23。结果基于真实 Windows 桌面 App。

| 用例 | 结果 | 实测证据 |
| --- | --- | --- |
| X1-R Codex 运行集 | **阻塞** | 在候选 App 的模型设置看到隔离账户已装 `0.171.0 (win32-x64)`。用同一启动环境直接调用旧式签名检查，退出 `1`，复现 `CouldNotAutoloadMatchingModule` 和 CLIXML。干净安装及损坏归档的两次实测未完成。[运行集初始状态](evidence/X1-R-initial-runtime.jpg) |
| U3 项目控件 | **通过** | 中文深色和英文浅色各测一遍。已选项目只有图标、名称、下拉小箭头，点击展开真实列表。将四个合成项目归档后，两种组合都显示对齐的「选择项目」或 `Select project`。[中文项目](evidence/U3-project-zh-dark.jpg) · [中文列表](evidence/U3-menu-zh-dark.jpg) · [中文空态](evidence/U3-empty-zh-dark.jpg) · [英文项目](evidence/U3-project-en-light.jpg) · [英文列表](evidence/U3-menu-en-light.jpg) · [英文空态](evidence/U3-empty-en-light.jpg) |
| U5 新助理介绍页 | **阻塞** | 新旧介绍页的显示检查通过。新助理首次打开依次显示「我怎么记住你」「做事」「首次使用」。输入 `/BOOTSTRAP` 后发送按钮禁用，界面要求先配置模型供应商，未执行到删除「首次使用」。[新介绍页上半部](evidence/U5-new-intro-upper.jpg) · [下半部](evidence/U5-new-intro-lower.jpg) · [已有助理](evidence/U5-existing-intro.jpg) · [发送受阻](evidence/U5-bootstrap-blocked.jpg) |

## 构建与隔离

- Windows 11 26H2 x64，OS Build `26300.9974`。构建源为 `main` 提交 `daf4e223e5fe08db716d319962ca46ae7932c63c`。对该 SHA 分别运行 `git merge-base --is-ancestor <修复提交> <构建SHA>`，`daf4e22`、`ce95e80`、`47ffb9f`、`845f9e2` 均返回 `0`。
- 在独立检出中复用本机 Windows 构建缓存，先运行 `npm.cmd run build:assets`，再通过 `npm.cmd run tauri:build:prepared -- --debug --no-bundle --target x86_64-pc-windows-msvc --config src-tauri/tauri.windows.conf.json` 构建，两步均退出 `0`。构建覆盖仅用于验收实例标识 `io.github.micsnq2yoma-lgtm.agenteam.quickcheckqa`，并关闭重复构建前置命令与更新器产物。候选程序沿用源码版本号 `0.6.8`，实际运行的是上述提交的 Tauri 可执行文件。SHA-256 为 `3e2019e20703b2da15a49c23bf5b5ccfa6110d73f4dcf8e2b0656a984f60c64d`。
- 以专用 Windows 标准用户 `AgenQALaunch69` 加载用户配置文件并启动候选 App。进程按 Windows Known Folder 查到 `C:\Users\AgenQALaunch69`，数据根为该账户专用的 `C:\Users\AgenQALaunch69\.agenteam`。与宿主用户的目录物理分开。沿用隔离账号内上一轮的合成助理作为旧版本对照，另建 `Quickcheck Assistant`。没有登录或配置模型供应商。

## X1-R 运行集

上一轮 [托管组件报告](https://github.com/micsnq2yoma-lgtm/AgenTeam/blob/5cfc5125829dd1f99492068bf5555650c3a805ba/specs/handoff/acceptance/managed-components/REPORT.md) 留下了故障标识，未保存完整的 `PSModulePath`。本轮让 App 继承 `C:\ProgramData\AgenTeam-Quickcheck-daf4e22\broken-modules;C:\ProgramData\AgenTeam-Quickcheck-daf4e22\pwsh7-modules`，后者指向实机 PowerShell 7 的模块目录。前者放入独立的测试模块，使旧式自动加载走同样的失败路径。在同一 Windows 测试身份和该环境中，用 Windows PowerShell 5.1 无配置、非交互调用 `Get-AuthenticodeSignature` 检查官方运行集二进制。日志摘要为 `exit=1, CouldNotAutoloadMatchingModule=true, CLIXML=true`。

另在同一机器的外层验收进程做了独立的原生探针。用相同的坏路径初始化 Windows PowerShell 5.1，再固定系统 5.1 模块路径并显式载入 `Microsoft.PowerShell.Security`。该样本的 Authenticode 状态为 `Valid`，退出 `0`。该结果仅覆盖签名子步骤。

本轮启动前隔离账户的 `runtimes\codex\installed.json` 已存在，模型设置显示 `v0.171.0`、`win32-x64` 和「未登录」。这是一份已有的安装基线。当前操作员访问该 Windows 用户的数据目录时收到 `Access denied`。用于暂存旧运行集的夹具启动请求由 Windows 返回「操作已被用户取消」，因此未准备出干净的安装目录。没有拿到干净安装后或损坏归档后的 `installed.json`，两种桌面结果均阻塞。

## U3 像素观察

在 1203 × 831 的原始截图中，读取控件图形和占位文字最左侧可见笔画。已选项目的图标约在 `x=425–426`，输入文字约在 `x=426`，偏差约 **0–1 px**。空态加号约在 `x=428`，比输入文字向右约 **2 px**。两种语言和主题组合结果一致。空态通过 App 自带的可逆归档获得，四个项目均保留在归档区，启动页无可选项目。

## U5 内容与后续动作

- 「我怎么记住你」逐项显示了解你、日常学习、随时更正、多个助理共享、主动提醒。
- 「做事」逐项显示任务完成后自动汇报、安装技能、知识库。底部是「首次使用」。已有的合成助理仍显示原介绍和原有「首次使用」，未出现新模板的两个章节。
- 在新助理聊天中输入 `/BOOTSTRAP`，发送按钮禁用，辅助功能文本为「请前往设置页面设置模型供应商」。本轮没有使用账号或模型，因此没有观察到指令完成前后的文件变化。

## 新缺陷与结论

已完成的检查未发现新缺陷。共 **1 项通过，0 项失败，2 项阻塞**。U3 通过。X1-R 的两次安装结果和 U5 的引导完成后清理仍未获得验收证据。