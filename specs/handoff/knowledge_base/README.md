# 知识库交接包

> 交接日期：2026-11-21。交接方：Claude（原验收者）。接收方：Codex 协作者（从本日起负责**开发和验收**）。
> 本目录是交接用的提案文档，不是现行规范。现行规范在 `specs/tech_docs/knowledge_base.md`，随任务逐步补全。

## 文件

| 文件 | 内容 |
|---|---|
| `prd.md` | 设计方案（PRD / RFC）。§13 是已确认的决策和实现细化 |
| `tasks.md` | 任务拆分（K0–K20）、全局完成定义 D1–D9、红线、每个任务的验收项 |
| `k1_windows_probe_tests.rs` | K1 验收时写的对抗性探针测试，K1 修复项要用到（见下文） |

两份文档原本在 `specs/prd/`（被 gitignore），这里是原样副本。`tasks.md` 与 `prd.md` 冲突时以 `tasks.md` 为准；它们与代码现状冲突时，先用代码、测试和 git 历史确认现状，再修正文档。

## 角色变化（历史，2026-11-21；现行分工见上文和 `../README.md`）

`tasks.md` §0.6 原本规定 Codex 执行、Claude 验收。从现在起两件事都由 Codex 负责。为了保留独立验收的效果，按下面的方式做：

1. **开发和验收分开进行**。开发完成并提交后，用一个新的会话（或至少清空上下文）作为验收者，只依据 `tasks.md`、代码 diff 和测试结果判断，不复用开发时的推理。
2. 验收者按 `tasks.md` §0.6 的五步执行：静态审查 → 复跑 D1–D4 → 对标了 🔒 的项写对抗探针 → 手动场景（涉及 UI 时）→ 给出结论。
3. 每个任务的验收结论写入 `acceptance/K<n>.md`，放在本目录下，随任务分支提交。内容包括：逐条 AC 的结果和对应测试名、复跑命令的输出摘要、探针及结果、结论（通过 / 有条件通过 / 打回）和遗留项。
4. **合并到 `main`、推送 tag、发布仍需用户明确授权**。验收通过只表示可以申请合并。

## 当前进度

| 任务 | 分支 | 状态 |
|---|---|---|
| K0–K4 契约、Store、摄取、切块、索引 | 已合并 | **通过**，见 `acceptance/K1.md`–`K4.md` |
| K5 会话 CLI | 已合并（`6188756`） | **通过**，见 `acceptance/K5.md` |
| K6 绑定与 Session 快照 | 已合并（`6188756`） | **通过**，M1 能检索实机通过，见 `acceptance/K6.md` |
| K7 Skills 与库规则模板 | 已合并（`e24c3bb`） | 自检通过、正式 CI 全绿，用户批准合并，见 `acceptance/K7.md` |
| K8 | 已合并 | 实机通过（见 `delivery/K8.md`），M2 阶段报告见 `acceptance/M2.md` |
| K9 知识库 UI | `feat/kb-k9-entry` | **开发完成待独立验收**：启动页"对话/知识库"开关、库列表（来源数/检索模式/占用空间/整理状态）、详情页 4 标签（资料虚拟文件夹树与拖拽归类/资料抽屉与字段锁解锁及切块预览/检索测试/Wiki 浏览与 lint 及待确认分类/设置与双语规则模板编辑）、Agent 设置知识库区块、对话工具菜单会话开关与 Store 事件推送均已实现并自检，见 `delivery/K9.md` |
| K10、K11 | — | 未开始 |

**2026-11-22 起由 Claude 负责开发与自检**，不再使用协作者；自检不是独立验收，记录里会注明。下文「角色变化」一节和 K0/K1 记录保留为历史。

### K0 验收记录（摘要）

- `tsc --noEmit`、ESLint、dependency-cruiser、agent-docs、测试分类均通过；TS 单测 4/4。
- `types.rs` 在独立 crate 中编译：6/6 测试通过，clippy `-D warnings` 无告警，rustfmt 无差异。
- 对抗探针：生成 2 万条随机变异的 ref，TS 解析结果交给 Rust 逐条比对解析和格式化，**0 处不一致**。
- 未覆盖：完整 Tauri crate 编译（验收机缺 cmake），即 `lib.rs` 中新增的 `pub mod knowledge;`。K1 的完整 crate 测试会一并覆盖。

### K1 必须完成的事项

1. **在完整 crate 中运行测试并贴出输出**：`cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge` 和 `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`。
2. **补上 Windows 的重定向防护测试（必须修复）**。现有的 `knowledge_refuses_external_directory_redirects` 只在 `#[cfg(unix)]` 下运行，而 Windows 是主要平台。把 `k1_windows_probe_tests.rs` 中的用例并入 `src-tauri/src/knowledge/store/tests.rs`（或作为正式测试模块），覆盖：
   - 来源目录被替换为 junction 时，读取和删除都被拒绝，库外文件不受影响；
   - 整个库目录被替换为 junction 时，删除被拒绝；
   - 库内含指向外部的 junction 时，删除整个库不波及外部文件；
   - 建库写完 `kb.json`、尚未登记 `kbs.json` 时崩溃留下的孤儿目录不可见、不影响后续建库；
   - 路径穿越形式的库 ID 在任何 IO 之前被拒绝。
   探针用 `cmd /C mklink /J` 创建 junction，不需要管理员权限。若某项不通过，修代码而不是改测试。
3. **小问题（不阻塞）**：上述孤儿目录目前不会被清理，只占空间。可以在 `read_registry` 或建库时清理未登记的库目录；如果决定不处理，在 tech doc 里说明。

### K1 静态审查结论（供参考）

AC-K1-1 至 AC-K1-8 都有对应测试；设计符合要求：所有读写在同一把跨进程文件锁内、基于磁盘最新状态；删除采用"重命名为待清理目录 + 以 `kbs.json` 提交为准"，两侧都可恢复；JSON 写入复用 `workspace_files` 的 no-follow 写入与 `durable_fs`；ID 先校验格式再做 IO；`cmd_kb_set_metadata` 固定以用户身份写入；提交成功后才广播。未发现违反红线。

## 环境注意事项（来自验收机，Windows）

- **npm 版本**：项目要求 npm ≥ 11.15（`devEngines`），低于此版本 `npm run …` 和 `npx` 都会直接报错。可在项目目录外安装 npm 11.15 再用其 `npm-cli.js` 执行 `ci`。
- **cmake**：完整 Tauri crate 依赖 `libopus-sys`，构建时需要 cmake。
- **Windows 路径长度**：工作目录路径过深时，CMake 的临时编译路径会超过 260 字符上限，`cl.exe` 报 `C1083 … Invalid argument`，MSBuild 也会报找不到 `VCTargetsPath`。把仓库或 `CARGO_TARGET_DIR` 放在较短路径下即可。
- **内存**：完整 crate 首次编译较重，内存紧张时用 `cargo … -j 2`。

## 相关的另一个交付（与知识库无关）

`feat/launcher-context-header`（`103f455d`）：启动页"项目 | 运行环境"上下文抬头（用户选定的 C1 方案，去掉下拉箭头）。TS 类型检查、ESLint、相关 DOM 测试 351/351 均已通过并已提交，**尚未在真实 App 中目视检查**（间距、浅色/深色主题、长项目名截断）。规范见 `specs/design/launcher_and_composer.md` §4.1。验收时启动 App 截图确认后，等待用户授权合并。
