import { DEFAULT_CONFIG } from './config-types';
import {
  coercePermissionModeForRuntime,
  getDefaultRuntimePermissionMode,
  type RuntimeType,
} from './types/runtime';
import {
  REASONING_EFFORT_DEFAULT,
  coerceReasoningEffortSettingForRuntime,
} from './reasoningEffort';

/** App-wide preferences use the Claude Agent SDK labels and storage keys. */
export function normalizeDefaultSessionPermissionMode(mode: unknown): 'auto' | 'plan' | 'fullAgency' {
  if (mode === 'plan') return 'plan';
  return mode === undefined || mode === 'fullAgency' ? 'fullAgency' : 'auto';
}

/** The preference applies to new desktop sessions in both integrated runtimes. */
export function getDefaultNewSessionPermissionMode(
  runtime: RuntimeType,
  preference: unknown = DEFAULT_CONFIG.defaultPermissionMode,
): string {
  if (runtime === 'builtin') return normalizeDefaultSessionPermissionMode(preference);
  if (runtime === 'dsh') {
    return normalizeDefaultSessionPermissionMode(preference) === 'fullAgency'
      ? 'full-autonomous'
      : 'approval-required';
  }
  return getDefaultRuntimePermissionMode(runtime);
}

export function coerceRuntimeBirthPermissionMode(
  value: string | null | undefined,
  runtime: RuntimeType,
): string | undefined {
  if (value === undefined || value === null) return undefined;
  return coercePermissionModeForRuntime(value, runtime)
    ?? getDefaultRuntimePermissionMode(runtime);
}

export function coerceRuntimeBirthReasoningEffort(
  value: string | null | undefined,
  runtime: RuntimeType,
): string | undefined {
  if (value === undefined || value === null) return undefined;
  return coerceReasoningEffortSettingForRuntime(value, runtime)
    ?? REASONING_EFFORT_DEFAULT;
}
