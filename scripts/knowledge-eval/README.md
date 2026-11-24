# Knowledge Base Retrieval Evaluation Suite (K11)

本目录存放 AgenTeam 知识库检索质量评测的合成语料、相关性标注（`qrels.json`）以及离线执行脚本（`run.mjs`）。

## 版权与可再分发说明

`corpus/` 中的 40 份 Markdown 文档及 `qrels.json` 中的 60 条查询标注**全部为本项目原创编写的合成评测数据**，不含第三方受版权限制文本或真实客户隐私数据，随本仓库许可证（`AGPL-3.0-only`）自由再分发。

## 语料构成

- **文档数量**：40 份（中文 16 份、英文 12 份、中英混合 12 份）
- **题材覆盖**：会议纪要（6）、合同与法务条款（6）、技术规范与代码片段（10）、结构化表格与矩阵（8）、企业制度/运维手册/操作指南（10）
- **查询标注**：`qrels.json` 包含 60 条独立构造的查询，以 `(file, headingPath)` 标注相关切块（不依赖 `chunk_id`）：
  - **42 条词法改写查询（70.0%）**：自然语言提问，非原文复制句。
  - **18 条零关键词重合语义查询（30.0%）**：经实际 `tantivy-jieba` 分词器校验与目标片段（含标题与正文）零词元重合，用于 P1 向量检索上线后衡量语义召回增益。

## 运行方式

```bash
node scripts/knowledge-eval/run.mjs
```

常用参数：

- `--json`：额外输出完整 JSON 指标与逐条查询明细
- `--out <path>`：将 JSON 报告写入指定文件

脚本在隔离临时目录中调用真实 Rust `KnowledgeStore`、`KnowledgeIngestManager` 与 `KnowledgeIndex`（Tantivy BM25 + Jieba 分词），不联网、不读写用户 `~/.agenteam` 数据。
