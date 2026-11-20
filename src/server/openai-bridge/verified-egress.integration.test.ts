import { generateKeyPairSync, sign, X509Certificate } from 'node:crypto';
import { createServer } from 'node:http';
import { createServer as createHttpsServer } from 'node:https';
import { connect, type AddressInfo, type Socket } from 'node:net';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { buildConnector } from 'undici';
import { fetchWithVerifiedModelEgress, forwardVerifiedAnthropic } from './verified-egress';

// The real Undici client, fetch, connection reuse, HTTP CONNECT and streaming
// run on loopback. Only the final TLS handshake is replaced by the test socket.
const fixture = vi.hoisted(() => ({ port: 0, admission: vi.fn() }));
vi.mock('../egress-model-policy', () => ({ verifyNativeModelAdmission: fixture.admission }));
vi.mock('../proxy-state', () => ({ getProxyForProviderUrl: () => undefined }));
vi.mock('undici', async importOriginal => {
  const actual = await importOriginal<typeof import('undici')>();
  const localize = (url: URL) => {
    if (!['https://api.openai.com', 'https://api.anthropic.com'].includes(url.origin)) return url;
    return new URL(url.pathname + url.search, 'https://127.0.0.1:' + fixture.port);
  };
  return { ...actual,
    Client: class extends actual.Client {
      constructor(origin: string | URL, options?: ConstructorParameters<typeof actual.Client>[1]) {
        super(localize(new URL(origin)), options);
      }
    },
    fetch: (url: URL, init: Parameters<typeof actual.fetch>[1]) => actual.fetch(localize(url), init),
    buildConnector: (tls: buildConnector.BuildOptions): buildConnector.connector => (options, callback) => {
      expect('rejectUnauthorized' in tls && tls.rejectUnauthorized).toBe(true);
      expect(options.hostname).toBe('127.0.0.1');
      if (options.httpSocket) callback(null, options.httpSocket);
      else {
        const socket = connect(fixture.port, '127.0.0.1');
        const failed = (error: Error) => callback(error, null);
        socket.once('error', failed);
        socket.once('connect', () => {
          socket.off('error', failed);
          callback(null, socket);
        });
      }
    },
  };
});

const endpoint = 'https://api.openai.com/v1/responses';
const expected = '203.0.113.10';
const post = { method: 'POST', headers: { authorization: 'Bearer test-only' }, body: 'test prompt' };

function createUntrustedProxyTls() {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  // Public DER template for a self-issued loopback-proxy-test certificate
  // (2020–2099). Insert this run's SubjectPublicKeyInfo before the IP SAN.
  const tbsCertificate = Buffer.concat([
    Buffer.from(
      'MIGyoAMCAQICAQEwBQYDK2VwMB4xHDAaBgNVBAMME2xvb3BiYWNrLXByb3h5LXRlc3Qw' +
      'IBcNMjAwMTAxMDAwMDAwWhgPMjA5OTAxMDEwMDAwMDBaMB4xHDAaBgNVBAMME2xvb3BiYWNrLXByb3h5LXRlc3Q=',
      'base64',
    ),
    publicKey.export({ type: 'spki', format: 'der' }),
    Buffer.from('oxMwETAPBgNVHREECDAGhwR/AAAB', 'base64'), // subjectAltName = 127.0.0.1
  ]);
  // Ed25519 fixes the lengths of both the public key and the 64-byte signature,
  // so the outer sequence and signature field headers are constant in this template.
  const certificate = new X509Certificate(Buffer.concat([
    Buffer.from('3081ff', 'hex'),
    tbsCertificate,
    Buffer.from('300506032b6570034100', 'hex'), // Ed25519 AlgorithmIdentifier, BIT STRING
    sign(null, tbsCertificate, privateKey),
  ]));
  expect(certificate.verify(publicKey)).toBe(true);
  expect(certificate.checkIP('127.0.0.1')).toBe('127.0.0.1');
  return { cert: certificate.toString(), key: privateKey.export({ type: 'pkcs8', format: 'pem' }) };
}

