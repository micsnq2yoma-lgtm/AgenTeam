# 灰度发布与自动回滚操作指南

## Canary Traffic Stages 灰度放量阶段

每次生产发布依次经过四个流量切分阶段：1%（观察 15 分钟）→ 10%（观察 30 分钟）→ 50%（观察 30 分钟）→ 100% 全量切换。During each soak window, operators watch error ratio, P99 latency, and memory RSS slope on the release dashboard before approving promotion.

## Automatic Rollback Triggers 自动回滚红线

发布控制器满足以下任一条件即自动中止放量并回退至上一稳定版本：5xx 服务端错误率较基线升高超过 0.5 个百分点且持续两分钟、核心接口 P99 延迟突破基线的 1.8 倍，或新版本 Pod 在十分钟内发生两次以上 OOMKilled 重启。
