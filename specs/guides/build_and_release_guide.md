# AgenTeam 桌面发布

AgenTeam 0.6.0 首发面向 Windows x64。安装包和更新清单都由 [AgenTeam Releases](https://github.com/micsnq2yoma-lgtm/AgenTeam/releases) 分发。普通界面修改运行仓库根目录的 `preview_windows.cmd`，桌面级验收使用候选安装包。

## 生成候选包

1. 用 `npm version 0.6.1 --no-git-tag-version` 这类命令设置下个版本，检查并提交应用、npm 锁和两个 Cargo 包的版本变更。首发已设为 0.6.0。
2. 将选定的提交推送到 AgenTeam 仓库的 `codex/agenteam-release` 分支。Windows 工作流构建所推送的提交，准备锁定的 Node、Rust 和资源，执行类型检查，生成 NSIS 安装包。Windows ONNX Runtime 的 NuGet 包来自微软，同版本源码提交和发布摘要均固定在资源锁中。
3. 从该次成功运行下载 `AgenTeam-Windows-x64-<完整提交 SHA>` artifact 并解压到独立目录。核对运行所属仓库、提交和版本。目录内应有 `AgenTeam_<版本>_x64-setup.exe` 和 `SHA256SUMS`。
4. 在该提交的签名工作站上运行 `.\publish_windows.ps1 -Directory <解压目录>`。脚本先比对构建摘要，再签署 EXE 并用客户端内嵌公钥验证签名，生成 `.exe.sig`、`latest.json` 和最终 `SHA256SUMS`。本地首次构建可运行 `.\setup_windows.ps1`，随后用 `.\build_windows.ps1 -Candidate -SkipPortable` 生成相同类型的安装包。手工签名前需在产出目录记录原始 EXE 的 SHA-256，格式与工作流一致。

## 签名及保管

Tauri 更新签名用于客户端验证安装包。Windows 文件属性显示的发布者来自 Authenticode。首个候选包使用 Tauri 签名，尚无 Authenticode 发布者证书。接入代码签名服务后，先对最终 EXE 做 Authenticode 签名和时间戳，再计算构建摘要及 Tauri 签名。签名完成后应对这个确切文件验收。

本机的 `%LOCALAPPDATA%\AgenTeam-Release\updater.key` 是加密私钥，`updater.password.dpapi` 仅供当前 Windows 用户解锁。维护者须将加密私钥和可在另一台受信机器恢复的口令分别离线保管。Windows 用户配置文件中的 DPAPI 副本只承担本机解锁。另一台工作站可通过 `TAURI_SIGNING_PRIVATE_KEY_PATH` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 指定自己的恢复副本。

需要在 CI 签名时，由仓库管理员在 `agenteam-signing` 环境设置 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。候选分支的构建任务只有读取权限。版本标签任务单独取得签名凭据并创建草稿。管理员应将环境限制到受信发布引用，并保护可以修改签名工作流或推送版本标签的账号。应用更新和内置运行时资源各有自己的公钥。

## 验收和发布

- 在干净 Windows x64 环境安装 0.6.0，核对程序名、开始菜单、系统卸载项、启动、创建 Agent Team 及 Sidecar。已有 MyAgents 的机器还要检查两套程序分别启动，并各自使用自己的目录。
- 用户首次安装后，数据在 `%USERPROFILE%\.agenteam`，更新缓存位于其 `updates` 子目录。需要沿用旧的命令、技能和模板时，可在首次启动前运行 `node scripts/import-agenteam-profile.mjs`。它仅复制这三类普通文件到新目录，原配置、会话、凭据和待安装更新留在原处。同名目标已存在时会停止。
- 用两版真实安装程序和隔离的 HTTPS 测试频道验证后台下载、退出后恢复、重启安装及用户数据保留。还要测试网络断开后重试、替换缓存字节后拒绝安装、撤回版本后拒绝旧下载。候选版的安装验证和跨版本验证分别记录。
- 本机签名的候选包通过验收后，可用 `gh release create v<版本> <EXE> <SIG> <latest.json> <SHA256SUMS> --repo micsnq2yoma-lgtm/AgenTeam --target <完整提交SHA> --draft --title "AgenTeam <版本>" --notes-file <发布说明文件>` 创建草稿。维护者需核对四个文件、对应提交和验收记录，再通过 GitHub 发布并设为 Latest。使用 CI 签名时，先在受信提交打匹配版本的标签并推送，由工作流创建草稿，选择其中一种草稿创建方式即可。
- 首次安装从发布页下载安装包。后续客户端通过 `/releases/latest/download/latest.json` 检查更新。清单使用 `platforms.windows-x86_64` 和带固定标签的 EXE URL。同一版本的安装包和清单发布后应保持不可变。

## 撤回及其他平台

已有健康版本时，执行 `.\rollback_release.ps1 -HealthyTag v<健康版本>` 将 GitHub Latest 指回该发行版。客户端在安装前重读稳定清单。已经装上问题版本的用户需要更高版本号的修复包。第一个版本缺少历史回退目标时，可撤下有问题的发行版并发布更高版本的修复包。

Ubuntu 暂时手工分发 DEB。macOS 进入稳定频道前须备好 AgenTeam 的 Developer ID、公证凭据，在目标架构验收后，将相应签名归档添加到同一个版本清单。CLIProxy、DSH、Cuse、语音模型等还有各自的外部来源，具体版本与完整性校验见各资源锁。
