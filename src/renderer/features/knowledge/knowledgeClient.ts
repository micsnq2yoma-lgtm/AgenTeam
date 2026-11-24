import { invoke } from '@tauri-apps/api/core';

import type {
  ImportOutcome,
  IngestJob,
  KnowledgeBaseCreateInput,
  KnowledgeBaseRecord,
  KnowledgeBaseStats,
  KnowledgeBaseUpdateInput,
  KnowledgeChunk,
  KnowledgeMetadataKey,
  KnowledgeSearchFilters,
  KnowledgeSearchResult,
  KnowledgeSourceView,
  LintReportEntry,
  MetadataOutcome,
  RuleDocKind,
  WikiPageEntry,
} from './types';

// Thin wrappers around the Rust control plane. Knowledge state never goes through the Sidecar.
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

export const knowledgeClient = {
  list: () => call<KnowledgeBaseRecord[]>('cmd_kb_list'),
  create: (settings: KnowledgeBaseCreateInput) =>
    call<KnowledgeBaseRecord>('cmd_kb_create', { settings }),
  update: (kbId: string, settings: KnowledgeBaseUpdateInput) =>
    call<KnowledgeBaseRecord>('cmd_kb_update', { kbId, settings }),
  remove: (kbId: string) => call<void>('cmd_kb_delete', { kbId }),
  stats: (kbId: string) => call<KnowledgeBaseStats>('cmd_kb_stats', { kbId }),
  /** Only the count is shown when stats are unavailable. */
  sourceCount: async (kbId: string): Promise<number> =>
    (await call<unknown[]>('cmd_kb_list_sources', { kbId })).length,
  listSourceViews: (kbId: string) =>
    call<KnowledgeSourceView[]>('cmd_kb_list_source_views', { kbId }),
  removeSource: (kbId: string, sourceId: string) =>
    call<void>('cmd_kb_remove_source', { kbId, sourceId }),
  setMetadata: (
    kbId: string,
    sourceId: string,
    fields: Partial<Record<KnowledgeMetadataKey, unknown>>,
  ) =>
    call<Partial<Record<KnowledgeMetadataKey, MetadataOutcome>>>('cmd_kb_set_metadata', {
      kbId,
      sourceId,
      fields,
    }),
  unlockMetadata: (kbId: string, sourceId: string, key: KnowledgeMetadataKey) =>
    call<unknown>('cmd_kb_unlock_metadata', { kbId, sourceId, key }),
  listCategories: (kbId: string) => call<string[]>('cmd_kb_list_categories', { kbId }),
  confirmCategory: (kbId: string, category: string) =>
    call<void>('cmd_kb_confirm_category', { kbId, category }),
  dismissCategory: (kbId: string, category: string) =>
    call<void>('cmd_kb_dismiss_category', { kbId, category }),
  curateNow: (kbId: string) => call<string>('cmd_kb_curate_now', { kbId }),
  importFiles: (kbId: string, paths: string[]) =>
    call<ImportOutcome[]>('cmd_kb_import_files', { kbId, paths }),
  jobStatus: (jobId: string) => call<IngestJob>('cmd_kb_job_status', { jobId }),
  cancelJob: (jobId: string) => call<IngestJob>('cmd_kb_cancel_job', { jobId }),
  retrySource: (kbId: string, sourceId: string) =>
    call<IngestJob>('cmd_kb_retry_source', { kbId, sourceId }),
  listChunks: (kbId: string, sourceId: string) =>
    call<KnowledgeChunk[]>('cmd_kb_list_chunks', { kbId, sourceId }),
  search: (
    kbIds: string[],
    queries: string[],
    filters?: KnowledgeSearchFilters,
    limit?: number,
  ) =>
    call<KnowledgeSearchResult>('cmd_kb_search', {
      kbIds,
      queries,
      filters: filters && (filters.tag || filters.category || filters.docType) ? filters : undefined,
      limit,
    }),
  listWikiPages: (kbId: string) => call<WikiPageEntry[]>('cmd_kb_list_wiki_pages', { kbId }),
  readWikiPage: (kbId: string, page: string) =>
    call<string>('cmd_kb_read_wiki_page', { kbId, page }),
  listLintReports: (kbId: string) =>
    call<LintReportEntry[]>('cmd_kb_list_lint_reports', { kbId }),
  readRuleDoc: (kbId: string, doc: RuleDocKind) =>
    call<string | null>('cmd_kb_read_rule_doc', { kbId, doc }),
  writeRuleDoc: (kbId: string, doc: RuleDocKind, content: string) =>
    call<void>('cmd_kb_write_rule_doc', { kbId, doc, content }),
};

/** The Store rejects with a bounded `{ code, message }` or bare string error code. */
export function knowledgeErrorCode(error: unknown): string {
  if (typeof error === 'string' && error.startsWith('KB_')) return error;
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') {
    return error.code;
  }
  return 'unknown';
}

export function formatStorageBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`;
  const gb = mb / 1024;
  return `${gb.toFixed(gb < 10 ? 2 : 1)} GB`;
}
