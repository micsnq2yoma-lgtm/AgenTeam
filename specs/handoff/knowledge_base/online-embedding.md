# K21 在线嵌入模型（设计与任务）

状态：**设计已确认，待知识库协作者实现。** 本文取代 `tasks.md` 中 K16（嵌入模型包）与 K17（Document Worker `embed` 作业）的本地推理方案，并把 K18 真正接入检索与导入链路。语音转写（Record）**不在本文范围，保持现状**。

## 1. 决策记录（2026-11-25，用户确认）

1. 嵌入改用**在线模型**，在「设置 → 模型设置」里配置；本地模型包路线（K16、K17）退役，不保留两条路径。
2. 语音模型暂不改动。
3. 知识库文本块发送给在线嵌入服务，**用户已接受**；不要求每个知识库单独开关。仍保留一个全局总开关（关闭即只用 BM25）。
4. 设计文档由本任务的作者提供，实现与验收按 `HANDOFF.md` 的分工由知识库协作者负责。
5. （2026-11-25 追加）优先直接使用已登录供应商里的向量模型，首选词元跳动账户里的 Qwen Text Embedding V4 / Qwen3.7 Text Embedding，用户不需要再单独填写地址和密钥；自定义端点只作为没有该账户时的备选。

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
  `{ enabled: boolean, providerId: string, model: string, dimensions?: number }`。`providerId` 指向已有供应商，密钥与地址**不在此处重复保存**。
- **零填写路径（首选）**：地址和密钥直接来自供应商。词元跳动登录后，Rust 已把 Key 存在 `providerApiKeys.tokendance`，基础地址为 `https://tokendance.space/gateway/v1`（`config-types.ts` 中该供应商的 `modelProtocolBaseUrls['openai:chat-completions']`）；嵌入请求用「供应商的 OpenAI 兼容基础地址 + `/embeddings`」。用户只需在下拉里选一个向量模型。
- **备选路径**：没有词元跳动账户时，通过现有「添加供应商」流程新增一个「OpenAI 兼容」协议的自定义供应商，再在嵌入卡片里选它的向量模型名（手填）。`baseUrl` 必须是 https，或回环地址上的 http（本机服务）；URL 不得含凭据；不跟随重定向。
- 界面：模型设置页新增「嵌入模型」卡片（先读 `specs/DESIGN.md` 相关章节）：供应商 + 向量模型下拉（见 5.9）、总开关、「测试连接」。已登录词元跳动时默认推荐 Qwen Text Embedding V4（支持 64–2048 维自定义，8K 上下文）；默认维度建议 1024，实现者可按实测调整。测试连接嵌入一个短文本并回填实际维度；失败显示明确原因（认证失败、余额不足、模型不存在、网络不可达、被限制模式拒绝）。
- 协议：先支持 OpenAI 兼容的 `POST {baseUrl}/embeddings`（`input` 为字符串数组，响应 `data[].embedding`）。其他协议后续按需增加。**嵌入请求不要加进 `MODEL_PROTOCOL_PRIORITY`**——那个列表是聊天传输优先级，混入会影响聊天路由。

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
- 费用：词元跳动向量模型约 ¥0.5 / 百万 token（截图，需以实际价格为准）。1 万块、每块约 500 token 约 500 万 token，约 ¥2.5，因此**不设确认弹窗**；只在库设置里显示已嵌入块数与已用 token 的估算。其他供应商价格可能不同，卡片里提示“按供应商计费”。

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

### 5.9 模型类别与发现（向量模型）

- 现状：`ModelEntity` 只描述聊天模型，聊天模型发现（`modelDiscoveryService.ts`）会主动过滤 `tts|ocr|embedding|rerank` 类模型，说明网关的 `/v1/models` 返回里本来就混有向量模型，只是被聊天界面排除。
- 新增模型类别字段（聊天 / 向量，其余类别忽略）。向量模型的发现单独走一条路径：取同一个 `modelListUrl`，保留 id 含 `embedding` 的条目，不改变聊天模型发现的行为；聊天模型列表与现有选择不得因此变化（加回归测试）。
- 词元跳动的公开目录快照（`TOKENDANCE_MODELS`）里没有向量模型，向量模型靠运行时发现，不写进静态预设；发现失败时卡片提示“无法获取向量模型列表”，仍允许手填模型名。

## 6. 任务拆分（建议顺序，每步单独 CI 与交付报告）

- **K21-0 网关实测（先做，几分钟）**：用已登录词元跳动的 Key，对 `https://tokendance.space/gateway/v1/embeddings` 发一次请求（模型选 Qwen Text Embedding V4），记录：路径是否可用、请求/响应格式是否与 OpenAI 一致、`dimensions` 参数是否生效、批量上限、Key 权限与额度限制、延迟。结果写入交付报告。**若网关没有开放嵌入接口，停止并反馈用户**，不要自行改成别的方案；备选路径（自定义 OpenAI 兼容端点）不依赖它。
- **K21-a 配置与服务**：`embeddingModel` 配置、模型类别与向量模型发现（5.9）、模型设置页卡片、`KnowledgeEmbeddingService`（含测试连接）、请求/重试/脱敏。
- **K21-b 嵌入阶段接线**：导入后置阶段、状态与进度、取消与重启续跑、按生成切换模型版本。
- **K21-c 检索接线**：`search()` 混合检索与降级、检索测试页三列对比真实有数据、`degraded_reason` 展示。
- **K21-d 退役 K16/K17**：5.8 的清理，保证无残留引用，ORT 共享部分不受影响。
- **K21-e 限制模式与文档**：5.6、规范文档同步。

## 7. 验收

- 自动：本机回环假嵌入服务（确定性向量）覆盖：批量、429 退避、401 不重试、维度变化、取消、重启续跑、日志脱敏、限制模式拒绝；端到端：导入 → 嵌入阶段 → 向量落盘 → `search()` 返回 `hybrid`；服务不可用时返回 `bm25` 且不报错。
- 评测：复用 K11 语料，在 credentialed 测试池（不进默认 CI）用真实供应商（优先词元跳动的 Qwen Text Embedding V4）跑；"关键词不重合"查询 Recall@8 比 BM25 基线提升 ≥ 0.15，整体指标不下降；混合检索（不含嵌入请求）p95 < 300 ms（1 万块）。
- **M5 重新定义**：配置在线嵌入模型 → 导入资料后向量就绪 → 同义词查询（关键词不重合）能召回 → 检索测试页看到 BM25 / 向量 / 融合三列有数据 → 关闭总开关或把密钥改错后自动降级 BM25，不报错 → 改回后恢复。
- 零填写验收：已登录词元跳动的账户上，打开模型设置页「嵌入模型」卡片能看到向量模型并选中，**不需要再填地址或密钥**即可测试连接成功并开始嵌入；退出词元跳动登录后，卡片显示需要登录或改用自定义端点，不报错。
- 红线与 🔒：5.7 的日志脱敏测试；`baseUrl` 校验（https 或回环 http、无凭据、不跟随重定向）；嵌入请求在限制模式下的拒绝与降级。

## 8. 给实现者的提醒

- 先复核第 3 节；确认后再动手。
- 新增网络请求只能走 Rust 的 `proxy_config` 客户端，不要使用裸 `reqwest::Client::new()`，也不要让 Renderer 直接 `fetch`。
- 改 Skill 或 CLI 契约时同步三处系统技能版本号（见 `HANDOFF.md`）。本任务预计不改 Skill。
- 合并到 `main`、打标签、发布都需要用户授权。
