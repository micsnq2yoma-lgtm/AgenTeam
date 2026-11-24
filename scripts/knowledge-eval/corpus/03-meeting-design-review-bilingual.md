# Design Token 主题迁移评审纪要

## Semantic Palette & Contrast Check

本次评审确认所有组件禁止直接硬编码 hex 颜色，必须改为引用语义令牌（如 `--ink-primary`、`--surface-raised` 与 `--accent-warm`）。For dark theme surfaces, body text contrast must stay at or above 4.5:1 against `--surface-base`, while interactive focus rings must maintain at least 3:1 contrast against adjacent containers.

## Migration Rollout Plan

前端各业务线按模块分两批推进迁移：第一批覆盖 Launcher、Settings 与 Knowledge Base 侧边栏，目标在 Sprint 14 结束前清零静态色值告警；第二批覆盖 Chat Composer 与 Artifact Preview 面板，并配合 `verify:theme-css` 脚本在 CI 阶段拦截未登记的裸色值。
