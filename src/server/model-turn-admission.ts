import type { RuntimeType } from '../shared/types/runtime';
import { getRestrictedModelExpectedIp, supportsVerifiedModelProvider, verifyNativeModelAdmission } from './egress-model-policy';
import { ModelEgressError, probeEdgeEgress } from './openai-bridge/verified-egress';
import type { ProviderEnv } from './provider-types';
import { getProxyForProviderUrl } from './proxy-state';

/**
 * Restricted-mode admission for model work that is not an official API-key request.
 *
 * The host (Rust) owns "this computer's public exit equals the allowed IP" and retires every
 * Sidecar when that stops holding. These helpers add the two checks a Sidecar can make itself:
 *  - layer 1: the host verdict is fresh (not the up-to-25 s cached one) before a turn starts;
 *  - layer 2: for providers whose edge answers `/cdn-cgi/trace`, the edge sees the allowed IP
 *    through the selected proxy path. Official hosts fail closed; other hosts are rejected only
 *    on a positive mismatch because many relays simply have no trace endpoint.
 * Official API-key providers keep their per-request proof and need neither.
 *
 * Design and limits: specs/tech_docs/proxy_config.md (出口 IP 限制模式).
 */

const FRESH_EVIDENCE_MS = 3_000;
const OFFICIAL_TRACE_HOSTS: ReadonlySet<string> = new Set(['api.anthropic.com', 'api.openai.com', 'chatgpt.com']);

function hostOf(url: string | undefined): string | undefined {
  if (!url) return undefined;
  try {
    return new URL(url).hostname.toLowerCase() || undefined;
  } catch {
    return undefined;
  }
}

/** Provider-owned model hosts (builtin SDK). */
export function modelEgressHostsForProvider(provider: ProviderEnv | undefined): string[] {
  if (!provider) return [];
  if (provider.providerId === 'anthropic-sub') return ['api.anthropic.com'];
  const host = hostOf(provider.baseUrl);
  return host ? [host] : [];
}

/** Runtime-owned model hosts (external runtimes). Hosts without a trace endpoint are omitted. */
export function modelEgressHostsForRuntime(runtime: RuntimeType): string[] {
  switch (runtime) {
    case 'claude-code':
      return ['api.anthropic.com'];
    case 'codex':
      return ['chatgpt.com'];
    default:
      return [];
  }
}

/** Builtin providers on the verified-bridge route are proven per request. */
export function builtinTurnNeedsAdmission(provider: ProviderEnv | undefined): boolean {
  return getRestrictedModelExpectedIp() !== null && !supportsVerifiedModelProvider(provider);
}

/** Layer 1 only, using the host's current verdict. For non-turn model-adjacent work (catalogues, probes). */
export async function admitModelAccess(signal?: AbortSignal): Promise<void> {
  const expected = getRestrictedModelExpectedIp();
  if (expected === null) return;
  await verifyNativeModelAdmission(expected, signal ?? new AbortController().signal);
}

/** Layers 1 and 2 before a model turn starts. Throws a user-facing error to refuse the turn. */
export async function admitModelTurn(
  target: { hosts?: readonly string[]; providerId?: string },
  signal?: AbortSignal,
): Promise<void> {
  const expected = getRestrictedModelExpectedIp();
  if (expected === null) return;
  const effective = signal ?? new AbortController().signal;
  await verifyNativeModelAdmission(expected, effective, FRESH_EVIDENCE_MS);
  for (const host of new Set(target.hosts ?? [])) {
    const official = OFFICIAL_TRACE_HOSTS.has(host);
    const proxy = getProxyForProviderUrl(target.providerId ?? '', `https://${host}/`);
    const result = await probeEdgeEgress(host, proxy, expected, effective);
    if (result.status === 'mismatch') {
      throw new ModelEgressError(`${host} 看到的出口 IP 是 ${result.observed}，与允许的 ${expected} 不一致，已暂停本轮模型请求。`);
    }
    if (result.status === 'unavailable' && official) {
      throw new ModelEgressError(`无法核验 ${host} 看到的出口 IP，已暂停本轮模型请求。请检查网络后重试。`);
    }
  }
}
