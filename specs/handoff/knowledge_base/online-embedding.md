# K21 在线嵌入模型（设计与任务）

状态：**设计已确认，待知识库协作者实现。** 本文取代 `tasks.md` 中 K16（嵌入模型包）与 K17（Document Worker `embed` 作业）的本地推理方案，并把 K18 真正接入检索与导入链路。语音转写（Record）**不在本文范围，保持现状**。

## 1. 决策记录（2026-11-25，用户确认）

1. 嵌入改用**在线模型**，在「设置 → 模型设置」里配置；本地模型包路线（K16、K17）退役，不保留两条路径。
2. 语音模型暂不改动。
3. 知识库文本块发送给在线嵌入服务，**用户已接受**；不要求每个知识库单独开关。仍保留一个全局总开关（关闭即只用 BM25）。
4. 设计文档由本任务的作者提供，实现与验收按 `HANDOFF.md` 的分工由知识库协作者负责。

## 2. 起因：为什么本地方案走不通

K16 沿用语音模型包的分发与验签机制：清单在 `download.myagents.io/models/knowledge/sets/<版本>/manifest.json`，必须附 `.minisig`，并由编译进程序的 `src-tauri/upstream-resources.pub` 验签；离线本地包安装同样验签。仓库里没有知识库模型包的打包、签名、上传脚本，也无法确认私钥归属，所以 M5 的真实模型验证无法进行。改为在线模型后，这条依赖整体消失。

## 3. 必须先知道的事实：K16–K18 没有接入真实链路

在 `main`（e13074e）上核对，非测试代码里：

- `KnowledgeEmbeddingClient` 从未被创建（只出现在 `embedding.rs` 自己和测试里）。
- `KnowledgeVectorIndex` 只在 `lib.rs` 里构造并 `app.manage`；`publish_revision`、`search_top_k` 没有生产调用方，所以**导入资料时不会产生任何向量**。
- `fuse_bm25_and_vector_rrf` 只被测试调用；`search/knowledge_indexer.rs` 的 `search()` 固定返回 `mode: "bm25"`、`bm25_hits: None`、`vector_hits: None`，检索测试页的「三列对比」向量列恒为空。
- K17/K18 的交付报告通过的是单元测试（合成向量），不是端到端。

结论：就算模型包已经签名发布，M5 也不会通过。本任务的主要工作量是**接线**，不是换后端。实现前先自己复核上面几点，若与现状不符以现状为准并更新本节。

## 4. 目标与非目标

目标：配置一个在线嵌入模型后，新导入和已有资料在后台生成向量，检索走 BM25 + 向量 RRF 的混合检索，同义词查询能召回；不可用时自动降级 BM25，不报错。

非目标：语音转写；本地嵌入；重排序（rerank）；多嵌入模型并存（同一时刻只有一个生效模型）。

## 5. 设计

### 5.1 配置

- `config.json` 新增 `embeddingModel`（由现有配置写入者写入，遵守「写盘前重读、锁内合并」）：
  `{ enabled: boolean, providerId: string, model: string, dimensions?: number }`。
- 密钥与地址复用现有供应商体系：`providerApiKeys[providerId]` 与供应商的 `baseUrl`；自定义端点通过现有「添加供应商」流程新增一个「OpenAI 兼容」协议的供应商，不另建第二套密钥存储。
- 界面：模型设置页新增「嵌入模型」卡片（先读 `specs/DESIGN.md` 相关章节）：选择供应商与模型名、总开关、「测试连接」。测试调用会嵌入一个短文本并回填实际维度；失败显示明确原因（认证失败、模型不存在、网络不可达、被限制模式拒绝）。
- 协议：先支持 OpenAI 兼容的 `POST {baseUrl}/embeddings`（`input` 为字符串数组，响应 `data[].embedding`）。其他协议后续按需增加，不在本任务内。`baseUrl` 必须是 https，或回环地址上的 http（本机服务）；URL 不得含凭据；不跟随重定向。

### 5.2 Owner 与进程边界

- 新增 Rust `KnowledgeEmbeddingService`，替换 `KnowledgeEmbeddingClient` 与 `KnowledgeModelPackManager`。它每次调用时读取当前配置，不缓存配置副本；HTTP 经 `proxy_config::build_client_with_proxy_for_provider(builder, provider_id)`（带供应商级代理）。
- 向量文件的写入仍只归 `KnowledgeVectorIndex`（`vectors/<模型版本>/` + `vectors/active.json` 的 generation 指针）；`KnowledgeStore` 仍是知识库内容的唯一写入权威。
- 嵌入是导入流水线的后置阶段，由 `KnowledgeIngestManager` 持有队列与状态，不另起新的调度器或进程类型。
- 在线向量是浮点，在 Rust 里用现有 `quantize_l2_int8` 量化为 int8 再存储，沿用 K18 的索引格式与 `cosine_similarity_int8`。

### 5.3 模型版本标识

`online:<主机>:<模型名>:<维度>`。模型、地址主机或维度任一变化即为新版本：后台按 generation 重嵌并原子切换，旧版本在切换成功后清理（沿用 K18 的缓存键 `(模型版本, 块内容哈希)`，同一版本内不重复请求）。

### 5.4 导入后的嵌入阶段

