# PostgreSQL 订单表按月分区方案

## 分区建表与索引脚本

交易流水表 `orders_ledger` 按 `created_at` 实施声明式范围分区，每个自然月建立独立子表，以保证热数据索引常驻内存并支持秒级历史清理：

```sql
CREATE TABLE orders_ledger (
  order_id BIGINT NOT NULL,
  tenant_id TEXT NOT NULL,
  amount_cents BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (order_id, created_at)
) PARTITION BY RANGE (created_at);

CREATE TABLE orders_ledger_2026_11 PARTITION OF orders_ledger
  FOR VALUES FROM ('2026-11-01 00:00:00+00') TO ('2026-12-01 00:00:00+00');
```

## 历史分区卸载与归档

保留最近 18 个月的在线分区；超过 18 个月的历史分区先通过 `ALTER TABLE orders_ledger DETACH PARTITION ... CONCURRENTLY` 脱离主表，导出为压缩 Parquet 文件写入对象存储后再执行 `DROP TABLE`，全程不阻塞线上写入事务。
