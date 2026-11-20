import { getRestrictedModelExpectedIp } from '../egress-model-policy';
import { lookupBridge } from './bridge-registry';
import { forwardVerifiedAnthropic, ModelEgressError } from './verified-egress';

export async function handleVerifiedAnthropicRoute(
  request: Request, token: string, operation: 'messages' | 'messages/count_tokens',
): Promise<Response> {
  try {
    const expected = getRestrictedModelExpectedIp();
    if (!expected) throw new ModelEgressError('当前会话无法使用此受限模型连接。');
    const upstream = await lookupBridge(token, request);
    if (!upstream?.restrictedAnthropic || !upstream.apiKey) {
      throw new ModelEgressError('此会话没有已核验的 Anthropic API Key 连接。');
    }
    return await forwardVerifiedAnthropic(request, operation, upstream.providerId, upstream.apiKey, expected);
  } catch (error) {
    const message = error instanceof ModelEgressError ? error.message : '无法核验 Anthropic 模型连接，已阻止请求。';
    return Response.json({ type: 'error', error: { type: 'permission_error', message } }, { status: 403 });
  }
}
