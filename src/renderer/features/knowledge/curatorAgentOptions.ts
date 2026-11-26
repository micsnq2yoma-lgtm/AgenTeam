import { isProjectArchived, type Project } from '@/config/types';
import type { AgentConfig } from '../../../shared/types/agent';
import { resolveAgentWorkspaceProjections } from '../../../shared/agentWorkspaceIdentity';

export interface CuratorAgentOption {
  id: string;
  name: string;
  /** The Agent is still configured but no active Project backs it any more. */
  removed: boolean;
}

/**
 * Agents a knowledge base can use as its curator.
 *
 * `config.agents` keeps an Agent after its Project is removed, archived or hidden, so listing it
 * directly shows projects the user already got rid of. Only Agents with an active Project are
 * selectable. The currently selected Agent stays in the list, marked `removed`, so the user can see
 * and replace a stale choice instead of the field silently showing an empty value.
 */
export function curatorAgentOptions(
  projects: readonly Project[],
  agents: readonly AgentConfig[],
  selectedAgentId: string,
): CuratorAgentOption[] {
  const projections = resolveAgentWorkspaceProjections(projects, agents).agentProjections;
  const activeAgentIds = new Set(
    projections
      .filter((entry) => entry.project && !isProjectArchived(entry.project) && !entry.project.internal)
      .map((entry) => entry.agentId),
  );
  const options = agents
    .filter((agent) => activeAgentIds.has(agent.id))
    .map((agent) => ({ id: agent.id, name: agent.name, removed: false }));
  if (selectedAgentId && !activeAgentIds.has(selectedAgentId)) {
    const stale = agents.find((agent) => agent.id === selectedAgentId);
    options.push({ id: selectedAgentId, name: stale?.name ?? selectedAgentId, removed: true });
  }
  return options;
}
