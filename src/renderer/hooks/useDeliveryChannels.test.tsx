import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useDeliveryChannels } from './useDeliveryChannels';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key,
  }),
}));

vi.mock('./useAgentStatuses', () => ({
  useAgentStatuses: () => ({
    loading: false,
    statuses: {
      'agent-1': {
        agentId: 'agent-1',
        agentName: 'atmer',
        enabled: false,
        channels: [{
          channelId: 'telegram-1',
          channelType: 'telegram',
          name: 'Telegram',
          status: 'online',
        }],
      },
    },
  }),
}));

vi.mock('./useConfig', () => ({
  useConfig: () => ({
    config: {
      agents: [{
        id: 'agent-1',
        name: 'atmer',
        enabled: false,
        permissionMode: 'auto',
        channels: [{ id: 'telegram-1', type: 'telegram', name: 'Telegram', enabled: true }],
      }],
    },
    projects: [{
      id: 'project-1',
      name: 'atmer',
      path: '/workspace/atmer',
      agentId: 'agent-1',
      providerId: null,
      permissionMode: null,
    }],
  }),
}));

describe('useDeliveryChannels', () => {
  it('keeps online Channels selectable while Proactive Agent is disabled', () => {
    const { result } = renderHook(() => useDeliveryChannels('/workspace/atmer'));

    expect(result.current.hasChannels).toBe(true);
    expect(result.current.options.some(option => option.value === 'telegram-1')).toBe(true);
    expect(result.current.resolveDelivery('telegram-1')).toEqual({
      botId: 'telegram-1',
      chatId: '_auto_',
      platform: 'telegram',
    });
  });
});
