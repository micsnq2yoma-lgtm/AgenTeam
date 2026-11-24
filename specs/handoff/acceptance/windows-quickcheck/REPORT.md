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

## X1-R 脚本级复验

2026-11-23，Windows 11 26H2 x64（Build `26300.9974`），原生 Windows PowerShell `5.1.26100.9818`。`git merge-base --is-ancestor ce95e80 origin/main` 返回 `0`，本地 `origin/main` 指向 `daf4e223e5fe08db716d319962ca46ae7932c63c`。调用参数与产品一致。所有调用均在未提升权限的进程中完成。两个 UTF-8 脚本和输出存放在隔离临时目录，环境变量的改动限于子进程。[产品中的 PowerShell 片段](../../../../src-tauri/src/managed_codex.rs)

目标为 `C:\Windows\System32\notepad.exe`，签名证书主体为 `CN=Microsoft Windows, O=Microsoft Corporation, L=Redmond, S=Washington, C=US`。测试前后可执行文件的 SHA-256 相同，均为 `2d0b3948c1edc5c6a1d1f3e3658613252cfbbfa1b98b46cb7cc8e5bf2efb158f`。

### 命令与环境

`old.ps1` 的内容如下。

```powershell
$ErrorActionPreference = 'Stop'
$sig = Get-AuthenticodeSignature -LiteralPath 'C:\Windows\System32\notepad.exe'
$cert = $sig.SignerCertificate
[string]$sig.Status
if ($cert -ne $null) { [System.BitConverter]::ToString($cert.GetCertHash('SHA256')).Replace('-','').ToLowerInvariant() }
```

`new.ps1` 仅在第一行后插入以下两行。

```powershell
$env:PSModulePath = (Join-Path $PSHOME 'Modules') + ';' + (Join-Path $env:windir 'System32\WindowsPowerShell\v1.0\Modules')
Import-Module Microsoft.PowerShell.Security -ErrorAction Stop
```

对每份脚本按以下方式编码和启动。用子进程的两条重定向流直接捕获 stdout 与 stderr 字节，再单独记录进程退出码。

```powershell
$script = [IO.File]::ReadAllText($scriptPath, [Text.Encoding]::UTF8)
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
& "$env:windir\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded
```

| 环境 | 启动时的 `PSModulePath` |
| --- | --- |
| (a) 干净 | 由无配置文件的 Windows PowerShell 5.1 父进程从缺省环境生成 `%USERPROFILE%\Documents\WindowsPowerShell\Modules;C:\Program Files\WindowsPowerShell\Modules;C:\Windows\system32\WindowsPowerShell\v1.0\Modules`。测试子进程直接继承。 |
| (b) 仅 PowerShell 7 | `C:\Program Files\WindowsApps\OpenAI.CodexPrimaryRuntime.v26-1118-904-0_26.1118.904.0_x64__3k8sg7r9htsxt\dependencies\native\powershell\Modules`。机器有 PowerShell `7.6.9` 的随应用部署副本，常规安装位置 `C:\Program Files\PowerShell\7\Modules` 不存在，使用实际的模块目录。 |
| (c) 不存在的目录 | `C:\no-such-dir`，已确认不存在。 |
| (d) 上轮复现环境 | `C:\ProgramData\AgenTeam-Quickcheck-daf4e22\broken-modules;C:\ProgramData\AgenTeam-Quickcheck-daf4e22\pwsh7-modules`。沿用上轮两个原路径。第一个目录是会在加载时抛错的隔离测试模块，第二个目录经 junction 指向 (b) 的 PowerShell 7 模块目录。 |
| (e) 空字符串 | 子进程启动时显式传入 `PSModulePath=''`，只运行 `new.ps1` 一次。 |
| 补测（直接继承 PowerShell 7） | 未覆盖当前 PowerShell 7 父进程的路径。其顺序为 `%USERPROFILE%\Documents\PowerShell\Modules;C:\Program Files\PowerShell\Modules;c:\program files\windowsapps\openai.codexprimaryruntime.v26-1118-904-0_26.1118.904.0_x64__3k8sg7r9htsxt\dependencies\native\powershell\Modules;%USERPROFILE%\Documents\WindowsPowerShell\Modules;C:\Program Files\WindowsPowerShell\Modules;C:\Windows\system32\WindowsPowerShell\v1.0\Modules`。 |

