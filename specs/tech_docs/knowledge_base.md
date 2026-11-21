# 知识库契约

K0 定义共享 wire、ID、引用和后续模块的边界。TypeScript 位于 `src/shared/knowledge/`，Rust 位于 `src-tauri/src/knowledge/types.rs`。两端共用 `src/shared/knowledge/__fixtures__/refs.json`。下文标注的实现阶段对应任务拆分，功能随各阶段接入。

## Owner 与进程边界

| 事实 | Owner | 写入与裁决 |
|---|---|---|
| 知识库清单、设置、来源记录、规范化 Markdown、来源元数据 | Rust `KnowledgeStore` | App 级单一写入权威。锁内读取最新状态并原子提交，提交成功后广播。字段已有人工值时，锁内拒绝 Agent 对该字段的写入。 |
| 来源内容状态 | Rust `KnowledgeStore` | `queued → converting → chunking → ready`，另有 `failed / cancelled / interrupted`。K2 原子发布内容后等待切块；K3 完成切块后提交 `ready`。 |
| 摄取队列、取消、重试 | Rust `KnowledgeIngestManager` | 作业归知识库域管理。Office、PDF、图片复用 `DocumentProcessingManager` 的转换。通过 Store 提交内容和状态。 |
| 可检索状态与派生索引 | Rust `SearchEngine` 下的 `KnowledgeIndex` | K4 提供按来源的 `pending / indexed / failed` 查询。全局一个 Tantivy 索引和一个 writer，文档以 `kb_id` 过滤，跨库 BM25 分数可比较。订阅 Store 广播增量更新；索引可重建，没有 Store 写权限。UI/CLI 将查询结果与来源内容状态组合展示。 |
| Agent 默认绑定 | `AgentConfig.knowledgeBaseIds` | `config.json` 持久化，复用 `agent-config-intent.lock`。 |
| Session 读取权限 | Session 配置快照 | 出生时固化。Rust 通过 Sidecar process generation 验证调用身份，再按快照裁决。 |
| 元数据和 wiki 写权限 | `KnowledgeStore` | 默认 `agentWriteAccess = curator-only`，仅允许该库整理 Task 当前执行的 Session 写入。用户可改为 `bound-sessions`。 |
| 整理推理 | 既有 Task 系统与 Product Session | 每个库配置 `curatorAgentId`。未配置时，模式只能是手动或关闭。配置后按库的整理设置执行，元数据和 wiki 默认每三天自动整理。 |
| Wiki 页面与索引登记 | `KnowledgeStore` | 整理 Task 通过 source 接口提交，wiki 页以来源身份进入索引。 |
| 物理整理方案、日志与恢复 | Rust `KnowledgeOrganizeManager`（P1） | Agent 可提议。审批、执行和撤销只由用户经 Tauri 命令发起。文件变更复用 `workspace_files` 的路径校验和严格 mutation helper。 |
| 嵌入与精排模型包 | Rust `KnowledgeModelPackManager`（P1） | 复用语音模型包的签名清单与激活指针。Document Worker 扩展 `embed` 作业，重算力租约由 `LocalComputeCoordinator` 管理。云端嵌入默认关闭。 |

Renderer 管理面经 Tauri 调 Rust。Agent 使用官方 `myagents kb` 命令，经 Session Sidecar Admin API 转发到 Rust Management API。Session 和工作区身份取自已验证的进程 generation，调用方提交的库 ID 只是请求范围。正常会话仅在其授权库中检索。资料内容按不可信输入处理，允许从中读取事实和引用，执行权限由 Rust 独立裁决。

## ID 与引用

| 类型 | 规范形式 |
|---|---|
| `KnowledgeBaseId` | `kb_` + 12 位小写十六进制 |
| `KnowledgeSourceId` | `src_` + 16 位小写十六进制 |
| `KnowledgeJobId` | `job_YYYYMMDD_` + 12 位小写十六进制 |
| `KnowledgePlanId` | `plan_YYYYMMDD_` + 12 位小写十六进制 |
| `KnowledgeChunkId` | 16 位小写十六进制，无前缀，由来源 ID、标题路径、独有正文哈希和同组出现序号派生 |

日期是四位年份的有效公历日期，范围 `00010101` 至 `99991231`。生成函数将调用方提供的 6 或 8 个字节编码成定宽字符串。生产分配应使用安全随机字节，Store 在提交锁内检查碰撞后重新分配。固定计数测试覆盖编码的一一对应关系。

