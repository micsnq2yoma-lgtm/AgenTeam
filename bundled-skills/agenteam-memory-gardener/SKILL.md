---
name: agenteam-memory-gardener
description: >
  仅当系统或用户明确指定完整名称 `agenteam-memory-gardener` 时使用；
  不要根据任务语义或相似表述自行触发。
metadata:
  author: AgenTeam
---

# AgenTeam Memory Gardener

本 skill 是长期记忆机制的第二层：它不捕获当天经历，而是把已经沉淀下来的
工作区已有的用户协作与长期记忆自动装载层修剪回可长期运行的状态。

核心判断：**遗忘是降级，不是删除。** 自动装载层只保留会影响每次会话行为的规则；
案例、项目细节、推导过程下放到 `memory/topics/`，并在自动装载层保留可召回指针。

## 工作区结构

先理解当前工作区内的记忆系统架构：读取工作区指令和已有记忆文件，确认各层职责、
实际路径及相互引用。沿用这套结构；缺少完成本次维护所必需的文件时，可以按现有
架构创建。文件编号、命名或分层不同，不表示需要补齐 AgenTeam 默认文件组。

下文的 SOUL / USER / MEMORY 表示职责，默认路径只是示例。脚本按文件名和默认
目录提供线索，不替代对工作区结构的理解；未识别到某层时，先判断该层是否适用。

## 边界

- `UPDATE_MEMORY.md`：24h 捕获层，只做 session 级增量沉淀。
- `agenteam-memory-gardener`：72h 整编层，只做删、下放、合并、体检。
- `agenteam-memory-molt`：14d 深反层，才允许改 SOUL / 底层信念。

## 工作流程

1. 确认当前目录是目标工作区；如需显式指定，脚本都支持 `--repo /path/to/workspace`。
2. 如果是 git 仓库，先记录工作树基线。已有的未提交改动不阻断记忆维护；保留并避开它们，只编辑本轮需要维护的记忆文件。
3. 运行 `python3 <skill>/scripts/memory_lint.py --repo <workspace>`，结合当前架构确认 RED 所指的实际问题后处理，WARN 酌情处理。
4. 读取上次园丁运行后的日志和 topic，把可长期复用的规则留下，把故事和证据下放。USER 文件使用固定章节时，同时按下文"USER 画像的整理"处理。
5. 对超预算自动装载层执行顺序：删过时项 -> 下放故事 -> 合并同根规则 -> 最后才新增。
6. 发现 SOUL / 底层信念问题时，只追加到 `memory/gardener/flags-for-molt.md`，不要直接改 SOUL。
7. 报告写入 `memory/gardener/YYYY-MM-DD.md`，再运行 `memory_lint.py --mark-run`。
8. 如果是 git 仓库，只暂存并提交本次记忆维护产生的改动；**不要 push**。不得暂存、覆盖或回滚基线中已有的其他改动。若同一个目标文件已有无法与本轮改动安全区分的未提交内容，保留该文件的全部改动、不把它纳入提交，并在报告中说明；继续处理和提交其他能够明确归属的文件。如果不是 git 仓库，跳过提交。

lint 全绿且没有新沉淀时，写一条空跑记录后结束。

## USER 画像的整理

USER 文件使用固定章节（含 `## 近期观察`，章节为：基本信息、背景、沟通风格、工作方式、偏好、写作风格、近期观察、来源概况）时，整理"近期观察"是你的职责，也是画像 Task（`agenteam-user-profile`）之外唯一整理主体章节的入口：

- 把"近期观察"里被明确表达、或跨情境反复出现且理由一致的条目，并入对应章节：背景、沟通风格、工作方式、偏好、写作风格。并入后**删除**该条，不在两处各留一份。
- 归属不明、证据不足或只出现过一次的条目，留在"近期观察"，不要为了清空而硬并。
- 用户更正过的内容优先：近期观察里的更正压过主体章节里与之矛盾的旧说法，并入时一并改掉旧说法。
- 只改你并入的那几处，其他章节逐字保留；章节标题和顺序不变；不改"来源概况"；"基本信息"只在条目是用户对名字、称呼、语言或时区的明确更正时才改。
- 不复制资料原文、联系方式或标识符进画像；不把推断写成用户画像。
- 并入后文件应净变小或持平，仍在预算内。

没有"近期观察"章节的旧式 USER 文件，沿用原有整编方式，不为它新增章节。

**用脚本写，不要整份改写文件。** `section_edit.py` 只改你点名的 `## ` 章节，其余内容逐字不变；文件在你读取之后被别的 Session 改过时拒绝写入；`--allow` 让程序强制章节归属，而不只靠自觉：

```bash
python3 <skill>/scripts/section_edit.py --file <USER 文件> --show
python3 <skill>/scripts/section_edit.py --file <USER 文件> --edits <edits.json> --base-sha <上一步的 sha256> --allow 背景,沟通风格,工作方式,偏好,写作风格,近期观察 --max-bytes 20480
```

`edits.json` 是列表，每项 `{"op": "replace|add|delete|append", "heading": "章节标题", "content": "内容"}`：`replace` 重写章节正文（章节开头标明谁来写的注释会保留），`append` 在章节末尾追加，`add` 新增章节，`delete` 删除章节。内容里不能出现 `## ` 标题。退出码 2 表示文件已被改动：重新 `--show`、重新判断，再提交一次；3 表示该章节不允许改，不要换别的方式绕过；4 表示结果超出大小上限，先精简。没有任何编辑通过时不会写入。

并入时对主体章节用 `replace`（带上合并后的完整内容），对"近期观察"用 `replace` 写回保留下来的条目；这样既不会漏掉其他章节，也不会碰"基本信息"和"来源概况"。

## 报告格式

```markdown
## Gardener Run YYYY-MM-DD
- lint: N RED / N WARN -> 处理结果各一行
- budgets: MEMORY xx.xKB / budget, USER xx.xKB / budget
- observations: 近期观察 N -> M（无此章节写无）
- operations: delete N, demote N, merge N clusters, add N
- drill / flags-for-molt / deferred: <各一行；无则写无>
```

建议 commit message：

```text
memory: gardener YYYY-MM-DD
```

## 自检

- 超预算文件净变小了吗？
- 下放内容真的落进 topic，并且自动装载层指针有效吗？
- 没有直接改 SOUL 吗？
- USER 画像并入的条目都已从"近期观察"删除，主体章节之外的内容逐字未变吗？
- `flags-for-molt.md` 只放信念级问题，不放日常整理事项吗？
- 如有 git 仓库，是否保留了工作树基线中的既有改动，且提交只包含能明确归属于本轮的记忆改动、没有 push？

具体整编手法见 [references/playbook.md](references/playbook.md)。
