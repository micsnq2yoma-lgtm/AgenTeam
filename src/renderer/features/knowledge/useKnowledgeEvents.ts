import { useEffect, useLayoutEffect, useRef } from 'react';

import { isTauriEnvironment } from '@/utils/browserMock';
import { listenWithCleanup } from '@/utils/tauriListen';

import type { KnowledgeChangeEvent } from './types';

export const KNOWLEDGE_CHANGED_EVENT = 'knowledge:changed';

/**
 * Subscribe to Rust Store invalidation events and coalesce bursts before triggering
 * a refetch. Never holds authoritative state in the renderer.
 */
export function useKnowledgeChangeSubscription(
  onChanged: (event: KnowledgeChangeEvent) => void,
  filterKbId?: string,
): void {
  const callbackRef = useRef(onChanged);
  useLayoutEffect(() => {
    callbackRef.current = onChanged;
  }, [onChanged]);

  useEffect(() => {
    if (!isTauriEnvironment()) return;
    const ac = new AbortController();
    let timer: ReturnType<typeof setTimeout> | null = null;
    let latestEvent: KnowledgeChangeEvent | null = null;

    void listenWithCleanup<KnowledgeChangeEvent>(
      KNOWLEDGE_CHANGED_EVENT,
      ({ payload }) => {
        if (filterKbId && payload.kind !== 'resync' && payload.kbId !== filterKbId) {
          return;
        }
        latestEvent = payload;
        if (timer !== null) return;
        timer = setTimeout(() => {
          timer = null;
          if (ac.signal.aborted || !latestEvent) return;
          const event = latestEvent;
          latestEvent = null;
          callbackRef.current(event);
        }, 120);
      },
      ac.signal,
    );

    return () => {
      ac.abort();
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
    };
  }, [filterKbId]);
}
