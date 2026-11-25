import { BUNDLED_PLAYWRIGHT_MCP_COMMAND } from '../../shared/mcpPackages';
import { spawn as defaultSpawn } from './subprocess';

type Spawn = typeof defaultSpawn;

export type McpWarmupOutcome =
  | { status: 'warmed' }
  | { status: 'cached' }
  | { status: 'failed'; reason: 'timeout' | 'spawn_error' | 'network' };

export interface McpWarmupLaunch {
  command: string;
  args: string[];
  env: Record<string, string>;
}

const warmed = new Set<string>();

/** Servers worth a `--help` run before an agent starts them: `npx` packages (download) and the
 * bundled Playwright entry (first Node/Windows-Defender load of the large package can itself
 * outlast an agent's discovery window, e.g. agy's 10 s preload). */
export function isWarmableMcpServer(server: { type?: string; command?: string }): boolean {
  return server.type === 'stdio'
    && (server.command === 'npx' || server.command === BUNDLED_PLAYWRIGHT_MCP_COMMAND);
}

/** Test hook: forget what this process already warmed. */
export function resetMcpWarmupCacheForTests(): void {
  warmed.clear();
}

const NETWORK_ERRORS = ['enotfound', 'econnrefused', 'econnreset', 'etimedout', 'eai_again'];

/**
 * Fetch an `npx` MCP package into the npm cache before an agent starts the server itself.
 *
 * Some agents (Antigravity's `agy` 1.5.x) give a stdio MCP server only a few seconds to answer
 * discovery. A cold `npx -y <pkg>` downloads and installs first; in the acceptance diagnosis the same
 * launch was cancelled after ~4.9 s cold and enumerated 22 tools in ~2.0 s warm. Running the package's
 * own `--help` once (stdin closed, so a server exits at once) leaves it cached for the real launch.
 *
 * Best effort: a failure is returned, never thrown, and never blocks the session. Successful warmups
 * are remembered for this process so later sessions do not pay for it again.
 */
export async function warmNpxMcpPackage(
  launch: McpWarmupLaunch,
  options: { cwd?: string; timeoutMs?: number; spawn?: Spawn } = {},
): Promise<McpWarmupOutcome> {
  const key = [launch.command, ...launch.args].join('\u0000');
  if (warmed.has(key)) return { status: 'cached' };
  const spawn = options.spawn ?? defaultSpawn;
  let handle: ReturnType<Spawn>;
  try {
    handle = spawn([launch.command, ...launch.args, '--help'], {
      env: launch.env,
      cwd: options.cwd,
      stdin: 'ignore',
      stdout: 'ignore',
      stderr: 'pipe',
    });
  } catch {
    return { status: 'failed', reason: 'spawn_error' };
  }

  let stderr = '';
  const drain = (async () => {
    if (!handle.stderr) return;
    const reader = handle.stderr.getReader();
    const decoder = new TextDecoder();
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        if (stderr.length < 16_384) stderr += decoder.decode(value, { stream: true });
      }
    } catch {
      /* process exit settles handle.exited */
    } finally {
      reader.releaseLock();
    }
  })();

  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    try { handle.kill('SIGTERM'); } catch { /* already gone */ }
  }, options.timeoutMs ?? 90_000);
  try {
    await handle.exited;
  } finally {
    clearTimeout(timer);
  }
  await drain;

  if (handle.error) return { status: 'failed', reason: 'spawn_error' };
  if (timedOut) return { status: 'failed', reason: 'timeout' };
  // `--help` may exit 0 or 1 depending on the package; only a fetch problem matters here.
  const lower = stderr.toLowerCase();
  if (NETWORK_ERRORS.some((word) => lower.includes(word))) return { status: 'failed', reason: 'network' };
  warmed.add(key);
  return { status: 'warmed' };
}