引用采用下列语法，方括号表示可选成分。

```text
kb://<kb-id>/<source-id>[?page=<页码>][#<chunk-id>]
```

`page` 是十进制正整数，范围 1 至 4,294,967,295，十进制写法无前导零。查询在片段之前，最多出现一次。解析按字面比较 ASCII，拒绝 URL 编码、空白、额外路径段、点段、未知参数和非规范 ID。格式化同样检查所有组成部分，非法输入返回 `null`（TypeScript）或 `None`（Rust）。解析成功只说明语法有效，存在性、库授权和原文定位由后续解析入口裁决。

引用依附稳定的 Source ID，文件路径只作为来源记录的属性。移动文件后更新来源路径即可保持原引用有效。

## 元数据与错误码

每个元数据字段为 `{ value, origin, updatedAt }`，其中 `origin` 取 `agent` 或 `user`，`updatedAt` 是 ISO 8601 时间字符串。人工修改后的字段由 Store 保护。批量写入按字段报告结果。

| 错误码 | 含义 |
|---|---|
| `KB_NOT_FOUND` | 授权范围内的请求对象不存在 |
| `KB_ACCESS_DENIED` | 调用方缺少对应知识库或操作的权限 |
| `KB_FIELD_LOCKED` | Agent 尝试修改人工设置过的字段 |
| `KB_LIMIT_EXCEEDED` | 超出容量或数量限制 |
| `KB_CHUNK_UNSPLITTABLE` | 切块遇到超过硬上限的单个扩展字素簇，摄取作业附原文位置 |
| `KB_SOURCE_UNSUPPORTED` | 来源类型或格式暂不支持 |
| `KB_SOURCE_IN_USE` | 来源文件被占用；提示键为 `closeFileAndRetry` |
| `KB_INDEXING` | 索引基线尚未可用 |
| `KB_CATEGORY_NOT_ALLOWED` | 分类尚未纳入用户批准的分类体系 |
| `KB_INVALID_INPUT` | ID、引用、设置或来源事实无效 |
| `KB_STORE_CORRUPT` | 已持久化的 JSON、身份或目录结构无效 |
| `KB_STORE_UNAVAILABLE` | 文件锁、读取或持久化失败，提交状态可能未确认 |
| `KB_INSUFFICIENT_SPACE` | 托管副本或转换产物所在卷可用空间不足 |
| `KB_JOB_CONFLICT` | 尝试已被取消、重试或结算，当前操作不能再提交 |

错误和变更通知只携带调用方处理结果所需的字段。资料正文、凭据和路径之外的隐私数据不得进入错误、日志或通知事件。

## 存储布局

`<app-data>` 由 Rust `app_dirs::myagents_data_dir()` 提供。当前发行版默认使用 `~/.agenteam/`。所有子路径均相对于这个目录。

```text
<app-data>/
  knowledge/
    kbs.json                     # 已发布库 ID 列表
    <kb-id>/
      kb.json
      ORGANIZE_RULES.md
      WIKI_SCHEMA.md
      sources/<source-id>/
        source.json
        original.<ext>
        content.md
        assets/
      wiki/
        index.md
        log.md
        topics/
      organize/
        plans/<plan-id>/
          plan.json
          journal.jsonl
        feedback.jsonl
  search_index/knowledge/
    .schema_version
    bm25/
    vectors/<model-revision>/
    chunks.manifest
```

托管来源保存 App 持有的原件副本。P1 接入的 linked folder 保持用户原件在原位置，清理 App 数据只处理托管内容。除人工逐项批准的移动、重命名或建目录外，用户原文件始终只读。物理整理默认排除 Git 工作树。用户可对特定目录单独授权，执行仍需要逐项批准。整理禁止跨卷移动和覆盖。

## 限制

| 项目 | 约定 | 阶段 |
|---|---|---|
| 引用长度 | 最多 74 个 ASCII 字节 | K0 |
| Store 单份 JSON | 最多 2 MiB | K1 |
| 明文单文件 | 8 MiB | K2 |
| 摄取队列 | 最多 64 个作业 | K2 |
| 单库来源 | P0 最多 10,000 个 | K2 |
| 切块 | 目标约 600 token，上限 1,000，相邻重叠约 80 | K3 |
| 检索 | 每路每条查询召回 50 条，默认返回 8 条，每个来源最多 3 条 | K4 |
| Wiki | 每页最多 1 MiB，每库最多 2,000 页 | K8 |
| 物理整理 | 每份方案最多 500 项，待审方案 7 天过期 | K13 |

