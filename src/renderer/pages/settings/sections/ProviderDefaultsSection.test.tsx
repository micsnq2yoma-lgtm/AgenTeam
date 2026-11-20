// @vitest-environment jsdom
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Provider } from '@/config/types';
import { resolveProviderDefaultReasoningEffort } from '@/../shared/reasoningEffort';
import { i18n } from '@/i18n';

import { ProviderDefaultsSection } from './ProviderDefaultsSection';

const provider = {
  id: 'deepseek',
  name: 'DeepSeek',
  vendor: 'DeepSeek',
  cloudProvider: '模型官方',
  type: 'api',
  primaryModel: 'deepseek-chat',
  isBuiltin: true,
  config: { baseUrl: 'https://api.deepseek.com/anthropic' },
  models: [
    { model: 'deepseek-chat', modelName: 'DeepSeek Chat' },
    { model: 'deepseek-reasoner', modelName: 'DeepSeek Reasoner' },
  ],
} as Provider;

function renderSection(overrides: Partial<React.ComponentProps<typeof ProviderDefaultsSection>> = {}) {
  const props = {
    provider,
    isDefaultProvider: false,
    defaultModel: 'deepseek-chat',
    defaultEffort: undefined,
    onDefaultProviderChange: vi.fn(),
    onDefaultModelChange: vi.fn(),
    onDefaultEffortChange: vi.fn(),
    ...overrides,
  };
  render(<ProviderDefaultsSection {...props} />);
  return props;
}

describe('ProviderDefaultsSection', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('zh-CN');
  });

  it('shows the current defaults and reports each change', () => {
    const props = renderSection();
    expect(screen.getByText('自定义默认设置')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'DeepSeek Chat' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('radio', { name: '跟随服务商' })).toHaveAttribute('aria-checked', 'true');

    fireEvent.click(screen.getByRole('switch', { name: '设为默认供应商' }));
    expect(props.onDefaultProviderChange).toHaveBeenCalledWith(true);

    fireEvent.click(screen.getByRole('radio', { name: 'DeepSeek Reasoner' }));
    expect(props.onDefaultModelChange).toHaveBeenCalledWith('deepseek-reasoner');

    fireEvent.click(screen.getByRole('radio', { name: 'high' }));
    expect(props.onDefaultEffortChange).toHaveBeenCalledWith('high');
  });

  it('marks a saved effort and an existing default provider', () => {
    renderSection({ isDefaultProvider: true, defaultEffort: 'medium' });
    expect(screen.getByRole('switch', { name: '设为默认供应商' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('radio', { name: 'medium' })).toHaveAttribute('aria-checked', 'true');
  });
});

describe('resolveProviderDefaultReasoningEffort', () => {
  it('returns the saved effort and treats default/absent as following the provider', () => {
    const efforts = { deepseek: 'high', zhipu: 'default', blank: ' ' };
    expect(resolveProviderDefaultReasoningEffort(efforts, 'deepseek')).toBe('high');
    expect(resolveProviderDefaultReasoningEffort(efforts, 'zhipu')).toBeUndefined();
    expect(resolveProviderDefaultReasoningEffort(efforts, 'blank')).toBeUndefined();
    expect(resolveProviderDefaultReasoningEffort(efforts, 'missing')).toBeUndefined();
    expect(resolveProviderDefaultReasoningEffort(undefined, 'deepseek')).toBeUndefined();
    expect(resolveProviderDefaultReasoningEffort(efforts, undefined)).toBeUndefined();
  });
});
