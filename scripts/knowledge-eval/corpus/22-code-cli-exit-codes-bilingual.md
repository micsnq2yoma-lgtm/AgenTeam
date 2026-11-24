# 官方 CLI 退出码与结构化错误规范

## Exit Code Contract 退出码约定

命令行工具在脚本和 Agent 自动化场景下必须返回稳定的进程退出码，便于调用方在不解析人类文案的情况下做出分支决策：

- `0`: 成功执行
- `2`: 命令行参数语法错误或必填标志缺失
- `3`: 权限不足或认证凭据失效
- `4`: 目标资源不存在（Not Found）
- `5`: 瞬态网络或上游服务不可用，可安全重试

## Machine-Readable JSON Error Output

When `--json` is passed, errors are printed to stderr as a single JSON object containing `code`, `message`, and optional `retryable` boolean. No ANSI color escape sequences may be emitted when stdout is not a TTY.