## K1 存储事务和变更广播

应用启动时把 `Arc<KnowledgeStore>` 注册为 Tauri managed state。命令通过 `tauri::async_runtime::spawn_blocking` 执行文件操作。Store 以 `app_dirs::myagents_data_dir()` 作为受信任的数据根，在 `knowledge/.store.lock` 上复用 `with_file_lock_blocking`。多个 Store 实例重新读取同一份磁盘状态，写入以锁内最新状态为准。

Windows 上，前一写入者释放锁目录时，系统可能尚未关闭最后一个句柄。锁 helper 识别原生 `STATUS_DELETE_PENDING`，在同一个 5 秒获取期限内等待目录释放。目录 ACL 拒绝创建时立即返回 I/O 错误。

### 磁盘格式与发布

- `kbs.json` 是已发布库 ID 的列表，只有首次读取文件不存在时按空列表处理。已登记的库必须具有同 ID 的 `kb.json`。读到截断 JSON、重复 ID 或身份不符时，返回可识别的错误并保留原字节。
- `kb.json` 保存名称、描述、整理 Agent、整理模式、周期、Agent 写权限、wiki 开关和待审分类。存在整理 Agent 的新库默认自动整理，否则默认为手动。默认周期为 4,320 分钟，写权限为 `curator-only`，wiki 默认开启，待审分类列表为空。更新省略的字段沿用旧值，`curatorAgentId: null` 显式清空。模式和 Agent 在同一次提交中校验。
- `source.json` 独立保存来源事实和八个元数据字段。来源事实可包含展示用的 `currentPath`、文件指纹和小写 SHA-256 摘要。更新来源事实保留元数据。K1 登记来源与引用身份，摄取状态流转交给 K2。
- JSON 文件使用 `workspace_files` 的 no-follow 写入，先同步临时文件，再原子发布，随后复用 `durable_fs` 同步父目录。建库时先同步 `kb.json`，随后将库 ID 写入 `kbs.json`。两步之间崩溃会在磁盘留下未登记的目录。Store 保留该目录供恢复，因为仅凭目录名无法确认哪些数据可以删除。列库和按 ID 查询均以登记表为准；后续建库分配 ID 时也会避开磁盘上已有的目录。来源使用有明确归属的隐藏暂存目录，在原子发布前中断的暂存目录可在列出来源时清理。
- 删除先将托管目录重命名为同父目录下的隐藏待清理目录。库删除以 `kbs.json` 提交为准：重启后，仍登记的目录恢复，已移除登记的目录接着清理。来源删除以同父目录重命名提交为准。清理仅使用已校验的托管路径，来源记录里的原件路径仅作描述。目录同步失败可表示提交状态未确认，调用方先重新读取 Store 再决定是否重试。

### 命令、字段锁与订阅

- 管理命令为 `cmd_kb_list`、`cmd_kb_create`、`cmd_kb_update`、`cmd_kb_delete`、`cmd_kb_list_sources`、`cmd_kb_get_source`、`cmd_kb_remove_source`、`cmd_kb_set_metadata` 和 `cmd_kb_resolve_ref`。
- `cmd_kb_set_metadata` 固定按人工写入。Rust 内部调用传入 actor，由 Store 在同一文件锁内裁决。已由人工写过的字段返回 `KB_FIELD_LOCKED`，仍可提交同批次中的其他字段。回复按字段返回 `updated` 或 `rejected`。写入空值仍保留写入方标记。K8 补充分类和字段内容规则。
- 引用解析验证格式、库与来源存在性，并保留页码及 chunk ID。具体页定位与 chunk 查询由后续模块完成。
- 同步提交后才发送 `SourceUpserted`、`SourceRemoved`、`KbRemoved` 或 `MetadataChanged`。广播是 Rust 进程内的索引通知，内容限于 ID 和可选 SHA-256 摘要。索引消费者掉队时从 Store 重新读取基线。

Windows CI 用应用 crate 执行知识库测试。目录重定向用例在 Windows 创建 NTFS junction，在 Unix 创建符号链接。测试覆盖被替换的库和来源目录，以及删除库时对库外原件的保护。

