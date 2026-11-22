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

  it('labels the runtime picker 环境 and keeps the existing inline selector', () => {
    const onRuntimeChange = vi.fn();
    renderControl(onRuntimeChange);

    const group = screen.getByRole('group', { name: '环境' });
    expect(group).toHaveTextContent('环境');
    expect(group).not.toHaveTextContent('运行环境');
    expect(group).toContainElement(screen.getByRole('button', { name: 'Claude Agent SDK' }));
    expect(runtimeSelector.props).toMatchObject({
      value: 'builtin',
      detections,
      onChange: onRuntimeChange,
      variant: 'launcher',
    });
  });

  it('collapses its text label with the other toolbar labels when the composer is narrow', () => {
    renderControl();

    const label = screen.getByText('环境');
    expect(label).toHaveClass('toolbar-label', 'text-sm', 'text-[var(--ink-muted)]');
  });

  it('is labelled in English too', async () => {
    await i18n.changeLanguage('en-US');
    renderControl();

    expect(screen.getByRole('group', { name: 'Environment' })).toBeInTheDocument();
  });
});
