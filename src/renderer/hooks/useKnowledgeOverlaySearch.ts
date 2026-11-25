import { useCallback, useEffect, useRef, useState } from 'react';

import {
    searchKnowledgeGlobal,
    type KnowledgeGlobalSearchHit,
} from '@/api/searchClient';

type KnowledgeOverlayStatus = 'idle' | 'searching' | 'indexing' | 'ready' | 'error';

interface KnowledgeOverlayState {
    status: KnowledgeOverlayStatus;
    hits: KnowledgeGlobalSearchHit[];
    total: number;
    hasMore: boolean;
    loadingMore: boolean;
    nextCursor: number | null;
    mode: string;
}

const EMPTY_STATE: KnowledgeOverlayState = {
    status: 'idle',
    hits: [],
    total: 0,
    hasMore: false,
    loadingMore: false,
    nextCursor: null,
    mode: 'bm25',
};

/**
 * K20 Knowledge category hook for the global search overlay.
 * Follows the exact same debounce, IME guard, generation cancellation,
 * and cursor pagination protocol as `useHistorySearch`.
 */
export function useKnowledgeOverlaySearch({
    query,
    enabled,
    composing,
}: {
    query: string;
    enabled: boolean;
    composing: boolean;
}) {
    const generation = useRef(0);
    const pagePending = useRef(false);
    const activeQuery = useRef('');
    const [state, setState] = useState<KnowledgeOverlayState>(EMPTY_STATE);
    const [submission, setSubmission] = useState(0);
    const consumedSubmission = useRef(0);

    useEffect(() => {
        let disposed = false;
        let timer: ReturnType<typeof setTimeout> | undefined;
        const trimmed = query.trim();

        const run = async () => {
            if (disposed) return;
            const myGen = ++generation.current;
            pagePending.current = false;
            activeQuery.current = trimmed;
            setState((prev) => ({
                ...prev,
                status: 'searching',
                hasMore: false,
                loadingMore: false,
                nextCursor: null,
            }));
            try {
                const res = await searchKnowledgeGlobal({ query: trimmed, limit: 20, cursor: 0 });
                if (disposed || generation.current !== myGen) return;
                setState({
                    status: 'ready',
                    hits: res.hits,
                    total: res.total,
                    hasMore: res.nextCursor !== null,
                    loadingMore: false,
                    nextCursor: res.nextCursor,
                    mode: res.mode,
                });
            } catch (error) {
                if (disposed || generation.current !== myGen) return;
                const preparing = String(error).includes('KB_INDEXING');
                setState((prev) => ({
                    ...prev,
                    status: preparing ? 'indexing' : 'error',
                    hasMore: false,
                    loadingMore: false,
                }));
                if (preparing) {
                    timer = setTimeout(() => {
                        void run();
                    }, 500);
                }
            }
        };

        const immediate = submission !== consumedSubmission.current;
        consumedSubmission.current = submission;
        if (!enabled || !trimmed) {
            generation.current += 1;
            pagePending.current = false;
            setState(EMPTY_STATE);
        } else {
            setState((prev) => ({
                ...prev,
                status: 'searching',
                hasMore: false,
                loadingMore: false,
            }));
            if (!composing) {
                if (immediate) void run();
                else {
                    timer = setTimeout(() => {
                        void run();
                    }, 200);
                }
            }
        }

        return () => {
            disposed = true;
            generation.current += 1;
            clearTimeout(timer);
        };
    }, [query, enabled, composing, submission]);

    const loadMore = useCallback(async () => {
        const myGen = generation.current;
        const cursor = state.nextCursor;
        const trimmed = activeQuery.current;
        if (pagePending.current || cursor === null || !trimmed) return;
        pagePending.current = true;
        setState((prev) => ({ ...prev, loadingMore: true }));
        try {
            const page = await searchKnowledgeGlobal({ query: trimmed, limit: 20, cursor });
            if (generation.current !== myGen) return;
            setState((prev) => {
                const known = new Set(prev.hits.map((h) => h.reference));
                const nextHits = [...prev.hits, ...page.hits.filter((h) => !known.has(h.reference))];
                return {
                    status: 'ready',
                    hits: nextHits,
                    total: page.total,
                    hasMore: page.nextCursor !== null,
                    loadingMore: false,
                    nextCursor: page.nextCursor,
                    mode: page.mode,
                };
            });
        } catch {
            if (generation.current !== myGen) return;
            setState((prev) => ({ ...prev, loadingMore: false }));
        } finally {
            pagePending.current = false;
        }
    }, [state.nextCursor]);

    const refresh = useCallback(() => setSubmission((v) => v + 1), []);

    return {
        ...state,
        loadMore,
        refresh,
    };
}
