import { timingSafeEqual } from 'node:crypto';
import type { SessionCompletionStatus } from '../../shared/sessionCompletion';
import { managementApi } from '../utils/management-api-client';
import { projectSessionActivity } from './observation';
import { memoryRefreshTracker } from './memory-refresh';
import type { SessionEngine } from './types';

type ActivityReport = 'running' | 'waiting_for_user' | 'idle' | SessionCompletionStatus;

let activityToken = process.env.MYAGENTS_SESSION_ACTIVITY_TOKEN;
// This capability belongs to the host Sidecar, never to a Runtime child,
// MCP server or command launched by an Agent.
delete process.env.MYAGENTS_SESSION_ACTIVITY_TOKEN;

const TRANSITION_EVENTS = /^(?:chat:(?:status|message-complete|message-stopped|message-error)|(?:permission|ask-user-question|enter-plan-mode|exit-plan-mode):(?:request|expired))$/;

export function createSessionActivityReporter(
  getEngine: () => Pick<SessionEngine, 'getLiveSessionState' | 'getSessionCompletionTerminal'>,
  send: (activity: ActivityReport) => Promise<void>,
  options?: { canSample: () => boolean; previousTerminalId?: string },
): () => void {
  let lastTerminalTurnId = options?.previousTerminalId;
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
      if (options && !options.canSample()) return;
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
let getReportingEngine: (() => SessionEngine) | undefined;
let transitionInProgress = false;

function installActivityReporter(
  getEngine: () => SessionEngine,
  token: string,
  previousTerminalId?: string,
): void {
  sampleActivity = createSessionActivityReporter(getEngine, async (activity) => {
    // Queued reports retain the credential active when they were sampled.
    const result = await managementApi('/api/session/activity', 'POST', { activity }, {
      timeoutMs: 1_000,
      sessionActivityToken: token,
    });
    if (result.ok !== true) console.warn('[session-activity] Rust did not accept the activity report');
  }, {
    canSample: () => !transitionInProgress && activityToken === token,
    previousTerminalId,
  });
  sampleActivity();
}

export function startSessionActivityReporting(getEngine: () => SessionEngine): void {
  if (!activityToken) return;
  getReportingEngine = getEngine;
  installActivityReporter(getEngine, activityToken);
}

type RebindOutcome = 'committed' | 'unchanged' | 'uncertain';
const TOKEN_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

/** Rust sends its current proof and the replacement in the existing request. */
export function beginSessionActivityRebind(
  current: string | null,
  next: unknown,
): ((outcome: RebindOutcome) => void) | null {
  if (!activityToken) return () => undefined;
  if (transitionInProgress || !TOKEN_PATTERN.test(activityToken) || !current
    || !TOKEN_PATTERN.test(current) || typeof next !== 'string' || !TOKEN_PATTERN.test(next)
    || current === next || !timingSafeEqual(Buffer.from(activityToken), Buffer.from(current))) {
    return null;
  }

  transitionInProgress = true;
  return (outcome) => {
    // The Rust lifecycle owner retires an uncertain writer. Keep it muted.
    if (outcome === 'uncertain') return;
    transitionInProgress = false;
    if (outcome === 'unchanged') {
      sampleActivity?.();
      return;
    }

    activityToken = next;
    const getEngine = getReportingEngine;
    if (!getEngine) return;
    let previousTerminalId: string | undefined;
    try {
      // Reset can leave an earlier terminal available on the same adapter.
      previousTerminalId = getEngine().getSessionCompletionTerminal()?.turnId;
    } catch (error) {
      console.warn('[session-activity] Could not read terminal at Session rebind', error);
    }
    installActivityReporter(getEngine, next, previousTerminalId);
  };
}

const TURN_END_EVENTS = /^chat:message-(?:complete|stopped|error)$/;

/** The adapters feed their existing lifecycle edges into one reporter. */
export function notifySessionActivityTransition(event?: string): void {
  if (event !== undefined && TURN_END_EVENTS.test(event)) memoryRefreshTracker.absorbTurnWrites();
  if (event === undefined || TRANSITION_EVENTS.test(event)) sampleActivity?.();
}
