# 生产服务内存泄漏与 CPU 采样排查手册

## Heap Growth & Memory Leak Triage 内存持续增长排查

当服务进程在业务流量平稳时常驻内存（RSS）呈单调阶梯状攀升且垃圾回收后无法回落，应间隔 15 分钟连续采集两份堆快照（Heap Snapshot / `pprof/heap`），对比两次快照之间存活对象数量净增长最多的分配调用栈，重点排查未清理的全局 Map 缓存、忘记取消订阅的事件监听器以及滞留闭包。

## CPU Flamegraph Sampling 火焰图采样分析

Use `perf record -F 99 -g` on Linux or `tokio-console` / `pprof/profile?seconds=30` to capture production CPU hotspots with less than 2% overhead. Wide flat plateaus at the top of a flamegraph indicate self-time CPU bottlenecks such as unindexed regex compilation or repeated JSON serialization inside tight loops.
