# AgenTeam Windows 构建

Windows 10/11 x64 上可以使用三种开发入口。

| 目的 | 仓库根目录命令 | 结果 |
| --- | --- | --- |
| 快速调整界面 | `preview_windows.cmd` | 浏览器本地预览，支持热更新 |
| 调试原生桌面能力 | `.\setup_windows.ps1`，随后 `npm run tauri:dev` | 启动开发版 WebView 和原生壳 |
| 验收实际安装体验 | `.\build_windows.ps1 -Candidate -SkipPortable` | 在 `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/` 生成 Windows x64 NSIS 安装包 |

原生构建需要 Visual Studio 的 C++ 桌面开发工具和 Windows SDK、Rustup、Node.js 及 CMake。脚本会根据 `rust-toolchain.toml` 和 `scripts/node-runtime.json` 对齐编译工具链。资源下载、缓存与校验由相应准备脚本处理。需要清洁构建时，使用 [GitHub Actions 工作流](../../.github/workflows/release.yml)。

候选包签名、正式发布、回退和首装导入见 [AgenTeam 桌面发布](build_and_release_guide.md)。原生更新调试见 [自动更新架构](../tech_docs/auto_update.md)。
