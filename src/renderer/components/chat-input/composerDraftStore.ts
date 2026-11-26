import type {
  AgentMentionSnapshot,
  QueryMentionContext,
} from '../../../shared/agentMentions';

export interface ComposerDraftRecord {
  readonly text: string;
  readonly agentMentions?: readonly AgentMentionSnapshot[];
  readonly primaryContext?: QueryMentionContext['primaryContext'];
  readonly requiredSystemSkill?: QueryMentionContext['requiredSystemSkill'];
  /** Workspace path captured when saving so Launcher can strip workspace-bound
   * references if restored under a different workspace. */
  readonly workspacePath?: string | null;
}

export const LAUNCHER_DRAFT_KEY = 'launcher';
export const MAX_COMPOSER_DRAFTS = 50;

export function createSessionDraftKey(sessionId: string): string {
  return `session:${sessionId}`;
}

const drafts = new Map<string, ComposerDraftRecord>();

export function getComposerDraft(key: string): ComposerDraftRecord | undefined {
  return drafts.get(key);
}

/**
 * Store an unsent composer draft in renderer memory.
 * - Empty text deletes the entry.
 * - Re-inserting refreshes LRU recency; entries beyond MAX_COMPOSER_DRAFTS
 *   evict the oldest key.
 */
export function setComposerDraft(key: string, draft: ComposerDraftRecord): void {
  if (draft.text.length === 0) {
    drafts.delete(key);
    return;
  }
  drafts.delete(key);
  drafts.set(key, draft);
  while (drafts.size > MAX_COMPOSER_DRAFTS) {
    const oldestKey = drafts.keys().next().value;
    if (oldestKey === undefined) break;
    drafts.delete(oldestKey);
  }
}

export function deleteComposerDraft(key: string): void {
  drafts.delete(key);
}

/** Test-only helper for isolating in-memory drafts across test cases. */
export function clearAllComposerDrafts(): void {
  drafts.clear();
}
