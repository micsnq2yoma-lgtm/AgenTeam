import { beforeEach, describe, expect, it, vi } from 'vitest';

import { resetMcpWarmupCacheForTests, warmNpxMcpPackage } from './mcp-warmup';

const launch = { command: 'node', args: ['npx-cli.js', '-y', '@playwright/mcp@0.0.68'], env: {} };

function handle(opts: { stderr?: string; exitCode?: number; error?: Error; never?: boolean } = {}) {
  const encoder = new TextEncoder();
  let closeStream = () => {};
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      if (opts.stderr) controller.enqueue(encoder.encode(opts.stderr));
      closeStream = () => { try { controller.close(); } catch { /* already closed */ } };
      if (!opts.never) closeStream();
    },
  });
  let release!: (code: number) => void;
  const exited = opts.never ? new Promise<number>((resolve) => { release = resolve; }) : Promise.resolve(opts.exitCode ?? 0);
  return {
    pid: 1, stdin: null, stdout: null, stderr: stream, exited,
    error: opts.error, exitSignal: undefined, unref() {},
    // Killing a real child closes its pipes too.
    kill: vi.fn(() => { release?.(-1); closeStream(); }),
  };
}

describe('warmNpxMcpPackage', () => {
  beforeEach(() => resetMcpWarmupCacheForTests());

  it('runs the package once with --help and stdin closed, then remembers it', async () => {
    const spawn = vi.fn(() => handle()) as never;
    expect(await warmNpxMcpPackage(launch, { spawn })).toEqual({ status: 'warmed' });
    expect((spawn as unknown as ReturnType<typeof vi.fn>).mock.calls[0][0]).toEqual(['node', 'npx-cli.js', '-y', '@playwright/mcp@0.0.68', '--help']);
    expect((spawn as unknown as ReturnType<typeof vi.fn>).mock.calls[0][1]).toMatchObject({ stdin: 'ignore' });
    expect(await warmNpxMcpPackage(launch, { spawn })).toEqual({ status: 'cached' });
    expect((spawn as unknown as ReturnType<typeof vi.fn>)).toHaveBeenCalledTimes(1);
  });

  it('treats a non-zero --help exit as warmed (some packages exit 1) but a fetch error as a failure', async () => {
    expect(await warmNpxMcpPackage(launch, { spawn: (() => handle({ exitCode: 1 })) as never })).toEqual({ status: 'warmed' });
    resetMcpWarmupCacheForTests();
    expect(await warmNpxMcpPackage(launch, { spawn: (() => handle({ exitCode: 1, stderr: 'npm error code ENOTFOUND registry' })) as never }))
      .toEqual({ status: 'failed', reason: 'network' });
  });

  it('does not mistake the word "proxy" in ordinary output for a network failure', async () => {
    expect(await warmNpxMcpPackage(launch, { spawn: (() => handle({ stderr: 'Usage: --proxy-server <url>' })) as never }))
      .toEqual({ status: 'warmed' });
  });

  it('kills a warmup that exceeds the timeout and reports it without throwing or caching', async () => {
    const hung = handle({ never: true });
    const result = await warmNpxMcpPackage(launch, { timeoutMs: 20, spawn: (() => hung) as never });
    expect(result).toEqual({ status: 'failed', reason: 'timeout' });
    expect(hung.kill).toHaveBeenCalled();
    expect(await warmNpxMcpPackage(launch, { spawn: (() => handle()) as never })).toEqual({ status: 'warmed' });
  });

  it('reports a spawn failure instead of throwing', async () => {
    expect(await warmNpxMcpPackage(launch, { spawn: (() => { throw new Error('ENOENT'); }) as never }))
      .toEqual({ status: 'failed', reason: 'spawn_error' });
    expect(await warmNpxMcpPackage(launch, { spawn: (() => handle({ error: new Error('boom') })) as never }))
      .toEqual({ status: 'failed', reason: 'spawn_error' });
  });
});
