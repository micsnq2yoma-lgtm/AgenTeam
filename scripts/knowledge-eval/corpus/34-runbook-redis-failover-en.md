# Redis Sentinel Failover and Recovery Runbook

## Detecting Quorum Loss & Promoting a Replica

If Sentinel logs report `+odown` without transitioning to `+switch-master` within 30 seconds, inspect sentinel quorum across all three availability zones using `redis-cli -p 26379 SENTINEL ckquorum mymaster`. Verify that at least two Sentinel instances are reachable before forcing a manual failover with `SENTINEL failover mymaster`.

## Preventing Split-Brain Writes

Production Redis primaries must configure `min-replicas-to-write 1` and `min-replicas-max-lag 10`. When a partitioned old primary reconnects after network healing, confirm it demotes cleanly to a replica and verify replication offset convergence before re-enabling write traffic to affected cache shards.