describe('model connection egress admission', () => {
  let server: ReturnType<typeof createServer>;
  let trace: string;
  let closeAfterTrace: boolean;
  let redirectModel: boolean;
  let streamAfterHeaders: boolean;
  let finishStream: (() => void) | undefined;
  let requests: { path?: string; auth?: string; apiKey?: string; beta?: string; socket: Socket; body: string }[];
  beforeEach(async () => {
    requests = [];
    closeAfterTrace = false;
    redirectModel = false;
    streamAfterHeaders = false;
    finishStream = undefined;
    trace = 'h=api.openai.com\nip=' + expected + '\n';
    fixture.admission.mockReset().mockResolvedValue(undefined);
    server = createServer((req, res) => {
      const record = {
        path: req.url, auth: req.headers.authorization, apiKey: req.headers['x-api-key'] as string,
        beta: req.headers['anthropic-beta'] as string, socket: req.socket, body: '',
      };
      requests.push(record);
      req.on('data', chunk => { record.body += chunk; });
      if (req.url === '/cdn-cgi/trace') {
        if (closeAfterTrace) res.setHeader('Connection', 'close');
        res.end(trace);
      } else if (redirectModel) {
        res.writeHead(307, { Location: '/unverified' }).end();
      } else {
        res.write('stream');
        if (streamAfterHeaders) finishStream = () => res.end(' done');
        else res.end(' done');
      }
    });
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    fixture.port = (server.address() as AddressInfo).port;
  });
  afterEach(async () => {
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  });
  it.each([false, true])('sends credentials only on the attested socket (proxy=%s)', async viaProxy => {
    streamAfterHeaders = true;
    const proxy = createServer();
    let tunnelCount = 0;
    proxy.on('connect', (req, downstream, head) => {
      expect(req.url).toBe('api.openai.com:443');
      tunnelCount++;
      const remote = connect(fixture.port, '127.0.0.1', () => {
        downstream.write('HTTP/1.1 200 OK\r\n\r\n');
        if (head.length) remote.write(head);
        downstream.pipe(remote).pipe(downstream);
      });
      downstream.on('close', () => remote.destroy());
      remote.on('close', () => downstream.destroy());
    });
    await new Promise<void>(resolve => proxy.listen(0, '127.0.0.1', resolve));
    try {
      const proxyUrl = viaProxy ? 'http://127.0.0.1:' + (proxy.address() as AddressInfo).port : undefined;
      const response = await fetchWithVerifiedModelEgress(endpoint, post, proxyUrl, expected);
      finishStream?.();
      expect(await response.text()).toBe('stream done');
      expect(tunnelCount).toBe(viaProxy ? 1 : 0);
      expect(requests.map(({ path, auth }) => [path, auth])).toEqual([
        ['/cdn-cgi/trace', undefined], ['/v1/responses', 'Bearer test-only'],
      ]);
      expect(requests[0].socket).toBe(requests[1].socket);
    } finally {
      await new Promise<void>(resolve => proxy.close(() => resolve()));
    }
  });
  it.each(['wrong IP', 'unknown IP', 'duplicate IP', 'new connection', 'host lock'])('blocks before disclosure: %s', async reason => {
    if (reason === 'wrong IP') trace = 'h=api.openai.com\nip=198.51.100.2';
    if (reason === 'unknown IP') trace = 'ip=garbage';
    if (reason === 'duplicate IP') trace += 'ip=' + expected;
    if (reason === 'new connection') closeAfterTrace = true;
    if (reason === 'host lock') fixture.admission.mockRejectedValue(new Error('locked'));
    await expect(fetchWithVerifiedModelEgress(endpoint, post, undefined, expected)).rejects.toThrow();
    expect(requests.map(({ path, auth }) => [path, auth])).toEqual([['/cdn-cgi/trace', undefined]]);
  });
  it('rejects an untrusted HTTPS proxy before sending its credentials even if TLS defaults were disabled', async () => {
    const proxy = createHttpsServer(createUntrustedProxyTls());
    let tcpConnections = 0;
    const proxyAuth: (string | undefined)[] = [];
    proxy.on('connection', () => { tcpConnections++; });
    proxy.on('connect', (req, socket) => {
      proxyAuth.push(req.headers['proxy-authorization']);
      socket.end('HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n');
    });
    await new Promise<void>(resolve => proxy.listen(0, '127.0.0.1', resolve));
    try {
      vi.stubEnv('NODE_TLS_REJECT_UNAUTHORIZED', '0');
      const proxyUrl = `https://fixture:fixture-password@127.0.0.1:${(proxy.address() as AddressInfo).port}`;
      await expect(fetchWithVerifiedModelEgress(endpoint, post, proxyUrl, expected)).rejects.toThrow();
      expect(tcpConnections).toBeGreaterThan(0);
      expect(proxyAuth).toEqual([]);
      expect(requests).toEqual([]);
    } finally {
      vi.unstubAllEnvs();
      proxy.closeAllConnections();
      await new Promise<void>(resolve => proxy.close(() => resolve()));
    }
  });
  it('rejects unsupported provider origins without contacting them', async () => {
    await expect(fetchWithVerifiedModelEgress('https://relay.invalid/v1/responses', post, undefined, expected))
      .rejects.toThrow('无法核验');
    expect(requests).toHaveLength(0);
  });
  it('admits a subsequent matching route after an earlier mismatch', async () => {
    trace = 'h=api.openai.com\nip=198.51.100.9';
    await expect(fetchWithVerifiedModelEgress(endpoint, post, undefined, expected)).rejects.toThrow('不匹配');
    trace = 'h=api.openai.com\nip=' + expected;
    const response = await fetchWithVerifiedModelEgress(endpoint, post, undefined, expected);
    await response.text();
    expect(requests.map(r => r.path)).toEqual(['/cdn-cgi/trace', '/cdn-cgi/trace', '/v1/responses']);
    expect(requests[0].socket).not.toBe(requests[1].socket);
  });
  it('does not replay credentials after a redirect', async () => {
    redirectModel = true;
    await expect(fetchWithVerifiedModelEgress(endpoint, post, undefined, expected)).rejects.toThrow();
    expect(requests.map(({ path }) => path)).toEqual(['/cdn-cgi/trace', '/v1/responses']);
  });
  it.each(['messages', 'messages/count_tokens'] as const)('forwards native Anthropic %s after proof only', async operation => {
    trace = 'h=api.anthropic.com\nip=' + expected;
    const request = new Request('http://127.0.0.1/bridge/test/anthropic/v1/' + operation + '?beta=true', {
      method: 'POST', body: '{"model":"test"}',
      headers: { 'x-api-key': 'local-only', authorization: 'local-only', 'anthropic-beta': 'test-feature' },
    });
    const response = await forwardVerifiedAnthropic(request, operation, 'anthropic-api', 'test-upstream-key', expected);
    expect(response.status).toBe(200);
    expect(await response.text()).toBe('stream done');
    expect(requests[0]).toMatchObject({ path: '/cdn-cgi/trace', apiKey: undefined, auth: undefined, body: '' });
    expect(requests[1]).toMatchObject({
      path: '/v1/' + operation + '?beta=true', apiKey: 'test-upstream-key', auth: undefined,
      beta: 'test-feature', body: '{"model":"test"}',
    });
    expect(requests[1].socket).toBe(requests[0].socket);
  });
});
