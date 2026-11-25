import { invoke } from '@tauri-apps/api/core';

import type {
  ApprovalDecision,
  EmbeddingConnectionTestResult,
  ImportOutcome,
  IngestJob,
  KnowledgeBaseCreateInput,
  KnowledgeBaseRecord,
  KnowledgeBaseStats,
  KnowledgeBaseUpdateInput,
  KnowledgeChunk,
  KnowledgeLinkedFolder,
  KnowledgeMetadataKey,
  KnowledgeSearchFilters,
  KnowledgeSearchResult,
  KnowledgeSourceView,
  LibraryEmbeddingSummary,
  LinkedFolderAttachResult,
  LinkedFolderScanSummary,
  LintReportEntry,
  MetadataOutcome,
  OrganizePlan,
  RuleDocKind,
  SourceEmbeddingProgress,
  UndoReport,
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
  importUrl: (kbId: string, url: string) =>
    call<ImportOutcome>('cmd_kb_import_url', { kbId, url }),
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
  addLinkedFolder: (kbId: string, path: string) =>
    call<LinkedFolderAttachResult>('cmd_kb_add_linked_folder', { kbId, path }),
  removeLinkedFolder: (kbId: string, path: string) =>
    call<number>('cmd_kb_remove_linked_folder', { kbId, path }),
  updateLinkedFolder: (
    kbId: string,
    path: string,
    options: { organizeAllowed?: boolean; gitConfirmed?: boolean },
  ) =>
    call<KnowledgeLinkedFolder>('cmd_kb_update_linked_folder', {
      kbId,
      path,
      organizeAllowed: options.organizeAllowed,
      gitConfirmed: options.gitConfirmed,
    }),
  scanLinkedFolder: (kbId: string, path: string) =>
    call<LinkedFolderScanSummary>('cmd_kb_scan_linked_folder', { kbId, path }),
  listOrganizePlans: (kbId: string) =>
    call<OrganizePlan[]>('cmd_kb_organize_list', { kbId }),
  getOrganizePlan: (kbId: string, planId: string) =>
    call<OrganizePlan>('cmd_kb_organize_get', { kbId, planId }),
  approveOrganizePlan: (kbId: string, planId: string, decision: ApprovalDecision) =>
    call<OrganizePlan>('cmd_kb_organize_approve', { kbId, planId, decision }),
  executeOrganizePlan: (kbId: string, planId: string) =>
    call<OrganizePlan>('cmd_kb_organize_execute', { kbId, planId }),
  undoOrganizePlan: (kbId: string, planId: string) =>
    call<UndoReport>('cmd_kb_organize_undo', { kbId, planId }),
  testEmbeddingConnection: (providerId: string, model: string, dimensions?: number) =>
    call<EmbeddingConnectionTestResult>('cmd_kb_test_embedding_connection', {
      providerId,
      model,
      dimensions,
    }),
  embeddingSummary: (kbId: string) =>
    call<LibraryEmbeddingSummary>('cmd_kb_embedding_summary', { kbId }),
  sourceEmbeddingStatus: (kbId: string, sourceId: string) =>
    call<SourceEmbeddingProgress>('cmd_kb_source_embedding_status', { kbId, sourceId }),
  retryEmbedding: (kbId: string, sourceId?: string) =>
    call<void>('cmd_kb_retry_embedding', { kbId, sourceId: sourceId ?? null }),
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
