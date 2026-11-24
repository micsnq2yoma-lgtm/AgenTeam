# API 套餐配额与突发限流矩阵

## Tier Quota Matrix 套餐配额对比表

| Tier 套餐 | Sustained RPS 稳态速率 | Burst Capacity 突发容量 | Daily Token Cap 每日上限 | Concurrent Streams 并发流 |
| --- | --- | --- | --- | --- |
| Free 免费版 | 2 req/s | 5 | 100,000 | 2 |
| Pro 专业版 | 15 req/s | 40 | 2,000,000 | 10 |
| Team 团队版 | 50 req/s | 150 | 15,000,000 | 40 |
| Enterprise 企业版 | 250 req/s | 800 | Custom 定制 | 200 |

## Over-Quota Behavior 超额响应行为

当调用速率超出突发桶容量时，网关返回 HTTP 429 并附带 `Retry-After` 秒数以及 `X-RateLimit-Reset` UNIX 时间戳。Enterprise 租户可申请开启软限流告警模式，在超过配额 15 分钟内仅推送告警不直接阻断关键生产流量。
