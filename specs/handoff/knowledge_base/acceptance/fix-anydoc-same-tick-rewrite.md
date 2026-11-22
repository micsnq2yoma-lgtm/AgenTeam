# AnyDoc 单次快照与 OCR 恢复验收

分支 `fix/anydoc-same-tick-rewrite`。main 分叉点为 `d2ebb8e3d1f4b73242b8d0e51f5250dde66d444a`，验收候选为 `a524cbefbe05133dc63781b5cfa2166e99634d2c`。快照实现及错误提示在 `ada50a92`、`04cb57f1`，OCR 恢复与回归在 `8210f8b3`、`4dd586af`。只读测试工作流在 `a524cbef`。

**独立验收结论：通过。** 四个 CI 作业全部成功。Windows 共享锁、OCR 恢复、调用方测试、Clippy 与大文件单次读取量均已复核。[完整 CI 运行 44914764411](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/runs/44914764411)

## 契约和基线

在 main 分叉点，用完整应用 crate 的同一个测试程序连续五次运行 `document_processing::`。四轮为 31/32，另一轮为 32/32。四次失败均为 `source_copy_rejects_same_inode_same_size_mutation_before_admission`。其期望与首次读取正文前能观测到的信息冲突。

用户批准将首次后台复制得到的私有快照作为此次转换的输入。接收时只持有原件句柄。复制同时计算 SHA-256，随后释放原件句柄。Windows 在复制期间用 `FILE_SHARE_READ` 排斥写入，打开共享冲突映射为可重试的 `DOCUMENT_SOURCE_IN_USE`。Unix/macOS 在读取前后核对设备、inode、长度和时间戳。若同刻度的等长改写在复制期间发生且所有可观测元数据相同，Unix/macOS 仍可能无法检测到。

## 逐项验收

| 项目 | 结果与证据 |
| --- | --- |
| 首次复制前的改写 | `source_copy_uses_the_version_present_at_the_first_read` 打开原件后等长改写并回写修改时间，断言快照及 SHA-256 对应新内容。 |
| Windows 共享锁 | `source_copy_refuses_an_existing_writer_and_blocks_writes_until_finished` 在真实 Windows 上分别验证独占打开、已有写者、复制期间的新写者，以及复制结束后写入恢复。 |
| Unix 复制期间一致性 | `source_copy_rejects_same_inode_mutation_during_copy` 涵盖读取中途修改，`source_copy_checks_metadata_after_the_last_data_read` 检验 EOF 后提交前的版本检查。 |
| 快照之后编辑原件 | `source_copy_is_independent_of_later_edits` 对照已写入原件和私有文件，检查快照字节与摘要。 |
| OCR 让出与恢复 | `live_preemption_requeues_same_job_with_first_snapshot_and_without_partial_staging` 使用生产句柄打开原件，复制后移走原文件并写入替代正文，触发让出并从 FIFO 取出下一代 Worker 的任务。它检查原 job ID、首次摘要、原快照字节、部分输出的清理以及终态清理。 |
| 调用方 | 独立复跑 TypeScript 类型检查、lint、测试分类，以及覆盖 AnyDoc CLI、Admin API 与 Session 附件的六个完整测试文件。在与 lockfile 一致的 Linux 环境下 351/351 通过。Windows 本机初次整组运行遇到 symlink 权限及无关 Record 用例超时，原始记录保留。K2 在依赖本分支的交付分支另行复验。 |

## 可复现的命令

- `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib document_processing::`。本地 Linux 完整 crate 最终代码为 34 passed、0 failed、0 ignored。
- Windows 在 Cargo JSON 标出的最终应用测试程序中嵌入该应用的 manifest，随后执行 `document_processing:: --format pretty --color never`。门禁同时核对共享锁及 OCR 抢占两条测试各自的 `... ok`。
- Rust 1.92.0 下在同一源码路径分别检出基线和候选，运行 `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --message-format=json`，检查构建成功，按 lint、相对文件名和消息去重求差集。

原生 Windows 完整 AnyDoc 为 **33 passed、0 failed、0 ignored**。共享锁和 OCR 恢复均在测试程序中按名称打印 `ok`。同一程序的 workspace IO 为 171/171。[Windows 作业及日志](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/runs/44914764411/job/136877881276)

CI 的 Linux 完整应用库为 1,594 passed、0 failed、8 项既有分池测试。固定工具链为 Rust 1.92.0 / Clippy 0.1.92。在同一次运行中，`d2ebb8e3` 基线与 `a524cbef` 候选均完成全目标编译，去重告警 **145 / 145，新增 0**。该分支的测试工作流使用 `contents: read`，没有引用签名证书、更新签名私钥或发布 token。[Linux 测试及 Clippy 差集](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/runs/44914764411/job/136877881125)

独立验收者从同次运行的 Clippy 原始产物复核了两个完整编译的应用目标。两边各输出 376 条原始告警，按目标和源片段细分为 197 个签名，逐项出现次数也一致。[原始 Clippy 产物](https://github.com/micsnq2yoma-lgtm/AgenTeam/actions/runs/44914764411/artifacts/14711202387)

在 Ubuntu 上另用正式 Worker、PDFium 和 OCR 模型实跑抢占。第一代 Worker 取得 DocumentOcr 租约后，实际请求 RecordLive 并改写原路径，使其内容从扫描 PDF 变为文字型 PDF。第二代 Worker 识别了扫描件的原文，输出包含 `SCAN ORCHARD`，首次 SHA-256 保持一致，OCR 页数为 1。独立复跑也通过。已提交的队列回归另覆盖移走原文件并在同名位置创建新文件。

## 近 512 MiB 的真实转换

同一份合法单页 PDF 共 524,288,861 字节，含 500 MiB 未压缩附件。文件 SHA-256 为 `3af1b74049132ca5757594c2d27c9e603d9775239be60a36b92f29b2f8a3b499`。在相同 Cargo 测试构建配置下调用完整 Manager 与官方 Ubuntu 发行 Worker，各版本预热一次后交替测量三轮。两边均输出正确正文、1 页原生文本和 0 页 OCR。原件读取量由另一次按输入文件身份单独计数的运行取得。

| 指标 | 基线 d2ebb8e3 | 修复版 |
| --- | ---: | ---: |
| 三轮端到端耗时（秒） | 14.168 / 14.072 / 12.314 | 13.995 / 14.305 / 13.670 |
| 中位耗时 | 14.072 秒 | 13.995 秒 |
| 20 ms 采样的进程树峰值 RSS，三轮中位数 | 1067.75 MiB | 1048.75 MiB |
| 用户原件读取量 | 524,288,861 B / 4,001 次读取 | 524,288,861 B / 4,001 次读取 |
| 整棵进程树逻辑读 / 写，中位数 | 1,111,650,546 B / 524,295,260 B | 1,111,650,546 B / 524,295,260 B |

两边都恰好读了一遍用户文件。Worker 对私有快照的读取计入进程树逻辑 I/O。三个计时样本用于说明这次测量的运行成本。基准汇总原始 JSON 的 SHA-256 为 `29c2656dde76076bd77fedbe291bbdd69ba0a19aa250e2febde177d0dc727ccf`。
