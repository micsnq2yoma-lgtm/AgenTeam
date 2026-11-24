import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastProvider } from '@/components/Toast';
import { i18n } from '@/i18n';

import AgentKnowledgeSection from './AgentKnowledgeSection';
import type { AgentConfig } from '../../../../shared/types/agent';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  patchAgentConfig: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@/config/services/agentConfigService', () => ({
  patchAgentConfig: mocks.patchAgentConfig,
}));

const makeAgent = (knowledgeBaseIds?: string[]): AgentConfig => ({
  id: 'agent_1',
  name: 'Researcher',
  enabled: false,
  channels: [],
  knowledgeBaseIds,
});

describe('AgentKnowledgeSection', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage('zh-CN');
    mocks.invoke.mockResolvedValue([
      {
        id: 'kb_111111111111',
        name: '产品资料',
        description: '设计文档',
        curationMode: 'manual',
        curationIntervalMinutes: 4320,
        agentWriteAccess: 'curator-only',
        wikiEnabled: true,
        pendingCategories: [],
      },
    ]);
    mocks.patchAgentConfig.mockResolvedValue(undefined);
  });

  it('toggles Agent default knowledgeBaseIds through patchAgentConfig', async () => {
    const onAgentChanged = vi.fn();
    render(
      <ToastProvider>
        <AgentKnowledgeSection agent={makeAgent([])} onAgentChanged={onAgentChanged} />
      </ToastProvider>,
    );

    const toggle = await screen.findByRole('switch', { name: '产品资料' });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(toggle);

    await waitFor(() =>
      expect(mocks.patchAgentConfig).toHaveBeenCalledWith('agent_1', {
        knowledgeBaseIds: ['kb_111111111111'],
      }),
    );
    expect(onAgentChanged).toHaveBeenCalled();
  });
});
