import { isIP, Socket } from 'node:net';
import { rootCertificates } from 'node:tls';
import { buildConnector, Client, fetch } from 'undici';
import { verifyNativeModelAdmission } from '../egress-model-policy';
import { getProxyForProviderUrl } from '../proxy-state';

export class ModelEgressError extends Error {}

const MODEL_ENDPOINTS = new Set([
  'https://api.openai.com/v1/chat/completions',
  'https://api.openai.com/v1/responses',
  'https://api.anthropic.com/v1/messages',
  'https://api.anthropic.com/v1/messages/count_tokens',
]);

/** Official endpoints whose edge can prove the egress. Everything else is gated
 * by the host-level and per-turn admission instead of a per-request proof. */
export function isVerifiedModelEndpoint(url: string): boolean {
  try {
    const target = new URL(url);
    return !target.username && !target.password && MODEL_ENDPOINTS.has(target.origin + target.pathname);
  } catch {
    return false;
  }
}

function canonicalIp(value: string): string | undefined {
  if (isIP(value) === 4) return value;
  if (isIP(value) === 6 && !value.includes('%')) return new URL(`http://[${value}]`).hostname;
  return undefined;
}

interface VerifiedClient {
  client: Client;
  proxyClient: Client | null;
}

/** A client whose first (and only) connection is the proof connection; a second
 * connection attempt fails so a proof can never cover a different socket.
 */
function createVerifiedClient(target: URL, proxyUrl: string | undefined, signal: AbortSignal): VerifiedClient {
  const proxy = proxyUrl ? new URL(proxyUrl) : null;
  if (proxy && !['http:', 'https:'].includes(proxy.protocol)) {
    throw new ModelEgressError('限制模式无法核验此代理连接。');
  }
  const proxyClient = proxy ? new Client(proxy.origin, {
    allowH2: false,
    // Authenticate an HTTPS proxy before sending its CONNECT credentials,
    // even when the parent environment disabled the default TLS check.
    connect: { rejectUnauthorized: true },
  }) : null;
  const tlsConnect = buildConnector({
    allowH2: false, timeout: 6_000, rejectUnauthorized: true, ca: [...rootCertificates],
  });
  let opened = false;
  const client = new Client(target.origin, {
    allowH2: false,
    connect(options, callback) {
      if (opened) {
        callback(new ModelEgressError('模型连接已更换，需要重新核验出口。'), null);
        return;
      }
      opened = true;
      if (!proxyClient || !proxy) {
        tlsConnect(options, callback);
        return;
      }
      const headers: Record<string, string> = { host: target.host + ':443' };
      if (proxy.username || proxy.password) {
        const auth = decodeURIComponent(proxy.username) + ':' + decodeURIComponent(proxy.password);
        headers['proxy-authorization'] = 'Basic ' + Buffer.from(auth).toString('base64');
      }
      void proxyClient.connect({ path: headers.host, headers, signal }).then(({ statusCode, socket }) => {
        if (statusCode !== 200 || !(socket instanceof Socket)) {
          socket.destroy();
          callback(new ModelEgressError('代理未建立模型核验隧道。'), null);
          return;
        }
        tlsConnect({ ...options, httpSocket: socket }, callback);
      }).catch((error: Error) => callback(error, null));
    },
  });
  return { client, proxyClient };
}

/** Reads `/cdn-cgi/trace` on an already-created proof client and returns the observed edge IP. */
async function readEdgeTrace(client: Client, target: URL, signal: AbortSignal): Promise<string> {
  const probe = await client.request({
    method: 'GET', path: '/cdn-cgi/trace', signal,
    headers: { 'cache-control': 'no-cache, no-store', accept: 'text/plain' },
  });
  if (probe.statusCode !== 200) throw new ModelEgressError('模型服务没有提供出口核验结果。');
  const text = new TextDecoder();
  let trace = '';
  let bytes = 0;
  for await (const chunk of probe.body) {
    bytes += chunk.byteLength;
    if (bytes > 4096) throw new ModelEgressError('模型服务的出口核验结果无效。');
    trace += text.decode(chunk, { stream: true });
  }
  trace += text.decode();
  const lines = trace.trim().split(/\r?\n/);
  const addresses = lines.filter(line => line.startsWith('ip='));
  const observed = addresses.length === 1 ? addresses[0].slice(3) : '';
  if (!lines.includes('h=' + target.hostname) || !canonicalIp(observed)) {
    throw new ModelEgressError('模型服务未返回可核验的出口 IP。');
  }
  return observed;
}

export type EdgeEgressResult =
  | { status: 'match'; observed: string }
  | { status: 'mismatch'; observed: string }
  | { status: 'unavailable' };

