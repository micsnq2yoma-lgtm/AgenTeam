import { describe, expect, it } from 'vitest';

import type { Project } from '@/config/types';
import type { AgentConfig } from '../../../shared/types/agent';
import { curatorAgentOptions } from './curatorAgentOptions';

const project = (id: string, agentId: string, extra: Partial<Project> = {}): Project => ({
  id, name: id, path: `/work/${id}`, agentId, providerId: null, permissionMode: null, ...extra,
});
const agent = (id: string, name: string): AgentConfig => ({ id, name, enabled: true } as AgentConfig);

describe('curatorAgentOptions', () => {
  const agents = [agent('a1', 'Active'), agent('a2', 'Removed project'), agent('a3', 'Archived'), agent('a4', 'Internal')];
  const projects = [
    project('p1', 'a1'),
    project('p3', 'a3', { archivedAt: '2026-11-01T00:00:00.000Z' }),
    project('p4', 'a4', { internal: true }),
  ];

  it('lists only Agents backed by an active Project', () => {
    expect(curatorAgentOptions(projects, agents, '').map((option) => option.id)).toEqual(['a1']);
  });

  it('keeps a stale selection visible and marked removed', () => {
    expect(curatorAgentOptions(projects, agents, 'a2')).toEqual([
      { id: 'a1', name: 'Active', removed: false },
      { id: 'a2', name: 'Removed project', removed: true },
    ]);
  });

  it('falls back to the id when the selected Agent no longer exists at all', () => {
    expect(curatorAgentOptions(projects, agents, 'gone').at(-1)).toEqual({ id: 'gone', name: 'gone', removed: true });
  });

  it('does not duplicate an active selection', () => {
    expect(curatorAgentOptions(projects, agents, 'a1')).toEqual([{ id: 'a1', name: 'Active', removed: false }]);
  });
});
