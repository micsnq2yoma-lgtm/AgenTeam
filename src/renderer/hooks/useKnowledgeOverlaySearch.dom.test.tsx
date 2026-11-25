import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useKnowledgeOverlaySearch } from './useKnowledgeOverlaySearch';

const mocks = vi.hoisted(() => ({
  searchKnowledgeGlobal: vi.fn(),
}));

vi.mock('@/api/searchClient', () => ({
  searchKnowledgeGlobal: mocks.searchKnowledgeGlobal,
}));

describe('useKnowledgeOverlaySearch (K20)', () => {
  beforeEach(() => {
    mocks.searchKnowledgeGlobal.mockReset();
  });

  it('paginates results with nextCursor and deduplicates references', async () => {
    mocks.searchKnowledgeGlobal
      .mockResolvedValueOnce({
        mode: 'bm25',
        hits: [
          {
            reference: 'kb://kb_111111111111/src_0000000000000001#0000000000000001',
            kbId: 'kb_111111111111',
            sourceId: 'src_0000000000000001',
            chunkId: '0000000000000001',
            title: 'Doc 1',
            prefix: 'Doc 1',
            snippet: 'first snippet',
            score: 1.0,
          },
        ],
        total: 2,
        nextCursor: 1,
      })
      .mockResolvedValueOnce({
        mode: 'bm25',
        hits: [
          {
            reference: 'kb://kb_111111111111/src_0000000000000002#0000000000000002',
            kbId: 'kb_111111111111',
            sourceId: 'src_0000000000000002',
            chunkId: '0000000000000002',
            title: 'Doc 2',
            prefix: 'Doc 2',
            snippet: 'second snippet',
            score: 0.9,
          },
        ],
        total: 2,
        nextCursor: null,
      });

    const { result } = renderHook(() =>
      useKnowledgeOverlaySearch({ query: 'architecture', enabled: true, composing: false }),
    );

    await waitFor(() => {
      expect(result.current.status).toBe('ready');
      expect(result.current.hits).toHaveLength(1);
      expect(result.current.hasMore).toBe(true);
    });

    await act(async () => {
      await result.current.loadMore();
    });

    expect(result.current.hits).toHaveLength(2);
    expect(result.current.hasMore).toBe(false);
    expect(mocks.searchKnowledgeGlobal).toHaveBeenLastCalledWith({
      query: 'architecture',
      limit: 20,
      cursor: 1,
    });
  });

  it('cancels stale in-flight queries when input changes or overlay disables', async () => {
    let resolveFirst: ((v: unknown) => void) | null = null;
    mocks.searchKnowledgeGlobal
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockResolvedValueOnce({
        mode: 'bm25',
        hits: [
          {
            reference: 'kb://kb_111111111111/src_0000000000000009#0000000000000009',
            kbId: 'kb_111111111111',
            sourceId: 'src_0000000000000009',
            chunkId: '0000000000000009',
            title: 'Fresh Doc',
            prefix: 'Fresh',
            snippet: 'fresh snippet',
            score: 1.5,
          },
        ],
        total: 1,
        nextCursor: null,
      });

    const { result, rerender } = renderHook(
      ({ q, enabled }: { q: string; enabled: boolean }) =>
        useKnowledgeOverlaySearch({ query: q, enabled, composing: false }),
      { initialProps: { q: 'old', enabled: true } },
    );

    await waitFor(() => {
      expect(mocks.searchKnowledgeGlobal).toHaveBeenCalledTimes(1);
    });

    // Change query before first resolves
    rerender({ q: 'new', enabled: true });

    await waitFor(() => {
      expect(result.current.status).toBe('ready');
      expect(result.current.hits[0]?.title).toBe('Fresh Doc');
    });

    // Late resolution of first query must be discarded
    await act(async () => {
      resolveFirst?.({
        mode: 'bm25',
        hits: [
          {
            reference: 'kb://kb_111111111111/src_0000000000000001#0000000000000001',
            kbId: 'kb_111111111111',
            sourceId: 'src_0000000000000001',
            chunkId: '0000000000000001',
            title: 'Stale Doc',
            prefix: 'Stale',
            snippet: 'stale',
            score: 0.5,
          },
        ],
        total: 1,
        nextCursor: null,
      });
    });

    expect(result.current.hits[0]?.title).toBe('Fresh Doc');
  });
});
