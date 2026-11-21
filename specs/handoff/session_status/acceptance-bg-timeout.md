# 后台保活超时修复验收

- 日期：2026-11-21
- 位置：本地 `fix/bg-completion-human-wait`，`specs/handoff/session_status/acceptance-bg-timeout.md`
- 基线：`109db3fe`。原修复：`f4eaffea`。临界行为修复：`8b5cff27`。后续清理：`d0daecbf`。
- 结论：后台行为复验通过。最终清理提交的 Rust 与 Clippy 尚待复验，暂不判定整个分支通过。

## 独立验证

| 项目 | 结果 |
| --- | --- |
| Rust 后台模块 | 独立复跑完整应用库的 `sidecar::background`，`f4eaffea` 8 项通过，`8b5cff27` 9 项通过，无失败、无忽略。 |
| 原修复的新增测试 | `human_wait_keeps_background_completion_alive` 和 `human_wait_does_not_consume_the_safety_budget` 在两个提交上都实际执行通过。 |
| 临界点新增测试 | `entering_human_wait_at_the_budget_boundary_preserves_time_to_resume` 在 `8b5cff27` 上实际执行通过。 |
| Sidecar HTTP 元数据 | `session-read.unit.test.ts` 10 项通过，包含 `reports a human wait so background completion pauses its safety budget`。 |
| Clippy | 父提交与 `f4eaffea` 均完成检查。215 条去重诊断对 215 条，新增 0，`sidecar/background.rs` 告警 0。`8b5cff27` 的行为补齐引入遗留释放函数的未使用告警，已在 `d0daecbf` 清理。复验时 Docker Desktop 的 Linux API 返回 HTTP 500，最终 Clippy 结果未取得。 |

- Rust 命令是 `cargo test --manifest-path src-tauri/Cargo.toml --lib sidecar::background` 和 `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --message-format=json`。两次 Clippy 使用相同离线 Linux 构建环境、Rust 1.92.0 和 Clippy 0.1.92。
- Clippy 按相对路径、诊断代码、消息及主跨度源文本去重，并检查 `build-finished.success`。两侧重新编译相同的 `build.rs`，消除缓存复用时构建脚本诊断显示不一致的问题。
- TypeScript 命令是 `vitest run src/server/routes/session-read.unit.test.ts`。

## 关闭 Tab 后批准

用 Tauri 测试应用调用实际的 Rust `start_background_completion` 和 `cmd_release_tab_session`。隔离的 loopback Sidecar 测试服务模拟运行、等待权限确认、收到批准后继续、正常结束。一次性副本把预算降到 3 秒，保持产品的 2 秒轮询间隔；等待 7 秒后验证 Tab owner 已移除、BackgroundCompletion owner 和子进程还在，发送批准并确认正常结束时释放后台 owner。仓库中的生产预算仍为 3600 秒。

- `detached_tab_can_still_approve_after_the_background_budget`：`f4eaffea` 通过。关闭 Tab 时已在等批准。
- `detached_tab_retains_approval_when_wait_starts_at_the_budget_boundary`：`f4eaffea` 复现失败。关 Tab 后第一轮仍在工作，随后进入待批准，旧轮询先按上一采样计算超时并丢失后台 owner。同一探针在 `8b5cff27` 通过，真实 Rust 保活和 Tab owner 释放流程保留了进程，收到批准后继续并正常收尾。

这项集成验证覆盖实际 Rust 保活、Tab owner 释放和 HTTP 协议，模型交互由测试服务模拟。

## 审查

`/api/session-state` 从 SessionEngine 当前状态投影 `waitingForUser`。旧 Sidecar 缺少字段时按 `false` 解析。补齐提交在判定超时前读取并验证当前进程代次的快照，人工等待两侧的交界采样区间保守地排除在活跃预算外。超时、完成和不可达分支共用代次校验后的 owner 释放路径。活跃超时仍覆盖持续运行且没有人工等待的任务。