## 摄取与中断恢复（K2）

`cmd_kb_import_files(kbId, paths[])` 按输入顺序返回逐文件结果。接受的文件返回来源 ID、任务及重复标记；拒绝的文件返回稳定错误码和恢复提示键。通过 `cmd_kb_job_status(jobId)` 查询，`cmd_kb_cancel_job(jobId)` 取消，`cmd_kb_retry_source(kbId, sourceId)` 从托管原件开始一次新尝试。转换执行在阻塞工作线程上，Tauri 事件循环只负责调度。

- Store 验证普通文件、输入类型和大小，检查可用空间后复制至托管的 `original.<ext>`。原始字节的 SHA-256 用于同库去重。来源记录保留文件大小、修改时间，以及发布后 `content.md` 的 SHA-256，供派生消费者识别内容版本。队列最多容纳 64 个排队或转换中的任务，每库最多 10,000 个来源。
- UTF-8、带 BOM 的 UTF-16LE/BE 文本通过有界读取转换，文本输入上限 8 MiB。Office、PDF 和图片交给现有 `DocumentProcessingManager`。知识库任务保存其内部转换 ID 以便取消；转换错误映射为稳定的恢复提示键。
- Windows 打开来源或从已持有的句柄复制快照时，共享冲突返回 `KB_SOURCE_IN_USE` 和 `closeFileAndRetry`。接收前被拒绝的文件在占用解除后重新导入。已接收的任务遇到同一错误时保存失败原因，可从托管原件重试。
- 新任务在 `queued` 等待。Store 在提交 `converting` 后提供本次尝试的原件和暂存目录，输出准备完成后，按当前任务 ID 原子发布 Markdown、资产和来源记录。K2 的成功终点为 `chunking`。失败或取消保留托管原件供重试，旧任务回调不能修改新尝试，人工元数据跨转换保留。
- 当前转换尝试和状态随 `source.json` 同一次提交。库内 `jobs/<job-id>.json` 定位任务，`original-hashes/` 用于去重，知识库根的 `active-imports.json` 记录待恢复的任务。应用关闭时停止接收导入，并收敛转换。异常退出后重开，未完成的排队或转换任务记为 `interrupted`，已发布并等待切块的来源保留 `chunking`。

## 切块与引用定位（K3）

`chunk_markdown` 按 Markdown 结构划分独有正文，返回顺序、标题路径、页码、显示前缀、片段文本、原文 UTF-8 字节范围和 UTF-16 范围。两个范围都从 0 起算，末端不包含在范围内。普通块以约 600 token 为目标，硬上限 1000 token，相邻块最多重叠约 80 token。`## Page N` 给后续内容标记 PDF 页码。

### 稳定引用

`chunk_id = hex(sha256(source_id ‖ heading_path ‖ sha256(独有正文) ‖ 出现序号))[..16]`。先将 CRLF 转成 LF，再移除每行行尾空白。标题路径带长度编码，避免不同的路径组合产生相同的输入字节。出现序号从 0 开始，仅在相同标题路径和正文哈希内递增。全局 ordinal 用于排序和相邻查询，页码用于打开原文，来源 content_hash 用于核对版本。三者及所有重复前缀都排除在 ID 之外。其他段落的插入、重新分页或界面语言变化不会改变既有片段的 ID。

`cmd_kb_resolve_ref` 对片段引用同时返回来源和片段状态。当前版本仍含该 ID 时可取得范围和正文。来源仍在但旧 ID 已消失时，返回来源及 `fragment.status = unavailable`，供 K10 显示“片段已更新”并打开来源。

### 宽表、长单元格与预算

