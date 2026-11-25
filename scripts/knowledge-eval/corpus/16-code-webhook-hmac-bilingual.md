# Webhook 签名校验与防重放规范

## HMAC-SHA256 Signature Verification

Webhook 接收端必须对原始请求体字节流（raw body）与时间戳拼接字符串计算 HMAC-SHA256，并使用常量时间比较函数防止时序侧信道攻击：

```ts
import crypto from 'node:crypto';

export function verifyWebhookSignature(secret: string, timestamp: string, rawBody: Buffer, sigHex: string): boolean {
  const mac = crypto.createHmac('sha256', secret).update(`${timestamp}.`).update(rawBody).digest('hex');
  const a = Buffer.from(mac, 'utf8');
  const b = Buffer.from(sigHex, 'utf8');
  return a.length === b.length && crypto.timingSafeEqual(a, b);
}
```

## Replay Protection Window 防重放窗口

Reject any webhook whose `X-Webhook-Timestamp` deviates by more than 300 seconds from server UTC time. In addition, cache processed `X-Webhook-Delivery-Id` UUIDs in Redis with a 10-minute TTL so duplicate deliveries within the clock-skew window are safely ignored.
