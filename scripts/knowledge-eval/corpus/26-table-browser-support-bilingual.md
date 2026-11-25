# 桌面端与浏览器兼容性矩阵

## Supported Platforms 支持环境列表

| Platform 平台 | Minimum OS 最低系统版本 | WebView / Engine 渲染引擎 | Architecture 架构支持 |
| --- | --- | --- | --- |
| Windows Desktop | Windows 10 21H2 | Edge WebView2 Evergreen | x64, ARM64 |
| macOS Desktop | macOS 13 Ventura | WKWebView | Apple Silicon, Intel |
| Linux Desktop | Ubuntu 22.04 LTS | WebKitGTK 2.42+ | x64, ARM64 |
| Web Console | Latest 2 versions | Chrome, Edge, Safari, Firefox | N/A |

## Deprecation Policy 弃用策略

当某一操作系统版本终止官方安全更新后，客户端将在随后两个次版本中弹出升级横幅提示，并在六个月缓冲期结束后停止为其提供安装包构建与技术支持。
