import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { ToastProvider } from '@/components/Toast';
import type { AppConfig, Provider } from '@/config/types';
import { useAvailableProviders } from '@/hooks/useAvailableProviders';
import AgentFunctionSettings from './AgentFunctionSettings';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@/hooks/useAvailableProviders', () => ({ useAvailableProviders: vi.fn() }));

describe('Agent function settings', () => {
  beforeEach(() => {
    vi.mocked(useAvailableProviders).mockReturnValue([]);
    vi.mocked(invoke).mockResolvedValue({
      builtin: { installed: true }, dsh: { installed: true },
      antigravity: { installed: true },
      'claude-code': { installed: true }, codex: { installed: true },
    });
  });

  it('groups default environment before queue mode and offers only the two Integrated runtimes', async () => {
    const user = userEvent.setup();
    const updateConfig = vi.fn().mockResolvedValue(undefined);
    render(<AgentFunctionSettings config={{}} updateConfig={updateConfig} />, { wrapper: ToastProvider });
    expect(screen.getByText('Agent 功能设置')).toBeInTheDocument();
    expect(screen.getByText('默认 Agent 运行环境').compareDocumentPosition(screen.getByText('队列响应模式')))
      .toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.getByText('修改后默认以此运行环境运行，可在 Agent 设置中单独修改')).toBeInTheDocument();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('cmd_detect_runtimes'));
    expect(updateConfig).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'AgenTeam (Claude Agent SDK)' }));
    expect(screen.queryByText('Codex CLI')).not.toBeInTheDocument();
    expect(screen.queryByText('Claude Code CLI')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /AgenTeam \(DeepSeek Harness\)/ }));
    expect(updateConfig).toHaveBeenCalledExactlyOnceWith({ defaultIntegratedRuntime: 'dsh' });
  });

  it('keeps the queue control independent of the default runtime', async () => {
    const user = userEvent.setup();
    const updateConfig = vi.fn().mockResolvedValue(undefined);
    render(<AgentFunctionSettings config={{ defaultIntegratedRuntime: 'dsh', chatQueueResponseMode: 'realtime' }} updateConfig={updateConfig} />, { wrapper: ToastProvider });
    expect(screen.getByRole('button', { name: 'AgenTeam (DeepSeek Harness)' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '轮次响应' }));
    expect(updateConfig).toHaveBeenCalledExactlyOnceWith({ chatQueueResponseMode: 'turn' });
  });

  it('defaults to full autonomy and saves the selected mode through app config', async () => {
    const user = userEvent.setup();
    const updateConfig = vi.fn();
    function Subject() {
      const [config, setConfig] = useState<Partial<AppConfig>>({});
      return <AgentFunctionSettings config={config} updateConfig={async patch => {
        updateConfig(patch);
        setConfig(current => ({ ...current, ...patch }));
      }} />;
    }
    render(<Subject />, { wrapper: ToastProvider });
    const trigger = screen.getByRole('button', { name: '默认会话模式' });
    expect(trigger).toHaveTextContent('完全自主');
    expect(updateConfig).not.toHaveBeenCalled();
    await user.click(trigger);
    await user.click(await screen.findByRole('button', { name: '请求批准' }));
    expect(updateConfig).toHaveBeenLastCalledWith({ defaultPermissionMode: 'auto' });
    expect(trigger).toHaveTextContent('请求批准');
    await user.click(trigger);
    await user.click(await screen.findByRole('button', { name: '规划模式' }));
    expect(updateConfig).toHaveBeenLastCalledWith({ defaultPermissionMode: 'plan' });
    expect(trigger).toHaveTextContent('规划模式');
  });

  it('shows full default model names and saves or clears the provider and model together', async () => {
    const providerName = '词元跳动 Token Dance';
    const modelName = 'DeepSeek-LongContext-Experimental-Model-With-A-Long-Identifier';
    const fullLabel = `${providerName} · ${modelName}`;
    vi.mocked(useAvailableProviders).mockReturnValue([{
      id: 'api', name: providerName, models: [{ model: 'agent-model', modelName }],
    } as Provider]);
    const user = userEvent.setup();
    const updateConfig = vi.fn();
    function Subject() {
      const [config, setConfig] = useState<Partial<AppConfig>>({});
      return <AgentFunctionSettings config={config} updateConfig={async patch => {
        updateConfig(patch);
        setConfig(current => ({ ...current, ...patch }));
      }} />;
    }
    render(<Subject />, { wrapper: ToastProvider });
    const trigger = screen.getByRole('button', { name: '默认模型' });
    expect(trigger).toHaveTextContent('自动选择');
    await user.click(trigger);
    const option = await screen.findByRole('button', { name: fullLabel });
    expect(option).toHaveAttribute('title', fullLabel);
    expect(screen.getByText(fullLabel)).toHaveClass('whitespace-normal', 'wrap-anywhere');
    await user.click(option);
    expect(updateConfig).toHaveBeenLastCalledWith({ defaultModelSelection: { providerId: 'api', model: 'agent-model' } });
    expect(trigger).toHaveTextContent(fullLabel);
    expect(trigger).toHaveAttribute('title', fullLabel);
    await user.click(trigger);
    await user.click(await screen.findByRole('button', { name: '自动选择' }));
    expect(updateConfig).toHaveBeenLastCalledWith({ defaultModelSelection: undefined });
  });
});
