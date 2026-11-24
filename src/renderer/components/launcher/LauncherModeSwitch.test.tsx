import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import LauncherModeSwitch from './LauncherModeSwitch';

describe('LauncherModeSwitch', () => {
  beforeEach(async () => { await i18n.changeLanguage('zh-CN'); });

  it('is a two-tab switch that marks the current mode', () => {
    render(<LauncherModeSwitch mode="chat" onChange={vi.fn()} />);
    expect(screen.getByRole('tablist', { name: '启动页模式' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '对话' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: '知识库' })).toHaveAttribute('aria-selected', 'false');
  });

  it('reports a change only when another mode is chosen', () => {
    const onChange = vi.fn();
    render(<LauncherModeSwitch mode="chat" onChange={onChange} />);
    fireEvent.click(screen.getByRole('tab', { name: '对话' }));
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('tab', { name: '知识库' }));
    expect(onChange).toHaveBeenCalledWith('knowledge');
  });

  it('is labelled in English too', async () => {
    await i18n.changeLanguage('en-US');
    render(<LauncherModeSwitch mode="knowledge" onChange={vi.fn()} />);
    expect(screen.getByRole('tab', { name: 'Knowledge' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: 'Chat' })).toBeInTheDocument();
  });
});
