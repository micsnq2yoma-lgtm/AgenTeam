/**
 * Renderer view of the Rust `KnowledgeStore` records (serde camelCase / kebab-case enums in
 * src-tauri/src/knowledge/store.rs). The Store stays the only authority; the UI only projects
 * what `cmd_kb_*` returns and refetches after every change.
 */
export type KnowledgeCurationMode = 'auto' | 'manual' | 'off';
export type KnowledgeAgentWriteAccess = 'curator-only' | 'bound-sessions';

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
}

/** Bounded error shape of the Store commands (`KnowledgeErrorCode` in shared/knowledge). */
export interface KnowledgeCommandError {
  code?: string;
  message?: string;
}
