# 桌面聊天 Task 结果回推 — 独立验收报告

> 分支：`acceptance/desktop-push`。验收对象：`main` @ `5d3b11fb`（含 dev/desktop-task-push 合并；系统技能版本 **78**，计划中"76"已过期）。
> 环境：真实桌面 App（CI 构建的 NSIS 安装包，release 构建，run 45170973065）、隔离 HOME、全新建助理工作区、合成简历与网页（虚构人物"林晓舟"）。
> 规则：按 `acceptance-plan.md` 第 0 节与 `desktop-push-acceptance-plan.md` 执行；🔒 为对抗用例。

## 用例结果

| 用例 | 结果 | 证据位置 | 备注 |
|---|---|---|---|
| P1 画像 Task 完成后自动汇报（DeepSeek） | TODO | | |
| P1 画像 Task 完成后自动汇报（Codex） | TODO | | |
| P2 排队不打断 | TODO | | |
| P3 🔒 聊天关闭不唤醒（session-inactive / session-gone） | TODO | | |
| P4 🔒 全局开关 taskResultPushBack | TODO | | 设置页开关与 config.json 一致性见备注 |
| P5 🔒 防链条 | TODO | | |
| P6 🔒 Task 输出是数据 | TODO | | |
| P7 默认范围 | TODO | | |
| P8 token 用量 | TODO | | 数值见下 |
| P9 回归 | TODO | | IM 子项见阻塞 |

## P8 token 数值

（推送回合 vs 普通用户消息回合，输入 / 缓存命中输入 / 输出；冷缓存一次）

## 新发现的缺陷

（复现步骤、预期与实际、严重程度）

## 阻塞项

## 环境

- App 版本：0.6.8（commit 5d3b11fb，CI release 构建）
- 系统技能版本：78
- 使用的模型：DeepSeek（deepseek provider / dsh runtime，模型 deepseek-v4-pro）；Codex（系统 codex CLI，ChatGPT 登录，模型 TODO）

## 结论

TODO（通过 / 有条件通过 / 不通过，与表格一致）
