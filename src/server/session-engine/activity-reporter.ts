import type { SessionCompletionStatus } from '../../shared/sessionCompletion';
import { managementApi } from '../utils/management-api-client';
import { projectSessionActivity } from './observation';
import type { SessionEngine } from './types';

type ActivityReport = 'running' | 'waiting_for_user' | 'idle' | SessionCompletionStatus;

const activityToken = process.env.MYAGENTS_SESSION_ACTIVITY_TOKEN;
// This capability belongs to the host Sidecar, never to a Runtime child,
// MCP server or command launched by an Agent.
delete process.env.MYAGENTS_SESSION_ACTIVITY_TOKEN;

const TRANSITION_EVENTS = /^(?:chat:(?:status|message-complete|message-stopped|message-error)|(?:permission|ask-user-question|enter-plan-mode|exit-plan-mode):(?:request|expired))$/;

export function createSessionActivityReporter(
  getEngine: () => Pick<SessionEngine, 'getLiveSessionState' | 'getSessionCompletionTerminal'>,
  send: (activity: ActivityReport) => Promise<void>,
): () => void {
  let lastTerminalTurnId: string | undefined;
  let lastReport: ActivityReport = 'idle';
  let scheduled = false;
  let inFlight = Promise.resolve();

  return () => {
    // Runtime broadcasts and request-map mutations can have several
    // synchronous edges in one operation. Sample their settled live facts.
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      try {
        const engine = getEngine();
        const liveState = engine.getLiveSessionState();
        const live = projectSessionActivity(liveState);
        const terminal = engine.getSessionCompletionTerminal();
        let report: ActivityReport;
        if (live === 'waiting_user_action') report = 'waiting_for_user';
        else if (live === 'running') report = 'running';
        else if (terminal && terminal.turnId !== lastTerminalTurnId) {
          lastTerminalTurnId = terminal.turnId;
          report = terminal.status;
        } else {
          report = 'idle';
        }
        if (report === lastReport) return;
        lastReport = report;
        // Serialize reports so an older request cannot arrive after a newer
        // state for this same generation. Failure never gates a turn.
        inFlight = inFlight.then(() => send(report)).catch((error: unknown) => {
          console.warn('[session-activity] Could not report an activity transition', error);
        });
      } catch (error) {
        console.warn('[session-activity] Could not read Runtime activity', error);
      }
    });
  };
}

let sampleActivity: (() => void) | undefined;

export function startSessionActivityReporting(getEngine: () => SessionEngine): void {
  if (!activityToken) return;
  sampleActivity = createSessionActivityReporter(getEngine, async (activity) => {
    const result = await managementApi('/api/session/activity', 'POST', { activity }, {
      timeoutMs: 1_000,
      sessionActivityToken: activityToken,
    });
    if (result.ok !== true) console.warn('[session-activity] Rust did not accept the activity report');
  });
  sampleActivity();
}

/** The adapters feed their existing lifecycle edges into one reporter. */
export function notifySessionActivityTransition(event?: string): void {
  if (event === undefined || TRANSITION_EVENTS.test(event)) sampleActivity?.();
}
