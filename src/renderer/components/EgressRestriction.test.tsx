import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '@/i18n';
import { EgressRestrictionGate, EgressRestrictionSettings } from './EgressRestriction';

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  changed: undefined as undefined | (() => Promise<void>),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@/utils/tauriListen', () => ({
  listenWithCleanup: async (_event: string, onChange: () => Promise<void>) => { native.changed = onChange; },
}));

const disabled = { enabled: false, expectedIp: '', observedIp: null, state: 'disabled' };
const mismatched = { enabled: true, expectedIp: '203.0.113.9', observedIp: '198.51.100.7', state: 'mismatch' };
const matched = { ...mismatched, observedIp: mismatched.expectedIp, state: 'matched' };

beforeEach(async () => {
  native.invoke.mockReset();
  native.changed = undefined;
  await i18n.changeLanguage('en-US');
});
afterEach(cleanup);

describe('egress restriction window admission', () => {
  it('keeps business content unmounted through cold start, mismatch and a failed status read', async () => {
    let status = mismatched;
    native.invoke.mockImplementation(async () => status);
    render(<EgressRestrictionGate>Workspace</EgressRestrictionGate>);
    expect(screen.queryByText('Workspace')).toBeNull();
    await screen.findByRole('heading', { name: 'AgenTeam is locked' });
    expect(screen.getByRole('status').textContent).toContain('198.51.100.7');
    expect(screen.getByLabelText('Allowed public IP')).toHaveProperty('value', '203.0.113.9');
    status = matched;
    await act(async () => native.changed?.());
    expect(screen.getByText('Workspace')).toBeDefined();
    native.invoke.mockRejectedValue(new Error('IPC unavailable'));
    await act(async () => native.changed?.());
    expect(screen.queryByText('Workspace')).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('status could not be read');
    native.invoke.mockResolvedValue(disabled);
    fireEvent.click(screen.getByRole('button', { name: 'Check again' }));
    await screen.findByText('Workspace');
  });

  it('applies a typed target with the proxy off and supports disable from a mismatch', async () => {
    let current: typeof disabled | typeof mismatched = disabled;
    native.invoke.mockImplementation(async (command, args) => {
      if (command === 'cmd_set_egress_restriction') current = args.enabled ? mismatched : disabled;
      return current;
    });
    render(<EgressRestrictionGate>Workspace<EgressRestrictionSettings /></EgressRestrictionGate>);
    const input = await screen.findByRole('textbox', { name: 'Allowed public IP' });
    fireEvent.click(screen.getByRole('switch'));
    expect(screen.getByRole('alert').textContent).toContain('single public IPv4 or IPv6');
    fireEvent.change(input, { target: { value: ' 203.0.113.9 ' } });
    fireEvent.click(screen.getByRole('switch'));
    await screen.findByRole('heading', { name: 'AgenTeam is locked' });
    expect(native.invoke).toHaveBeenCalledWith('cmd_set_egress_restriction', { enabled: true, expectedIp: '203.0.113.9' });
    fireEvent.click(screen.getByRole('switch'));
    await screen.findByText('Workspace');
    expect(native.invoke).toHaveBeenCalledWith('cmd_set_egress_restriction', { enabled: false, expectedIp: '203.0.113.9' });
  });

  it('keeps a recovery-only window in recovery while the native shell is restarting', async () => {
    native.invoke.mockResolvedValue(matched);
    render(<EgressRestrictionGate recoveryOnly>Workspace</EgressRestrictionGate>);
    await screen.findByRole('heading', { name: 'Reopening AgenTeam…' });
    expect(screen.queryByText('Workspace')).toBeNull();
  });

  it('ignores a late allowed snapshot after a newer lock reaches an auxiliary window', async () => {
    let finishInitialRead!: (status: typeof matched) => void;
    native.invoke.mockImplementationOnce(() => new Promise(resolve => { finishInitialRead = resolve; }))
      .mockResolvedValue(mismatched);
    render(<EgressRestrictionGate auxiliary>Companion</EgressRestrictionGate>);
    await waitFor(() => expect(native.invoke).toHaveBeenCalled());
    await act(async () => native.changed?.());
    await act(async () => finishInitialRead(matched));
    expect(screen.queryByText('Companion')).toBeNull();
    expect(screen.queryByRole('textbox')).toBeNull();
  });

  it('keeps the editor draft during polling and reports native validation and probe failures', async () => {
    const noExit = { ...mismatched, state: 'unavailable', observedIp: null };
    native.invoke.mockImplementation(async command => {
      if (command === 'cmd_set_egress_restriction') throw 'invalid_expected_ip';
      return noExit;
    });
    render(<EgressRestrictionGate>Workspace</EgressRestrictionGate>);
    const input = await screen.findByRole('textbox', { name: 'Allowed public IP' });
    fireEvent.click(screen.getByRole('button', { name: 'Check again' }));
    await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('cmd_recheck_egress_restriction'));
    expect(screen.getByRole('status').textContent).toContain('could not be verified');
    fireEvent.change(input, { target: { value: 'invalid.test' } });
    await act(async () => native.changed?.());
    expect(input).toHaveProperty('value', 'invalid.test');
    fireEvent.click(screen.getByRole('button', { name: 'Save and check' }));
    expect((await screen.findByRole('alert')).textContent).toContain('single public IPv4 or IPv6');
  });
});
