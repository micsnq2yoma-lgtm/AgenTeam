// RuntimeFactory — creates and caches AgentRuntime instances (v0.1.59)

import type { RuntimeSource, RuntimeType } from '../../shared/types/runtime';
import type { AgentRuntime } from './types';
import { ClaudeCodeRuntime } from './claude-code';
import { AntigravityRuntime } from './antigravity';
import { CodexRuntime } from './codex';
import { DshRuntime } from '../integrated-runtimes/dsh/runtime';
import { admitModelAccess, admitModelTurn, modelEgressHostsForRuntime } from '../model-turn-admission';

// ─── Runtime registry ───

const runtimes: Partial<Record<RuntimeType, AgentRuntime>> = {};

// Runtime types that have actual implementations
const SUPPORTED_AGENT_RUNTIMES = new Set<RuntimeType>(['dsh', 'claude-code', 'codex', 'antigravity']);

/**
 * Restricted-mode admission for every external runtime in one place (design: proxy_config.md).
 * A session start needs the host's egress verdict; each turn additionally needs fresh evidence and,
 * where the provider's edge supports it, a matching edge-observed IP. Both are no-ops when no
 * restriction is configured.
 */
function withModelAdmission(runtime: AgentRuntime, type: RuntimeType): AgentRuntime {
  return new Proxy(runtime, {
    get(target, prop) {
      const value: unknown = Reflect.get(target, prop, target);
      if (typeof value !== 'function') return value;
      const method = value as (...args: unknown[]) => unknown;
      if (prop === 'startSession') {
        return async (...args: unknown[]) => {
          await admitModelAccess();
          return method.apply(target, args);
        };
      }
      if (prop === 'sendMessage') {
        return async (...args: unknown[]) => {
          await admitModelTurn({ hosts: modelEgressHostsForRuntime(type) });
          return method.apply(target, args);
        };
      }
      return method.bind(target);
    },
  });
}

function ensureRuntime(type: RuntimeType): AgentRuntime {
  if (!runtimes[type]) {
    switch (type) {
      case 'dsh':
        runtimes[type] = new DshRuntime();
        break;
      case 'claude-code':
        runtimes[type] = new ClaudeCodeRuntime();
        break;
      case 'antigravity':
        runtimes[type] = new AntigravityRuntime();
        break;
      case 'codex':
        runtimes[type] = new CodexRuntime();
        break;
      default:
        throw new Error(`Runtime "${type}" is not yet supported. Available: ${[...SUPPORTED_AGENT_RUNTIMES].join(', ')}`);
    }
    runtimes[type] = withModelAdmission(runtimes[type]!, type);
  }
  return runtimes[type]!;
}

/**
 * Check if a runtime type has an actual implementation (not just type definition)
 */
export function isRuntimeSupported(type: RuntimeType): boolean {
  return type === 'builtin' || SUPPORTED_AGENT_RUNTIMES.has(type);
}

/**
 * Get a runtime instance by type.
 * 'builtin' is not handled here — it uses the existing agent-session.ts path.
 */
export function getExternalRuntime(type: RuntimeType): AgentRuntime {
  if (type === 'builtin') {
    throw new Error('builtin runtime does not use AgentRuntime interface — use existing agent-session.ts path');
  }
  return ensureRuntime(type);
}

/**
 * External CLI classification (Claude Code / Codex), excluding Integrated DSH.
 * This does not imply SDK ownership: builtin-only operations must check
 * `type === 'builtin'` explicitly.
 */
export function isExternalRuntime(type: RuntimeType | undefined): boolean {
  return type === 'claude-code' || type === 'codex' || type === 'antigravity';
}

export function isDshRuntime(type: RuntimeType | undefined): type is 'dsh' {
  return type === 'dsh';
}

/**
 * Get the current runtime type from environment or default to 'builtin'
 */
export function getCurrentRuntimeType(): RuntimeType {
  const env = process.env.MYAGENTS_RUNTIME;
  if (env === 'dsh' || env === 'claude-code' || env === 'codex' || env === 'antigravity') return env;
  return 'builtin';
}

/**
 * Get the current runtime source from environment.
 *
 * Missing source on existing external runtime sessions is intentionally
 * interpreted as system-cli for backward compatibility.
 */
export function getCurrentRuntimeSource(): RuntimeSource | undefined {
  if (getCurrentRuntimeType() === 'dsh') return 'integrated';
  if (!isExternalRuntime(getCurrentRuntimeType())) return undefined;
  return getCurrentRuntimeType() === 'codex' && process.env.MYAGENTS_RUNTIME_SOURCE === 'managed-provider'
    ? 'managed-provider'
    : 'system-cli';
}
