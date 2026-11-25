import { readFileSync } from 'node:fs';
import { isIP } from 'node:net';
import { join } from 'node:path';
import { stripBom } from '../shared/utils';
import type { ProviderEnv } from './provider-types';
import { getHomeDirOrNull } from './utils/platform';

const UNKNOWN_POLICY = '无法确认限制模式配置，已暂停模型请求。';
const PROCESS_POLICY_CHANGED = '限制模式配置已变更，请等待客户端重新启动。';
const birthExpectedIp = process.env.MYAGENTS_EGRESS_EXPECTED_IP;

export function readModelEgressPolicy(path: string, birthIp: string | undefined): string | null {
  let config: unknown;
  try {
    config = JSON.parse(stripBom(readFileSync(path, 'utf8')));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw new Error(UNKNOWN_POLICY);
    config = {};
  }
  if (!config || typeof config !== 'object' || Array.isArray(config)) throw new Error(UNKNOWN_POLICY);
  const restriction = (config as { egressRestriction?: unknown }).egressRestriction;
  let expected: string | null = null;
  if (restriction !== undefined) {
    if (!restriction || typeof restriction !== 'object' || !('enabled' in restriction)
      || typeof restriction.enabled !== 'boolean') throw new Error(UNKNOWN_POLICY);
    const literal = 'expectedIp' in restriction ? restriction.expectedIp : '';
    if (typeof literal !== 'string') throw new Error(UNKNOWN_POLICY);
    if (restriction.enabled) {
      const address = literal.trim();
      const family = isIP(address);
      if (!family || address.includes('%')) throw new Error(UNKNOWN_POLICY);
      expected = family === 6 ? new URL(`http://[${address}]`).hostname.slice(1, -1) : address;
    }
  }
  // A settings write may race an existing process. Neither the old nor the
  // new policy authorizes that generation until native startup checks it.
  if (birthIp !== undefined && birthIp !== (expected ?? '')) throw new Error(PROCESS_POLICY_CHANGED);
  return expected;
}

export function getRestrictedModelExpectedIp(): string | null {
  const home = getHomeDirOrNull();
  if (!home) throw new Error(UNKNOWN_POLICY);
  return readModelEgressPolicy(join(home, '.agenteam', 'config.json'), birthExpectedIp);
}

export async function verifyNativeModelAdmission(
  expectedIp: string,
  signal: AbortSignal,
  /** Per-turn callers require evidence no older than this; omitted accepts the current native verdict. */
  maxAgeMs?: number,
): Promise<void> {
  signal.throwIfAborted();
  const { managementApi } = await import('./utils/management-api-client');
  const path = maxAgeMs === undefined ? '/api/model-egress/admission' : `/api/model-egress/admission?maxAgeMs=${Math.max(0, Math.floor(maxAgeMs))}`;
  const response = await managementApi(path, 'GET', undefined, {
    timeoutMs: maxAgeMs === undefined ? 3_000 : 10_000, parentSignal: signal,
  });
  signal.throwIfAborted();
  if (response.ok !== true || response.expectedIp !== expectedIp) {
    throw new Error('客户端尚未确认当前出口 IP，已暂停模型请求。');
  }
}

export function supportsVerifiedModelProvider(provider: ProviderEnv | undefined): boolean {
  if (!provider?.baseUrl || !provider.apiKey?.trim() || provider.providerId === 'anthropic-sub'
    || provider.credentialSource || provider.endpointSource) return false;
  try {
    const url = new URL(provider.baseUrl);
    if (url.username || url.password || url.search || url.hash) return false;
    if (provider.apiProtocol === 'openai') {
      return url.origin === 'https://api.openai.com' && /^\/v1\/?$/.test(url.pathname);
    }
    if (provider.apiProtocol !== undefined && provider.apiProtocol !== 'anthropic') return false;
    return url.origin === 'https://api.anthropic.com' && url.pathname === '/'
      && (!provider.authType || provider.authType === 'api_key' || provider.authType === 'both');
  } catch {
    return false;
  }
}

/** OpenAI always needs translation. Native Messages enters the loopback bridge
 * only while using a verified official API-key provider. */
export function usesSdkModelBridge(provider: ProviderEnv | undefined): provider is ProviderEnv {
  return provider?.apiProtocol === 'openai'
    || (supportsVerifiedModelProvider(provider) && getRestrictedModelExpectedIp() !== null);
}

export function modelSdkSettingSources(): 'project'[] {
  // Project settings can supply environment variables for the native CLI.
  // Protected model sessions take their routing from the host exclusively.
  return getRestrictedModelExpectedIp() === null ? ['project'] : [];
}

/** True when this provider takes the verified-bridge route (official API key, per-request edge proof).
 * Any other provider is no longer rejected: it runs on its normal route under the host-level gate and
 * the per-turn admission (`admitModelTurn`), which the host retires when the egress is lost.
 */
export function assertRestrictedProvider(provider: ProviderEnv | undefined): boolean {
  return getRestrictedModelExpectedIp() !== null && supportsVerifiedModelProvider(provider);
}

// Routing selectors recognized by the bundled Claude Code binary. Only the
// Sidecar needs the real upstream key. The SDK speaks to its local bridge.
export function sealRestrictedSdkRoute(env: NodeJS.ProcessEnv, loopbackUrl: string): void {
  const routing: Record<string, string> = {
    ANTHROPIC_BASE_URL: loopbackUrl,
    ANTHROPIC_API_KEY: 'agenteam-local-bridge',
    ANTHROPIC_AUTH_TOKEN: '',
    ANTHROPIC_CUSTOM_HEADERS: '',
    HTTP_PROXY: '',
    HTTPS_PROXY: '',
    ALL_PROXY: '',
    NO_PROXY: '127.0.0.1,localhost,::1',
    CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST: '1',
    CLAUDE_CODE_DISABLE_MODEL_ACCESS_FALLBACK: '1',
    CLAUDE_CODE_API_BASE_URL: '',
    CLAUDE_CODE_USE_BEDROCK: '',
    CLAUDE_CODE_USE_VERTEX: '',
    CLAUDE_CODE_USE_FOUNDRY: '',
    CLAUDE_CODE_USE_MANTLE: '',
    CLAUDE_CODE_USE_ANTHROPIC_AWS: '',
    CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD: '',
    CLAUDE_CODE_USE_GATEWAY: '',
    CLAUDE_CODE_GATEWAY_TOKEN: '',
    CLAUDE_CODE_GATEWAY_TOKEN_FILE_DESCRIPTOR: '',
    CLAUDE_CODE_OAUTH_TOKEN: '',
    CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR: '',
  };
  // Windows folds environment key names when converting the JS object for
  // spawn. Keep just one spelling so a competing inherited value cannot win.
  for (const key of Object.keys(env)) {
    const canonical = key.toUpperCase();
    if (key !== canonical && Object.hasOwn(routing, canonical)) delete env[key];
  }
  Object.assign(env, routing);
  // Proxy clients on Unix commonly prefer lowercase over uppercase.
  Object.assign(env, { http_proxy: '', https_proxy: '', all_proxy: '', no_proxy: routing.NO_PROXY });
}
