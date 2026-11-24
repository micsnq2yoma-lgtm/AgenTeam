# gRPC OpenTelemetry 链路追踪与采样规范

## Trace Context Propagation 链路传播

All internal gRPC services inject and extract W3C `traceparent` and `tracestate` headers via shared interceptor middleware. 业务日志必须同时打印 `trace_id` 与 `span_id` 字段，禁止把用户隐私正文或认证令牌写入 Span Attribute。

## Tail-Based Sampling 尾部采样策略

Head sampling is fixed at 5% for normal HTTP/gRPC traffic to control collector storage costs, while the OpenTelemetry Collector applies tail-based sampling that retains 100% of traces containing non-OK gRPC status codes or end-to-end latency above 1500 ms.
