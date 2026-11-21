# 知识库契约

K0 定义共享 wire、ID、引用和后续模块的边界。TypeScript 位于 `src/shared/knowledge/`，Rust 位于 `src-tauri/src/knowledge/types.rs`。两端共用 `src/shared/knowledge/__fixtures__/refs.json`。下文标注的实现阶段对应任务拆分，功能随各阶段接入。

## Owner 与进程边界

| 事实 | Owner | 写入与裁决 |
|---|---|---|
| 知识库清单、设置、来源记录、规范化 Markdown、来源元数据 | Rust `KnowledgeStore` | App 级单一写入权威。锁内读取最新状态并原子提交，提交成功后广播。字段已有人工值时，锁内拒绝 Agent 对该字段的写入。 |
| 摄取队列、取消、重试 | Rust `KnowledgeIngestManager` | 作业归知识库域管理。Office、PDF、图片复用 `DocumentProcessingManager` 的转换。 |
| 派生索引 | Rust `SearchEngine` 下的 `KnowledgeIndex` | 全局使用一个 Tantivy 索引和一个 writer，文档以 `kb_id` 过滤。同一查询下跨库 BM25 分数可比较。按提交后的广播增量更新，可从 Store 重建。 |
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
| `KnowledgeChunkId` | 16 位小写十六进制，无前缀。K3 定义内容派生算法 |

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
| `KB_SOURCE_UNSUPPORTED` | 来源类型或格式暂不支持 |
| `KB_INDEXING` | 索引基线尚未可用 |
| `KB_CATEGORY_NOT_ALLOWED` | 分类尚未纳入用户批准的分类体系 |

错误和变更通知只携带调用方处理结果所需的字段。资料正文、凭据和路径之外的隐私数据不得进入错误、日志或通知事件。

## 存储布局

`<app-data>` 由 Rust `app_dirs::myagents_data_dir()` 提供。当前发行版默认使用 `~/.agenteam/`。所有子路径均相对于这个目录。

```text
<app-data>/
  knowledge/
    kbs.json
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
| 明文单文件 | 8 MiB | K2 |
| 摄取队列 | 最多 64 个作业 | K2 |
| 单库来源 | P0 最多 10,000 个 | K2 |
| 切块 | 目标约 600 token，上限 1,000，相邻重叠约 80 | K3 |
| 检索 | 每路每条查询召回 50 条，默认返回 8 条，每个来源最多 3 条 | K4 |
| Wiki | 每页最多 1 MiB，每库最多 2,000 页 | K8 |
| 物理整理 | 每份方案最多 500 项，待审方案 7 天过期 | K13 |

## 待接入章节

### K1 存储事务和变更广播

### K2–K3 摄取、切块和引用定位

### K4 检索、索引重建与权限过滤

### K5–K7 API、CLI、Session 绑定与 Skill

### K8 元数据与 wiki 整理

### K9–K11 UI、引用与检索评测

### K12–K15 Linked folder、物理整理与恢复

### K16–K20 模型包、语义检索和其他来源
