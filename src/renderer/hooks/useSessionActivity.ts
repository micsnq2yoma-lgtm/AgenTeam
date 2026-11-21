import { useSyncExternalStore } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { isTauriEnvironment } from '@/utils/browserMock';
import { listenWithCleanup } from '@/utils/tauriListen';

export type SessionActivity = 'running' | 'waiting_for_user' | 'error';

type ActivityChange = {
  sessionId: string;
  activity: SessionActivity | null;
};

type Snapshot = ReadonlyMap<string, SessionActivity>;

const EMPTY: Snapshot = new Map();

export function createSessionActivityProjection(
  listen: (change: (event: ActivityChange) => void, signal: AbortSignal) => Promise<boolean>,
  load: () => Promise<Record<string, SessionActivity>>,
) {
  const subscribers = new Set<() => void>();
  let snapshot = EMPTY;
  let controller: AbortController | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;

  function publish(next: Snapshot) {
    snapshot = next;
    for (const notify of subscribers) notify();
  }

  async function start() {
    const active = new AbortController();
    controller = active;
    const { signal } = active;
    let buffer: Map<string, SessionActivity | null> | undefined = new Map();
    try {
      const registered = await listen(({ sessionId, activity }) => {
        if (signal.aborted) return;
        if (buffer) {
          buffer.set(sessionId, activity);
          return;
        }
        if ((snapshot.get(sessionId) ?? null) === activity) return;
        const next = new Map(snapshot);
        if (activity === null) next.delete(sessionId);
        else next.set(sessionId, activity);
        publish(next);
      }, signal);
      if (signal.aborted) return;
      if (!registered) throw new Error('Session activity listener unavailable');
      const next = new Map(Object.entries(await load()));
      if (signal.aborted) return;
      for (const [id, activity] of buffer) {
        if (activity === null) next.delete(id);
        else next.set(id, activity);
      }
      buffer = undefined;
      publish(next);
    } catch (error) {
      if (signal.aborted) return;
      active.abort();
      console.warn('[session-activity] Could not establish the activity projection', error);
      retry = setTimeout(() => { void start(); }, 2_000);
    }
  }

  return {
    getSnapshot: () => snapshot,
    subscribe(notify: () => void) {
      subscribers.add(notify);
      if (subscribers.size === 1) void start();
      return () => {
        subscribers.delete(notify);
        if (subscribers.size !== 0) return;
        controller?.abort();
        clearTimeout(retry);
        snapshot = EMPTY;
      };
    },
  };
}

const projection = createSessionActivityProjection(
  async (change, signal) => {
    const result = await listenWithCleanup<ActivityChange>(
      'session:activity-changed', (event) => change(event.payload), signal,
    );
    return result.isRegistered();
  },
  () => invoke('cmd_session_activity_snapshot'),
);

const noopSubscribe = () => () => undefined;

export function useSessionActivity(): Snapshot {
  return useSyncExternalStore(
    isTauriEnvironment() ? projection.subscribe : noopSubscribe,
    projection.getSnapshot,
  );
}