| 情况 | 处理 |
| --- | --- |
| 表头最多占预算的 25% | 每个续块重复所有列；超过 48 token 的列名在续块中使用同一个显示函数缩写，完整原名保留在起始表头。 |
| 表头超过 250 token | 完整表头放在表起始处。超过 1000 token 时沿单元格边界放入连续首块，必要时在超长单元格内续分。续块按原列顺序提取约 250 token 的显示列名，标明总列数、已显示列数和首块引用。 |
| 显示列名 | 表头摘录和单元格定位共用显示函数。列名最多 48 token 时完整显示。超过时沿扩展字素簇边界取开头约 32 token，随后显示省略号、列序号与完整列名首块的 `kb://` 引用。列目录记录 `columnIndex`、原始字节范围和该列名的首个表头块 ID。完整长列名可跨多个连续块。 |
| 单行超过上限 | 先沿单元格边界分段，附所在行的段号。单元格连同必要标记仍放不下时，只续分该单元格，依次优先换行、句末、词边界和扩展字素簇。每段标记行号、显示列名和第 k/n 段。 |
| 续分块的正文容量 | 表头摘录与定位标记合计最多 400 token。先尝试完整摘录，再收缩为本列名，最后仅保留定位标记。至少 600 token 可用于正文，按可用容量和结构断点填充，末段保留实际剩余正文。所有段的独有正文按序拼接等于原单元格。 |

摘录和标记模板取自中英 i18n 资源。正文分界依据两个受支持语言的固定预算计算，因此缩短摘录或切换语言仍能解析同一条引用。

### 不可拆字素与失败位置

预算以十分之一 token 为单位。CJK 字符、组合附标和其他非空白符号各计 10 单位，连续的非 CJK 字母、数字或下划线词串计 13 单位，空白计 0。扩展字素簇的估算等于该簇覆盖的费用总和。例如一个 `a` 与后随的 1000 个 U+0301 形成单个字素簇，费用为 1001.3 token。超过硬上限时，切块器返回第一个此类字素的位置，覆盖正文、表头和 Markdown 标题。

Store 保留托管原件及已发布的 `content.md`，在该作业上持久化 `failed`、`KB_CHUNK_UNSPLITTABLE` 和 `position`。其中 `row` 和 `column` 分别为从 1 起算的原文物理行和 Unicode 标量列，`byteOffset` 是从 0 起算的 UTF-8 字节偏移。调用方可据此定位问题并修正输入。同批其他作业继续处理。

### 摄取与恢复

摄取管理器在既有后台队列中处理新发布及重启时已处于 `chunking` 的来源。Store 校验已发布 Markdown 的摘要，在锁外切块，并在提交前重新核对当前尝试、内容哈希和来源状态。切块成功后由 Store 提交 `ready` 并广播。来源的可检索状态由 K4 的索引单独维护。

新导入来源只保存所选文件的文件名作为显示标题。旧来源没有此字段时使用已有路径的文件名，或以来源 ID 显示。启动恢复按持久化来源发现遗留的 `chunking` 任务。唯一归属可从当前来源记录确定时，Store 补建缺失的当前任务定位文件。已有冲突记录会返回存储损坏错误。

## K4 检索、索引重建与权限过滤

`SearchEngine` 持有全局知识库索引。数据目录为 `search_index/knowledge/bm25/`，schema 版本独立记录于 `search_index/knowledge/.schema_version`。每份资料的摘要和各个正文片段分别建文档。索引使用与 Session、Record 共用的中文分词器，标签同时保留可分词文本和完整过滤值。

Store 先提供提交广播的订阅，再由后台读取基线。后台一次读取一份来源，通过 Store 核对来源状态、正文摘要和元数据快照。Store 的 `ready` 表示内容可用，索引状态单独显示 `pending`、`indexed` 或 `failed`。写入及 reader reload 成功后才能报告 `indexed`。元数据变化重新索引，库或来源删除按精确身份移除。积压超过广播容量时从 Store 重建。

库、标签、分类、类型的精确过滤应用于摘要候选和正文候选。空的库范围返回空结果。每条查询在同一个全局 reader 中返回最多 50 个正文候选，其中摘要能提供相关来源候选。RRF 用 `k=60` 融合，按同库同源限 3 条。结果默认 8 条，可取 1 至 20 条，返回 `mode: "bm25"` 和规范片段引用。会话授权在 K5 的调用入口裁决。

派生目录版本变化和确认的 Tantivy 数据损坏由索引持有者恢复。确认无活动 writer 后，仅清理 BM25 子目录，再从 Store 重建。重建期间查询报告 `KB_INDEXING`。其他索引与资料原件、来源记录均保留。

## 待接入章节

### K5–K7 API、CLI、Session 绑定与 Skill

### K8 元数据与 wiki 整理

### K9–K11 UI、引用与检索评测

### K12–K15 Linked folder、物理整理与恢复

### K16–K20 模型包、语义检索和其他来源
