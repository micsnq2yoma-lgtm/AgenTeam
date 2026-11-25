# Mitigating Cache Stampede in Go Services

## Singleflight Request Coalescing

When a hot cache entry expires simultaneously for hundreds of concurrent callers, wrap the origin loader in `golang.org/x/sync/singleflight` so only one goroutine queries the database while peers share the in-flight result:

```go
val, err, _ := group.Do(cacheKey, func() (any, error) {
    return repo.LoadCatalogSnapshot(ctx, tenantID)
})
```

## Probabilistic Early Expiration & TTL Jitter

Always add plus-or-minus 15% random TTL jitter when populating Redis keys during cold-start warmups so bulk records do not expire in the same second. For extremely expensive aggregations, serve stale cached data for up to 30 seconds while triggering an asynchronous background refresh.
