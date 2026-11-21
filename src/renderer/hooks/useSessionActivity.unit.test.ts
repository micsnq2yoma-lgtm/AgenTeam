import { describe, expect, it, vi } from 'vitest';
import { createSessionActivityProjection } from './useSessionActivity';

describe('Session activity projection', () => {
  it('registers the listener before reading the initial snapshot and replays overlapping deltas', async () => {
    type Change = { sessionId: string; activity: 'running' | 'waiting_for_user' | 'error' | null };
    const registration = Promise.withResolvers<boolean>();
    const snapshot = Promise.withResolvers<Record<string, 'running' | 'waiting_for_user' | 'error'>>();
    let emit: (event: Change) => void = () => { throw new Error('Listener is not installed'); };
    const load = vi.fn(() => snapshot.promise);
    const projection = createSessionActivityProjection((change) => {
      emit = change;
      return registration.promise;
    }, load);
    const notify = vi.fn();
    const unsubscribe = projection.subscribe(notify);
    expect(load).not.toHaveBeenCalled();

    registration.resolve(true);
    await vi.waitFor(() => expect(load).toHaveBeenCalledOnce());
    emit({ sessionId: 'gone', activity: null });
    emit({ sessionId: 'wait', activity: 'running' });
    emit({ sessionId: 'wait', activity: 'waiting_for_user' });
    snapshot.resolve({ gone: 'running', wait: 'running', preserved: 'error' });
    await vi.waitFor(() => expect(projection.getSnapshot()).toEqual(
      new Map([['wait', 'waiting_for_user'], ['preserved', 'error']]),
    ));

    emit({ sessionId: 'wait', activity: 'running' });
    expect(projection.getSnapshot().get('wait')).toBe('running');
    unsubscribe();
    emit({ sessionId: 'wait', activity: 'error' });
    expect(projection.getSnapshot().size).toBe(0);
  });
});