/** Proof-only check (no model request): which IP does `host`'s edge see through
 * the selected proxy path? Unreachable, non-Cloudflare or malformed answers are
 * `unavailable`, never a match.
 */
export async function probeEdgeEgress(
  host: string,
  proxyUrl: string | undefined,
  expectedIp: string,
  signal?: AbortSignal,
): Promise<EdgeEgressResult> {
  const probeSignal = AbortSignal.timeout(8_000);
  const effective = signal ? AbortSignal.any([signal, probeSignal]) : probeSignal;
  let created: VerifiedClient | undefined;
  try {
    const target = new URL(`https://${host}/`);
    created = createVerifiedClient(target, proxyUrl, effective);
    const observed = await readEdgeTrace(created.client, target, effective);
    return canonicalIp(observed) === canonicalIp(expectedIp)
      ? { status: 'match', observed }
      : { status: 'mismatch', observed };
  } catch (error) {
    if (signal?.aborted) throw error;
    return { status: 'unavailable' };
  } finally {
    if (created) {
      await created.client.destroy().catch(() => {});
      if (created.proxyClient) void created.proxyClient.close().catch(() => {});
    }
  }
}

/** One check and one model POST share an authenticated connection to the provider's edge.
 * A new TCP/TLS connection, even to the same origin through the same proxy, needs new proof.
 */
export async function fetchWithVerifiedModelEgress(
  url: string,
  init: NonNullable<Parameters<typeof fetch>[1]>,
  proxyUrl: string | undefined,
  expectedIp: string,
): Promise<Awaited<ReturnType<typeof fetch>>> {
  const target = new URL(url);
  if (!isVerifiedModelEndpoint(url)) {
    throw new ModelEgressError('限制模式无法核验此模型服务的实际出口。');
  }
  const probeSignal = AbortSignal.timeout(8_000);
  const signal = init.signal ? AbortSignal.any([init.signal, probeSignal]) : probeSignal;
  const { client, proxyClient } = createVerifiedClient(target, proxyUrl, signal);

  try {
    const observed = await readEdgeTrace(client, target, signal);
    if (canonicalIp(observed) !== canonicalIp(expectedIp)) {
      throw new ModelEgressError(`模型出口 IP ${observed} 与允许的 IP ${expectedIp} 不匹配。`);
    }
    await verifyNativeModelAdmission(expectedIp, signal);
    // The only call carrying the API key or prompt. Never follow a redirect
    // or re-use a proof from a different socket, model retry or provider.
    return await fetch(target, { ...init, dispatcher: client, redirect: 'error' });
  } catch (error) {
    await client.destroy();
    if (error instanceof ModelEgressError || init.signal?.aborted) throw error;
    throw new ModelEgressError('无法通过已核验的模型连接完成请求。');
  } finally {
    // Graceful close lets an admitted streaming response drain.
    void client.close().catch(() => {});
    if (proxyClient) void proxyClient.close().catch(() => {});
  }
}

/** A native Messages request keeps its body and streaming response format. Credentials
 * come exclusively from the token-bound provider record in the sidecar.
 */
export async function forwardVerifiedAnthropic(
  request: Request,
  operation: 'messages' | 'messages/count_tokens',
  providerId: string,
  apiKey: string,
  expectedIp: string,
): Promise<Response> {
  const url = new URL('/v1/' + operation, 'https://api.anthropic.com');
  if (new URL(request.url).searchParams.get('beta') === 'true') url.search = 'beta=true';
  const headers: Record<string, string> = {
    'content-type': 'application/json',
    'anthropic-version': request.headers.get('anthropic-version') || '2023-06-01',
    'x-api-key': apiKey,
  };
  for (const key of ['anthropic-beta', 'accept']) {
    const value = request.headers.get(key);
    if (value) headers[key] = value;
  }
  const deadline = new AbortController();
  const timer = setTimeout(() => deadline.abort(), 300_000);
  try {
    const response = await fetchWithVerifiedModelEgress(url.href, {
      method: 'POST', body: request.body, duplex: 'half', headers,
      signal: AbortSignal.any([request.signal, deadline.signal]),
    }, getProxyForProviderUrl(providerId, url.href), expectedIp);
    const downstream = new Headers();
    for (const key of ['content-type', 'request-id', 'retry-after']) {
      const value = response.headers.get(key);
      if (value) downstream.set(key, value);
    }
    return new Response(response.body as ReadableStream<Uint8Array> | null, { status: response.status, headers: downstream });
  } catch (error) {
    if (request.signal.aborted) throw error;
    const message = error instanceof ModelEgressError ? error.message : '无法通过已核验的 Claude API 连接完成请求。';
    return Response.json({ type: 'error', error: { type: 'permission_error', message: providerId + ' · ' + message } }, { status: 403 });
  } finally {
    clearTimeout(timer);
  }
}
