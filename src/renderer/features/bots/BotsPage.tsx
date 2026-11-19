/**
 * 聊天机器人 page, opened from the global sidebar. Lists the IM platforms an
 * Agent can be reached through and adds one to a project by opening that
 * project's Agent config on the Channel step. It owns the flow end to end —
 * it used to be a Settings section borrowing Settings' overlay state.
 */

import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import AgentIdentityConflicts from '@/components/AgentSettings/AgentIdentityConflicts';
import { BotPlatformRegistry } from '@/components/ImSettings';
import { getBotWorkspaceCandidates } from '@/components/ImSettings/botWorkspaceSelection';
import { useToast } from '@/components/Toast';
import WorkspaceConfigPanel from '@/components/WorkspaceConfigPanel';
import type { Project } from '@/config/types';
import { reconcilePersistedAgentWorkspaceIdentities } from '@/config/services/agentConfigService';
import { useConfig } from '@/hooks/useConfig';

import type { ChannelType } from '../../../shared/types/agent';

interface ChannelSetupTarget {
  workspacePath: string;
  initialAddChannelPlatform?: ChannelType;
}

export default function BotsPage() {
  const { t } = useTranslation('settings');
  const { config, projects, refreshConfig } = useConfig();
  const toast = useToast();
  const toastRef = useRef(toast);
  toastRef.current = toast;
  const tRef = useRef(t);
  tRef.current = t;
  const [target, setTarget] = useState<ChannelSetupTarget | null>(null);

  const handleAddBotToWorkspace = useCallback(async (platform: ChannelType, selectedProject: Project) => {
    try {
      const identity = await reconcilePersistedAgentWorkspaceIdentities();
      const project = getBotWorkspaceCandidates(identity.projects, config.defaultWorkspacePath)
        .find((candidate) => candidate.id === selectedProject.id);
      if (!project) throw new Error(`Project '${selectedProject.id}' is no longer available.`);
      if (!identity.agentProjections.some((item) => item.projectId === project.id)) {
        throw new Error(`Agent identity is unavailable for Project '${project.id}'.`);
      }
      await refreshConfig();
      setTarget({ workspacePath: project.path, initialAddChannelPlatform: platform });
    } catch (error) {
      console.error('[Bots] Failed to prepare workspace for Channel setup:', error);
      toastRef.current.error(tRef.current('agentSettings.botRegistry.openWorkspaceFailed'));
    }
  }, [config.defaultWorkspacePath, refreshConfig]);

  const handleInitialPlatformConsumed = useCallback(() => {
    setTarget((current) => (current ? { workspacePath: current.workspacePath } : current));
  }, []);

  return (
    <div className="h-full overflow-y-auto overscroll-contain" data-bots-page>
      <div className="mx-auto max-w-4xl px-8 pb-8 pt-14">
        <AgentIdentityConflicts />
        <BotPlatformRegistry
          projects={projects}
          defaultWorkspacePath={config.defaultWorkspacePath}
          onAddToWorkspace={handleAddBotToWorkspace}
        />
      </div>
      {target && (
        <WorkspaceConfigPanel
          agentDir={target.workspacePath}
          onClose={() => setTarget(null)}
          initialTab="agent"
          initialAddChannelPlatform={target.initialAddChannelPlatform}
          onInitialAddChannelPlatformConsumed={handleInitialPlatformConsumed}
        />
      )}
    </div>
  );
}
