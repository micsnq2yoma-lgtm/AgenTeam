import { describe, expect, it } from 'vitest';

import { createMemoryRefreshTracker } from './memory-refresh';

function trackerWith(files: Record<string, string>) {
  const state = { files, fail: false };
  const tracker = createMemoryRefreshTracker(() => {
    if (state.fail) throw new Error('EACCES');
    return Object.entries(state.files).map(([path, content]) => ({ path, content }));
  });
  return { state, tracker };
}

const WORKSPACE = '/nonexistent/memory-refresh-workspace';

describe('memory refresh tracker', () => {
  it('adopts the first observation as the baseline without a notice', () => {
    const { tracker } = trackerWith({ 'CLAUDE.md': '## A\nx' });
    expect(tracker.observe(WORKSPACE).notice).toBeNull();
    expect(tracker.observe(WORKSPACE).notice).toBeNull();
    expect(tracker.observe(undefined).notice).toBeNull();
  });

  it('reports a change once after the turn carrying it is admitted', () => {
    const { state, tracker } = trackerWith({ '.claude/rules/04-MEMORY.md': '## Prefs\n- a' });
    tracker.observe(WORKSPACE);
    state.files['.claude/rules/04-MEMORY.md'] = '## Prefs\n- b';
    const first = tracker.observe(WORKSPACE);
    expect(first.notice?.files[0]).toMatchObject({ change: 'updated', sections: ['Prefs'] });
    first.commit();
    expect(tracker.observe(WORKSPACE).notice).toBeNull();
  });

  it('gives a concurrent turn no duplicate and repeats the notice after a rejected admission', () => {
    const { state, tracker } = trackerWith({ 'CLAUDE.md': 'v1' });
    tracker.observe(WORKSPACE);
    state.files['CLAUDE.md'] = 'v2';
    const first = tracker.observe(WORKSPACE);
    expect(first.notice).not.toBeNull();
    expect(tracker.observe(WORKSPACE).notice).toBeNull();
    first.release();
    const retry = tracker.observe(WORKSPACE);
    expect(retry.notice).not.toBeNull();
    retry.commit();
    retry.release();
    expect(tracker.observe(WORKSPACE).notice).toBeNull();
  });

  it('absorbs what the Session wrote during its own turn', () => {
    const { state, tracker } = trackerWith({ '.claude/rules/03-USER.md': '## Context\n-' });
    tracker.observe(WORKSPACE);
    state.files['.claude/rules/03-USER.md'] = '## Context\n- writes code in Rust';
    tracker.absorbTurnWrites();
    expect(tracker.observe(WORKSPACE).notice).toBeNull();
  });

  it('does not absorb over a notice that is being delivered', () => {
    const { state, tracker } = trackerWith({ 'CLAUDE.md': 'v1' });
    tracker.observe(WORKSPACE);
    state.files['CLAUDE.md'] = 'v2';
    const pending = tracker.observe(WORKSPACE);
    tracker.absorbTurnWrites();
    pending.release();
    expect(tracker.observe(WORKSPACE).notice).not.toBeNull();
  });

  it('never blocks a turn when the files cannot be read', () => {
    const { state, tracker } = trackerWith({ 'CLAUDE.md': 'v1' });
    tracker.observe(WORKSPACE);
    state.fail = true;
    expect(tracker.observe(WORKSPACE).notice).toBeNull();
    expect(() => tracker.absorbTurnWrites()).not.toThrow();
  });
});
