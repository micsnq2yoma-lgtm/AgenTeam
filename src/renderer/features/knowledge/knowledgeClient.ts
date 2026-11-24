import { invoke } from '@tauri-apps/api/core';

import type { KnowledgeBaseCreateInput, KnowledgeBaseRecord } from './types';

// Thin wrappers around the Rust control plane. Knowledge state never goes through the Sidecar.
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

export const knowledgeClient = {
  list: () => call<KnowledgeBaseRecord[]>('cmd_kb_list'),
  create: (settings: KnowledgeBaseCreateInput) =>
    call<KnowledgeBaseRecord>('cmd_kb_create', { settings }),
  remove: (kbId: string) => call<void>('cmd_kb_delete', { kbId }),
  /** Only the count is shown on the list; the full source list belongs to the detail page. */
  sourceCount: async (kbId: string): Promise<number> =>
    (await call<unknown[]>('cmd_kb_list_sources', { kbId })).length,
};

/** The Store rejects with a bounded `{ code, message }`; anything else is a generic failure. */
export function knowledgeErrorCode(error: unknown): string {
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') return error.code;
  return 'unknown';
}
