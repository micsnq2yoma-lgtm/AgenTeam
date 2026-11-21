import { describe, expect, it, vi } from 'vitest';
import type { SessionCompletionTerminal } from '../../shared/sessionCompletion';
import { UNKNOWN_SESSION_ORIGIN } from '../../shared/session-origin';
import { createSessionActivityReporter } from './activity-reporter';
import type { SessionEngineLiveState } from './types';

describe('SessionEngine activity reporting', () => {
  it('pushes live and terminal adapter facts in order without polling', async () => {
    let live: SessionEngineLiveState = { isBusy: false, sessionState: 'idle' };
    let terminal: SessionCompletionTerminal | null = null;
    const send = vi.fn(async () => undefined);
    const notify = createSessionActivityReporter(() => ({
      getLiveSessionState: () => live,
      getSessionCompletionTerminal: () => terminal,
    }), send);

    async function publish(next: SessionEngineLiveState, expected: number) {
      live = next;
      notify();
      await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(expected));
    }

    await publish({ isBusy: true, sessionState: 'running' }, 1);
    await publish({ isBusy: true, sessionState: 'running', waitingForUser: true }, 2);
    await publish({ isBusy: true, sessionState: 'running' }, 3);
    terminal = {
      sessionId: 'session',
      workspacePath: '/fixture',
      turnId: 'turn',
      status: 'error',
      origin: UNKNOWN_SESSION_ORIGIN,
    };
    await publish({ isBusy: false, sessionState: 'error' }, 4);
    expect(send.mock.calls).toEqual([['running'], ['waiting_for_user'], ['running'], ['error']]);
  });

  it('keeps sending after the preceding report rejects', async () => {
    let waiting = false;
    const send = vi.fn<(_: 'waiting_for_user' | 'running' | 'idle' | 'complete' | 'stopped' | 'error') => Promise<void>>()
      .mockRejectedValueOnce(new Error('fixture result unavailable'))
      .mockResolvedValue(undefined);
    const notify = createSessionActivityReporter(() => ({
      getLiveSessionState: () => ({ isBusy: true, sessionState: 'running', waitingForUser: waiting }),
      getSessionCompletionTerminal: () => null,
    }), send);
    notify();
    await vi.waitFor(() => expect(send).toHaveBeenCalledWith('running'));
    waiting = true;
    notify();
    await vi.waitFor(() => expect(send).toHaveBeenLastCalledWith('waiting_for_user'));
  });
});
