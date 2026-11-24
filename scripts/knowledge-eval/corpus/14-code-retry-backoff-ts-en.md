# TypeScript Retry and Exponential Backoff Guide

## Full Jitter Backoff Algorithm

To prevent synchronized retry storms when an upstream microservice recovers, always combine exponential backoff with full jitter and respect server-supplied `Retry-After` headers:

```ts
export function computeRetryDelayMs(attempt: number, baseMs = 200, maxMs = 10_000): number {
  const cap = Math.min(maxMs, baseMs * 2 ** attempt);
  return Math.floor(Math.random() * cap);
}
```

## Idempotency Rules for Mutating Requests

Never automatically retry non-idempotent POST operations unless the caller attaches a unique `Idempotency-Key` header persisted by the backend for at least 24 hours. HTTP 400, 401, 403, and 422 responses are terminal client errors and must fail fast without retrying.