### 逐次结果

单元格为「退出码 / stdout / stderr」。`V` 为下列两行签名输出，`—` 为零字节。`P1`、`P2` 分别含一条和两条 CLIXML 进度记录；`E` 为完整的 CLIXML 加载错误。相同标识的输出字节完全一致。25 次指定调用和 6 次补测均留有原始字节。

| 启动环境 | old 第 1 次 | old 第 2 次 | old 第 3 次 | new 第 1 次 | new 第 2 次 | new 第 3 次 |
| --- | --- | --- | --- | --- | --- | --- |
| (a) Windows PowerShell 5.1 默认 | `0 / V / P2` | `0 / V / P2` | `0 / V / P2` | `0 / V / P1` | `0 / V / P1` | `0 / V / P1` |
| (b) 仅 PowerShell 7 | `1 / — / E` | `1 / — / E` | `1 / — / E` | `0 / V / P1` | `0 / V / P1` | `0 / V / P1` |
| (c) 不存在的目录 | `0 / V / P2` | `0 / V / P2` | `0 / V / P2` | `0 / V / P1` | `0 / V / P1` | `0 / V / P1` |
| (d) 上轮复现环境 | `1 / — / E` | `1 / — / E` | `1 / — / E` | `0 / V / P1` | `0 / V / P1` | `0 / V / P1` |
| (e) 空字符串 | — | — | — | `0 / V / P1` | — | — |
| 补测（直接继承 PowerShell 7） | `1 / — / E` | `1 / — / E` | `1 / — / E` | `0 / V / P1` | `0 / V / P1` | `0 / V / P1` |

### 输出原文

以下按字节相同的输出去重。stderr 按 CP936 解读，保留完整 XML 和 PowerShell 自带截断。证据文件含逐次退出码、流的 SHA-256 和 Base64 编码的原始字节。[逐次原始字节](evidence/X1-R-script-streams.json)

**V · stdout**

```text
Valid
d06ec42eadb861493242595674730d1a93bce619e745df855bb550b979869ed4
```

**E · stderr**

```text
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">Get-AuthenticodeSignature : 在模块“Microsoft.PowerShell.Security”中找到“Get-AuthenticodeSignature”命令，但无法加载该模_x000D__x000A_</S><S S="Error">块。有关详细信息，请运行“Import-Module Microsoft.PowerShell.Security”。_x000D__x000A_</S><S S="Error">所在位置 行:2 字符: 8_x000D__x000A_</S><S S="Error">+ $sig = Get-AuthenticodeSignature -LiteralPath 'C:\Windows\System32\no ..._x000D__x000A_</S><S S="Error">+        ~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : ObjectNotFound: (Get-AuthenticodeSignature:String) [], ParentContainsErrorRecordExceptio _x000D__x000A_</S><S S="Error">   n_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : CouldNotAutoloadMatchingModule_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
```

**P1 · stderr**

```text
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><Obj S="progress" RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCustomObject</T><T>System.Object</T></TN><MS><I64 N="SourceId">1</I64><PR N="Record"><AV>正在准备首次使用模块。</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj></Objs>
```

**P2 · stderr**

```text
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><Obj S="progress" RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCustomObject</T><T>System.Object</T></TN><MS><I64 N="SourceId">1</I64><PR N="Record"><AV>正在准备首次使用模块。</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj><Obj S="progress" RefId="1"><TNRef RefId="0" /><MS><I64 N="SourceId">2</I64><PR N="Record"><AV>正在准备首次使用模块。</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj></Objs>
```

### 缺陷与结论

**脚本级复验通过，无新缺陷或阻塞。** 旧脚本在 (b)、(d) 和补测中反复报 `CouldNotAutoloadMatchingModule`。规定的 13 次新脚本调用全部退出 `0`，输出 `Valid` 和相同的证书 SHA-256，补测的 3 次也通过。成功调用的 stderr 只有进度记录。

结果足以确认 `ce95e80` 加入的两行能在这些启动环境中使产品采用的同一签名查询和证书哈希提取正常工作。复验样本是微软签名的 Notepad，范围限于 PowerShell 子步骤。Codex 运行集的下载、安装和实际证书比对仍需端到端证据，上方桌面用例继续记为阻塞。本轮跳过 Rust CLIXML 转换单元测试。
