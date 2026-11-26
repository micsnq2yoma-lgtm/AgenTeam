import { beforeEach, describe, expect, it } from 'vitest';

import {
  clearAllComposerDrafts,
  createSessionDraftKey,
  deleteComposerDraft,
  getComposerDraft,
  LAUNCHER_DRAFT_KEY,
  MAX_COMPOSER_DRAFTS,
  setComposerDraft,
} from './composerDraftStore';

describe('composerDraftStore', () => {
  beforeEach(() => {
    clearAllComposerDrafts();
  });

  it('stores, reads, and deletes drafts by key', () => {
    const key = createSessionDraftKey('sess-1');
    expect(key).toBe('session:sess-1');
    expect(LAUNCHER_DRAFT_KEY).toBe('launcher');
    expect(getComposerDraft(key)).toBeUndefined();

    setComposerDraft(key, { text: 'hello world', workspacePath: '/mock/Mino' });
    expect(getComposerDraft(key)).toEqual({
      text: 'hello world',
      workspacePath: '/mock/Mino',
    });

    deleteComposerDraft(key);
    expect(getComposerDraft(key)).toBeUndefined();
  });

  it('treats empty text as delete', () => {
    setComposerDraft(LAUNCHER_DRAFT_KEY, { text: '打算找个人' });
    expect(getComposerDraft(LAUNCHER_DRAFT_KEY)?.text).toBe('打算找个人');

    setComposerDraft(LAUNCHER_DRAFT_KEY, { text: '' });
    expect(getComposerDraft(LAUNCHER_DRAFT_KEY)).toBeUndefined();
  });

  it('evicts the oldest entry when exceeding MAX_COMPOSER_DRAFTS and refreshes recency on overwrite', () => {
    for (let i = 0; i < MAX_COMPOSER_DRAFTS; i += 1) {
      setComposerDraft(`session:s-${i}`, { text: `draft-${i}` });
    }
    // Touch s-0 so s-1 becomes the oldest entry.
    setComposerDraft('session:s-0', { text: 'draft-0-updated' });
    setComposerDraft('session:s-overflow', { text: 'overflow' });

    expect(getComposerDraft('session:s-1')).toBeUndefined();
    expect(getComposerDraft('session:s-0')?.text).toBe('draft-0-updated');
    expect(getComposerDraft('session:s-overflow')?.text).toBe('overflow');
  });
});
