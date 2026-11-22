import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SessionCompletionTerminal } from '../../shared/sessionCompletion';
import { UNKNOWN_SESSION_ORIGIN } from '../../shared/session-origin';
import { createSessionActivityReporter } from './activity-reporter';
import type { SessionEngine, SessionEngineLiveState } from './types';

const management = vi.hoisted(() => vi.fn());
vi.mock('../utils/management-api-client', () => ({ managementApi: management }));

afterEach(() => {
  vi.unstubAllEnvs();
  management.mockReset();
});

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

  it('keeps an old queued terminal on the old proof after a conversation rebind', async () => {
    const oldToken = '11111111-1111-4111-8111-111111111111';
    const nextToken = '22222222-2222-4222-8222-222222222222';
    const unrelatedToken = '33333333-3333-4333-8333-333333333333';
    vi.stubEnv('MYAGENTS_SESSION_ACTIVITY_TOKEN', oldToken);
    vi.resetModules();
    const reporter = await import('./activity-reporter');
    const blocked = Promise.withResolvers<{ ok: boolean }>();
    management.mockImplementationOnce(() => blocked.promise).mockResolvedValue({ ok: true });

    let live: SessionEngineLiveState = { isBusy: true, sessionState: 'running' };
    let terminal: SessionCompletionTerminal | null = null;
    reporter.startSessionActivityReporting(() => ({
      getLiveSessionState: () => live,
      getSessionCompletionTerminal: () => terminal,
    } as SessionEngine));
    await vi.waitFor(() => expect(management).toHaveBeenCalledTimes(1));

    terminal = {
      sessionId: 'previous',
      workspacePath: '/fixture',
      turnId: 'previous-turn',
      status: 'error',
      origin: UNKNOWN_SESSION_ORIGIN,
    };
    live = { isBusy: false, sessionState: 'error' };
    reporter.notifySessionActivityTransition();
    // Let the old terminal sample capture the old proof while its send is blocked.
    await Promise.resolve();
    expect(reporter.beginSessionActivityRebind(unrelatedToken, nextToken)).toBeNull();
    const finish = reporter.beginSessionActivityRebind(oldToken, nextToken);
    expect(finish).not.toBeNull();
    live = { isBusy: false, sessionState: 'idle' };
    finish?.('committed');
    await Promise.resolve();

    live = { isBusy: true, sessionState: 'running', waitingForUser: true };
    reporter.notifySessionActivityTransition();
    await vi.waitFor(() => expect(management).toHaveBeenCalledTimes(2));
    blocked.resolve({ ok: true });
    await vi.waitFor(() => expect(management).toHaveBeenCalledTimes(3));
    expect(management.mock.calls.map(([, , body, options]) => [
      body.activity, options.sessionActivityToken,
    ])).toEqual([
      ['running', oldToken],
      ['waiting_for_user', nextToken],
      ['error', oldToken],
    ]);
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
