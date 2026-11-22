// MEMORY_REFRESH tracking for the Session hosted by this Sidecar process.
//
// A Runtime loads CLAUDE.md and .claude/rules when it starts and does not
// reload them mid-Session. Memory flows (memory update, Gardener, Molt) and the
// user can rewrite those files while a long-lived Session (an IM Bot or a
// long-running desktop chat) keeps running, so the facade tells the Session
// once per change on its next Desktop, IM or Inbox input.
//
// The baseline is in-process state, not persisted: a new Sidecar process
// starts a new Runtime, which loads the current files, so the first
// observation after process start becomes the baseline without a notice.
// When a turn of this Session ends, the baseline absorbs the files as they are
// then: the Session already knows what it wrote itself (an assistant updating
// its own memory, or a memory-update turn injected into this Session). Changes
// written between turns by other Sessions, Gardener/Molt or the user are
// reported on the next admitted turn.
//
// Known limits, all on the side of a missed or extra hint rather than wrong
// content: a change between Runtime pre-warm and the first message, or one
// written by someone else while this Session's turn was running, is absorbed
// without a notice; a Runtime restart inside one Sidecar (config boundary)
// that already reloaded the files may still receive one redundant notice.

import { realpathSync } from 'fs';

import {
  diffInstructionFiles,
  type InstructionFile,
  type MemoryRefreshNotice,
} from '../../shared/memoryRefresh';
import { readWorkspaceInstructionSnapshot } from '../runtimes/workspace-instructions';

type SnapshotReader = (workspacePath: string) => InstructionFile[];

export interface MemoryRefreshObservation {
  notice: MemoryRefreshNotice | null;
  /** Mark this version as delivered. Call only after the turn was admitted. */
  commit(): void;
  /** The turn was not admitted; the next turn reports the change again. */
  release(): void;
}

const NOOP_OBSERVATION: MemoryRefreshObservation = { notice: null, commit: () => {}, release: () => {} };

export function createMemoryRefreshTracker(read: SnapshotReader = readWorkspaceInstructionSnapshot) {
  const baselines = new Map<string, InstructionFile[]>();
  const workspaces = new Map<string, string>();
  // A notice already riding on a turn that is being admitted. Concurrent
  // admissions skip it, so one change produces one notice.
  const inFlight = new Set<string>();

  const keyOf = (workspacePath: string): string => {
    try {
      return realpathSync(workspacePath);
    } catch {
      return workspacePath;
    }
  };

  return {
    observe(workspacePath: string | undefined): MemoryRefreshObservation {
      if (!workspacePath) return NOOP_OBSERVATION;
      const key = keyOf(workspacePath);
      if (inFlight.has(key)) return NOOP_OBSERVATION;
      let current: InstructionFile[];
      try {
        current = read(workspacePath);
      } catch (error) {
        // A hint must never block a user turn.
        console.warn('[memory-refresh] instruction snapshot failed:', error instanceof Error ? error.message : String(error));
        return NOOP_OBSERVATION;
      }
      const baseline = baselines.get(key);
      if (!baseline) {
        baselines.set(key, current);
        workspaces.set(key, workspacePath);
        return NOOP_OBSERVATION;
      }
      const notice = diffInstructionFiles(baseline, current);
      if (!notice) return NOOP_OBSERVATION;
      inFlight.add(key);
      let settled = false;
      return {
        notice,
        commit: () => {
          if (settled) return;
          settled = true;
          inFlight.delete(key);
          baselines.set(key, current);
        },
        release: () => {
          if (settled) return;
          settled = true;
          inFlight.delete(key);
        },
      };
    },
    /** A turn ended: adopt the current files, including what this Session wrote. */
    absorbTurnWrites(): void {
      for (const [key, workspacePath] of workspaces) {
        if (inFlight.has(key)) continue;
        try {
          baselines.set(key, read(workspacePath));
        } catch (error) {
          console.warn('[memory-refresh] instruction snapshot failed:', error instanceof Error ? error.message : String(error));
        }
      }
    },
  };
}

export type MemoryRefreshTracker = ReturnType<typeof createMemoryRefreshTracker>;

export const memoryRefreshTracker = createMemoryRefreshTracker();
