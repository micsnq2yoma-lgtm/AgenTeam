# Rust 令牌桶限流器实现说明

## Token Bucket Implementation

高并发 API 网关采用基于单调时钟（`Instant`）的令牌桶限流器，避免系统 NTP 校时跳变导致配额计算错乱：

```rust
pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last_refill: std::time::Instant,
}

impl TokenBucket {
    pub fn try_acquire(&mut self, cost: f64) -> bool {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = std::time::Instant::now();
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}
```

## Concurrency & Sharding 并发分片策略

In multi-tenant gateways, wrapping a single global `Mutex<HashMap<TenantId, TokenBucket>>` causes lock contention under burst traffic. Production workers shard buckets across 64 independent lock stripes keyed by FNV-1a hash of the tenant identifier and evict idle entries after 15 minutes of inactivity.
