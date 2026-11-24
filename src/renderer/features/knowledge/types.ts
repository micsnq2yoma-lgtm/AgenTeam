/**
 * Renderer view of the Rust `KnowledgeStore`, `KnowledgeIngestManager` and `SearchEngine`
 * records. The Rust Store stays the only authority; the UI only projects what `cmd_kb_*`
 * returns and refetches on user mutations or `knowledge:changed` events.
 */
export type KnowledgeCurationMode = 'auto' | 'manual' | 'off';
export type KnowledgeAgentWriteAccess = 'curator-only' | 'bound-sessions';
export type KnowledgeSourceKind = 'file' | 'note' | 'wiki' | 'linked-file';
export type KnowledgeSourceStatus =
  | 'queued'
  | 'converting'
  | 'chunking'
  | 'ready'
  | 'failed'
  | 'cancelled'
  | 'interrupted';
export type KnowledgeSearchStatus = 'pending' | 'indexed' | 'failed';
export type KnowledgeMetadataOrigin = 'agent' | 'user';
export type KnowledgeMetadataKey =
  | 'summary'
  | 'tags'
  | 'category'
  | 'docType'
  | 'date'
  | 'entities'
  | 'language'
  | 'duplicateOf';

export interface KnowledgeBaseRecord {
  id: string;
  name: string;
  description: string;
  curatorAgentId?: string;
  curationMode: KnowledgeCurationMode;
  curationIntervalMinutes: number;
  agentWriteAccess: KnowledgeAgentWriteAccess;
  wikiEnabled: boolean;
  pendingCategories: string[];
  pendingCategoryReasons?: Record<string, string>;
}

export interface KnowledgeBaseCreateInput {
  name: string;
  description?: string;
  locale?: string;
}

export interface KnowledgeBaseUpdateInput {
  name?: string;
  description?: string;
  curatorAgentId?: string | null;
  curationMode?: KnowledgeCurationMode;
  curationIntervalMinutes?: number;
  agentWriteAccess?: KnowledgeAgentWriteAccess;
  wikiEnabled?: boolean;
}

export interface KnowledgeBaseStats {
  kbId: string;
  sourceCount: number;
  storageBytes: number;
  searchMode: string;
  pendingCurationCount: number;
}

export interface KnowledgeMetadataField<T = unknown> {
  value: T;
  origin: KnowledgeMetadataOrigin;
  updatedAt: string;
}

export interface SourceFingerprint {
  size: number;
  mtimeMs: number;
}

export interface ChunkPosition {
  row: number;
  column: number;
  byteOffset: number;
}

export interface KnowledgeSourceView {
  id: string;
  displayName?: string;
  kind: KnowledgeSourceKind;
  currentPath?: string;
  contentHash?: string;
  fileFingerprint?: SourceFingerprint;
  status: KnowledgeSourceStatus;
  metadata: Partial<Record<KnowledgeMetadataKey, KnowledgeMetadataField>>;
  curationPending: boolean;
  jobId?: string;
  ingestError?: string;
  ingestSuggestion?: string;
  ingestPosition?: ChunkPosition;
  searchStatus: KnowledgeSearchStatus;
}

export interface IngestJob {
  id: string;
  kbId: string;
  sourceId: string;
  status: KnowledgeSourceStatus;
  error?: string;
  suggestion?: string;
  position?: ChunkPosition;
}

export interface ImportReceipt {
  sourceId: string;
  job?: IngestJob;
  duplicate: boolean;
}

export interface ImportFailure {
  code: string;
  suggestion: string;
}

export type ImportOutcome =
  | { outcome: 'accepted'; receipt: ImportReceipt }
  | { outcome: 'rejected'; error: ImportFailure };

export interface KnowledgeChunk {
  chunkId: string;
  ordinal: number;
  headingPath: string[];
  page?: number;
  prefix: string;
  text: string;
  byteRange: [number, number];
  utf16Range: [number, number];
}

export interface KnowledgeSearchFilters {
  tag?: string;
  category?: string;
  docType?: string;
}

export interface KnowledgeSearchHit {
  reference: string;
  kbId: string;
  sourceId: string;
  chunkId: string;
  title: string;
  prefix: string;
  page?: number;
  snippet: string;
  score: number;
}

export interface KnowledgeSearchResult {
  mode: string;
  hits: KnowledgeSearchHit[];
}

export interface WikiPageEntry {
  path: string;
  sizeBytes: number;
  updatedAtMs?: number;
}

export type LintKind = 'contradiction' | 'stale' | 'orphan' | 'missing-citation';

export interface LintItem {
  kind: LintKind;
  page: string;
  message: string;
  refs: string[];
}

export interface LintReport {
  version: number;
  generatedAt: string;
  items: LintItem[];
}

export interface LintReportEntry {
  id: string;
  report: LintReport;
}

export type RuleDocKind = 'organize-rules' | 'wiki-schema';

export type MetadataOutcome =
  | { status: 'updated' }
  | { status: 'rejected'; code: string };

export interface KnowledgeChangeEvent {
  kind: 'kb-upserted' | 'kb-removed' | 'source-upserted' | 'source-removed' | 'metadata-changed' | 'resync';
  kbId: string;
  sourceId?: string;
}

/** Bounded error shape of the Store commands (`KnowledgeErrorCode` in shared/knowledge). */
export interface KnowledgeCommandError {
  code?: string;
  message?: string;
}
