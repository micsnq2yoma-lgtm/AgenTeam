import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import LauncherRuntimeToolbarControl from './LauncherRuntimeToolbarControl';

const runtimeSelector = vi.hoisted(() => ({ props: null as Record<string, unknown> | null }));

vi.mock('@/components/RuntimeSelector', () => ({
  default: (props: Record<string, unknown>) => {
    runtimeSelector.props = props;
    return <button type="button">Claude Agent SDK</button>;
  },
}));

const detections = {
  builtin: { installed: true },
  dsh: { installed: false },
  'claude-code': { installed: false },
  codex: { installed: false },
};

function renderControl(onRuntimeChange = vi.fn()) {
  return render(
    <LauncherRuntimeToolbarControl
      runtime="builtin"
      runtimeDetections={detections}
      onRuntimeChange={onRuntimeChange}
    />,
  );
}

describe('LauncherRuntimeToolbarControl', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('zh-CN');
    runtimeSelector.props = null;
  });

  it('shows only the runtime icon and name, with 环境 as the accessible group name', () => {
    const onRuntimeChange = vi.fn();
    renderControl(onRuntimeChange);

    const group = screen.getByRole('group', { name: '环境' });
    expect(group).toHaveTextContent(/^Claude Agent SDK$/);
    expect(group).toContainElement(screen.getByRole('button', { name: 'Claude Agent SDK' }));
    expect(runtimeSelector.props).toMatchObject({
      value: 'builtin',
      detections,
      onChange: onRuntimeChange,
      variant: 'launcher',
    });
  });

  it('has an accessible name in English too', async () => {
    await i18n.changeLanguage('en-US');
    renderControl();

    expect(screen.getByRole('group', { name: 'Environment' })).toBeInTheDocument();
  });
});
