import { describe, expect, it } from 'vitest';

import type { SessionMetadata } from '@/api/sessionClient';
import type { Project } from '@/config/types';
import type { SessionTag } from '@/hooks/useTaskCenterData';

import { characterFrame } from './pixel/characters';
import { layoutOffice, seatedLabels } from './pixel/office';
import { modelFamilyFor } from './pixel/types';
import {
  buildTeamOffice,
  employeeStatus,
  RESTING_AFTER_MS,
  STAFF_PER_PROJECT,
} from './teamOfficeModel';

const NOW = Date.parse('2026-11-18T12:00:00.000Z');

function project(overrides: Partial<Project> & Pick<Project, 'id' | 'path'>): Project {
  return { name: overrides.id, providerId: null, permissionMode: null, ...overrides };
}

function session(overrides: Partial<SessionMetadata> & Pick<SessionMetadata, 'id' | 'agentDir'>): SessionMetadata {
  return {
    title: overrides.id,
    createdAt: '2026-11-18T10:00:00.000Z',
    lastActiveAt: '2026-11-18T11:00:00.000Z',
    ...overrides,
  };
}

const tags = (entries: Array<[string, SessionTag[]]> = []) => new Map(entries);

describe('employeeStatus', () => {
  const s = session({ id: 's', agentDir: '/p' });

  it('reports what the conversation is actually doing, background work first', () => {
    expect(employeeStatus(s, [{ type: 'background' }, { type: 'cron' }], NOW)).toBe('working');
    expect(employeeStatus(s, [{ type: 'cron' }], NOW)).toBe('scheduled');
    expect(employeeStatus(s, [{ type: 'im', platform: 'feishu' }], NOW)).toBe('on-call');
    expect(employeeStatus(s, undefined, NOW)).toBe('idle');
  });

  it('rests employees untouched for a week', () => {
    const stale = session({
      id: 'old',
      agentDir: '/p',
      lastActiveAt: new Date(NOW - RESTING_AFTER_MS - 1).toISOString(),
    });
    expect(employeeStatus(stale, undefined, NOW)).toBe('resting');
  });
});

describe('buildTeamOffice', () => {
  it('turns projects into departments, the assistant into the lead and conversations into employees', () => {
    const assistant = project({ id: 'mino', path: '/home/mino', displayName: 'Mino', templateId: 'mino', model: 'claude-sonnet' });
    const plain = project({ id: 'app', path: '/work/app', displayName: 'App' });
    const office = buildTeamOffice({
      projects: [assistant, plain],
      sessions: [
        session({ id: 'a1', agentDir: '/home/mino', title: 'Plan trip' }),
        session({ id: 'b1', agentDir: '/work/app', title: 'Fix login', model: 'gpt-5.5', runtime: 'codex' }),
      ],
      sessionTags: tags([['b1', [{ type: 'background' }]]]),
      now: NOW,
    });

    expect(office.snapshot.departments.map((d) => d.name).sort()).toEqual(['App', 'Mino']);
    const lead = office.snapshot.people.find((p) => p.isLead);
    expect(lead).toMatchObject({ departmentId: 'mino', name: 'Mino', eye: 'claude', anim: 'idle' });
    expect(office.snapshot.people.filter((p) => p.departmentId === 'app' && p.isLead)).toHaveLength(0);

    const coder = office.snapshot.people.find((p) => p.agentId === 'session:b1');
    expect(coder).toMatchObject({ eye: 'codex', anim: 'working', bubble: 'Fix login' });
    expect(office.entries.get('session:b1')).toMatchObject({ kind: 'employee', status: 'working' });
    expect(office.counts).toEqual({ projects: 2, employees: 2, working: 1 });
  });

  it('lets the assistant type while one of its employees works', () => {
    const assistant = project({ id: 'mino', path: '/home/mino', workspaceType: 'system-preset', systemPresetId: 'mino' });
    const office = buildTeamOffice({
      projects: [assistant],
      sessions: [session({ id: 'a1', agentDir: '/home/mino' })],
      sessionTags: tags([['a1', [{ type: 'background' }]]]),
      now: NOW,
    });
    expect(office.snapshot.people.find((p) => p.isLead)?.anim).toBe('working');
  });

  it('seats only the most recent employees and reports the rest', () => {
    const app = project({ id: 'app', path: '/work/app' });
    const sessions = Array.from({ length: STAFF_PER_PROJECT + 2 }, (_, i) => session({
      id: `s${i}`,
      agentDir: '/work/app',
      lastActiveAt: new Date(NOW - i * 60_000).toISOString(),
    }));
    const office = buildTeamOffice({ projects: [app], sessions, sessionTags: tags(), now: NOW });

    const seated = office.snapshot.people.map((p) => p.agentId);
    expect(seated).toHaveLength(STAFF_PER_PROJECT);
    expect(seated[0]).toBe('session:s0');
    expect(office.hiddenStaff.get('app')).toBe(2);
  });

  it('skips product maintenance sessions', () => {
    const app = project({ id: 'app', path: '/work/app' });
    const office = buildTeamOffice({
      projects: [app],
      sessions: [session({ id: 'm', agentDir: '/work/app', systemMaintenanceKind: 'memory-update' as SessionMetadata['systemMaintenanceKind'] })],
      sessionTags: tags(),
      now: NOW,
    });
    expect(office.snapshot.people).toHaveLength(0);
  });

  it('places a label under every seated person', () => {
    const app = project({ id: 'app', path: '/work/app' });
    const office = buildTeamOffice({
      projects: [app],
      sessions: [session({ id: 'a', agentDir: '/work/app' }), session({ id: 'b', agentDir: '/work/app' })],
      sessionTags: tags(),
      now: NOW,
    });
    const labels = seatedLabels(office.snapshot, layoutOffice(office.snapshot));
    expect(labels.map((l) => l.agentId).sort()).toEqual(['session:a', 'session:b']);
    for (const label of labels) {
      expect(label.left).toBeGreaterThan(0);
      expect(label.left).toBeLessThan(1);
      expect(label.top).toBeLessThan(1);
    }
  });
});

describe('ported pixel assets', () => {
  it('recognises model families and draws every family in every action', () => {
    expect(modelFamilyFor(undefined, 'claude-opus-5')).toBe('claude');
    expect(modelFamilyFor('codex', undefined)).toBe('codex');
    expect(modelFamilyFor(undefined, 'deepseek-v4')).toBe('deepseek');
    for (const family of ['claude', 'codex', 'deepseek', 'gemini', 'kimi', 'glm', 'qwen', 'minimax', 'grok', 'other'] as const) {
      for (const action of ['idle', 'working', 'thinking', 'reading', 'paused'] as const) {
        expect(characterFrame(family, action, 1).length).toBeGreaterThan(0);
      }
    }
  });
});