- 每个资料在 `ready` 后进入 `embedding` 阶段，状态独立记录：`pending | embedding | ready | failed | skipped`；失败带原因，可重试；界面在资料抽屉和库设置里显示进度（已嵌入块数 / 总块数）。
- 批量：每批最多 64 块、单批总字符数设上限（建议 100k 字符）、单块截断到模型上限；并发度默认 2。
- 429 与 5xx：指数退避，最多 5 次；`Retry-After` 优先；认证类错误（401/403）不重试，直接把整个嵌入阶段置为 `failed` 并提示配置有误。
- 取消与恢复：删除资料或库时取消该资料的待处理批次；应用重启后根据缓存键幂等续跑，不重复请求已有向量。
- 首次给一个库嵌入时，若块数超过 2000，先显示预估的块数和大致 token 量并要求用户确认一次（这是费用保护，不是隐私开关；总开关仍然是全局的）。

### 5.5 查询路径

`knowledge_indexer.rs` 的 `search()`：若总开关开启、当前模型有可用的向量版本，则对查询文本发起一次嵌入请求（超时 5 秒，查询向量做小型 LRU 缓存），调用 `search_top_k` 得到向量结果，再用 `fuse_bm25_and_vector_rrf` 融合，返回 `mode: "hybrid"` 并填充 `bm25_hits` / `vector_hits`。任何一步失败（未配置、无向量、请求失败、超时、被限制模式拒绝）都返回 `mode: "bm25"` 和可选的 `degraded_reason`，且整体延迟不得因嵌入请求而增加超过 5 秒。不得让检索命令因为嵌入失败而报错。

### 5.6 与限制模式的关系（出口 IP 核验）

在线嵌入是模型相关的出站请求。Rust 侧每批请求前：先 `ensure_model_egress_admitted()`，再 `egress_restriction::fresh_exit_ip(3s)`，不满足则该批失败并降级。Rust 侧没有域名 trace 检查（那一层在 Node 的 `admitModelTurn`），文档里如实写明：嵌入请求只有出口核验的前两层。更新 `specs/tech_docs/proxy_config.md` 的限制模式一节。

### 5.7 隐私与日志

- 块文本会发送给用户配置的在线服务，设置页「嵌入模型」卡片需有一行明确说明（中英文）。
- 🔒 日志、错误信息、诊断导出中不得出现 API Key、块文本、查询文本；错误只记录状态码和脱敏后的服务端错误类别。需要有测试覆盖。

### 5.8 退役清单（K16、K17）

- 删除 `KnowledgeModelPackManager`（`knowledge/model_pack.rs` 及其测试）、四个 `cmd_kb_embedding_pack_*` 命令及其在 `lib.rs`、`knowledgeClient.ts`、`types.ts` 的注册与封装、`embedding-model-pack-source-lock.json`。
- 删除 `ComputeWorkloadKind::KnowledgeEmbedding`（嵌入不再是本机重算力）。
- Document Worker 的 `embed` 作业与对应协议帧：删除前先确认 `ort` 和共享 ONNX Runtime 是否仍被 OCR 或语音使用——这些必须保留，只删除 embed 特有部分。
- 保留 `quantize_l2_int8`、`cosine_similarity_int8`（向量索引使用）。
- 同步更新 `specs/tech_docs/knowledge_base.md`、`tasks.md`、`delivery/K16.md`–`K18.md`（标注"K16、K17 已由 K21 取代"，不要删历史交付记录）。

## 6. 任务拆分（建议顺序，每步单独 CI 与交付报告）

- **K21-a 配置与服务**：`embeddingModel` 配置、模型设置页卡片、`KnowledgeEmbeddingService`（含测试连接）、请求/重试/脱敏。
- **K21-b 嵌入阶段接线**：导入后置阶段、状态与进度、取消与重启续跑、按生成切换模型版本、费用预估确认。
- **K21-c 检索接线**：`search()` 混合检索与降级、检索测试页三列对比真实有数据、`degraded_reason` 展示。
- **K21-d 退役 K16/K17**：5.8 的清理，保证无残留引用，ORT 共享部分不受影响。
- **K21-e 限制模式与文档**：5.6、规范文档同步。

## 7. 验收

- 自动：本机回环假嵌入服务（确定性向量）覆盖：批量、429 退避、401 不重试、维度变化、取消、重启续跑、日志脱敏、限制模式拒绝；端到端：导入 → 嵌入阶段 → 向量落盘 → `search()` 返回 `hybrid`；服务不可用时返回 `bm25` 且不报错。
- 评测：复用 K11 语料，在 credentialed 测试池（不进默认 CI）用真实供应商跑；"关键词不重合"查询 Recall@8 比 BM25 基线提升 ≥ 0.15，整体指标不下降；混合检索（不含嵌入请求）p95 < 300 ms（1 万块）。
- **M5 重新定义**：配置在线嵌入模型 → 导入资料后向量就绪 → 同义词查询（关键词不重合）能召回 → 检索测试页看到 BM25 / 向量 / 融合三列有数据 → 关闭总开关或把密钥改错后自动降级 BM25，不报错 → 改回后恢复。
- 红线与 🔒：5.7 的日志脱敏测试；`baseUrl` 校验（https 或回环 http、无凭据、不跟随重定向）；嵌入请求在限制模式下的拒绝与降级。

## 8. 给实现者的提醒

- 先复核第 3 节；确认后再动手。
- 新增网络请求只能走 Rust 的 `proxy_config` 客户端，不要使用裸 `reqwest::Client::new()`，也不要让 Renderer 直接 `fetch`。
- 改 Skill 或 CLI 契约时同步三处系统技能版本号（见 `HANDOFF.md`）。本任务预计不改 Skill。
- 合并到 `main`、打标签、发布都需要用户授权。
