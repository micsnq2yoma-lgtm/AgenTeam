import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import LauncherInputContextRow from './LauncherInputContextRow';

vi.mock('./WorkspaceSelector', () => ({
  default: () => <button type="button">Workspace</button>,
}));

vi.mock('@/components/RuntimeSelector', () => ({
  default: () => <button type="button">Runtime</button>,
}));

const detections = {
  builtin: { installed: true },
  dsh: { installed: false },
  'claude-code': { installed: false },
  codex: { installed: false },
};

describe('LauncherInputContextRow', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('zh-CN');
  });

  it('labels both fields in English too', async () => {
    await i18n.changeLanguage('en-US');
    render(
      <LauncherInputContextRow
        projects={[]}
        selectedProject={null}
        onSelectWorkspace={vi.fn()}
        onAddFolder={vi.fn()}
        showRuntime
        runtime="builtin"
        runtimeDetections={detections}
        onRuntimeChange={vi.fn()}
      />,
    );

    expect(screen.getByRole('group', { name: 'Project' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Runtime' })).toBeInTheDocument();
  });

  it('renders as a labelled header row separated from the message body by a bottom hairline', () => {
    render(
      <LauncherInputContextRow
        projects={[]}
        selectedProject={null}
        onSelectWorkspace={vi.fn()}
        onAddFolder={vi.fn()}
        showRuntime={false}
      />,
    );

    const header = screen.getByTestId('launcher-context-header');
    expect(header).toHaveClass('border-b', 'border-[var(--line-subtle)]', 'px-4');
    // No floating-chip chrome from the previous below-input design.
    expect(header.innerHTML).not.toContain('shadow-xs');

    const projectGroup = screen.getByRole('group', { name: '项目' });
    expect(projectGroup).toHaveTextContent('项目');
    expect(projectGroup).toContainElement(screen.getByRole('button', { name: 'Workspace' }));
  });

  it('hides the runtime field and its divider when runtime selection is unavailable', () => {
    render(
      <LauncherInputContextRow
        projects={[]}
        selectedProject={null}
        onSelectWorkspace={vi.fn()}
        onAddFolder={vi.fn()}
        showRuntime={false}
        runtime="builtin"
        runtimeDetections={detections}
        onRuntimeChange={vi.fn()}
      />,
    );

    expect(screen.queryByRole('group', { name: '运行环境' })).not.toBeInTheDocument();
    expect(screen.queryByTestId('launcher-context-divider')).not.toBeInTheDocument();
  });

  it('separates project and runtime with a vertical hairline instead of a dot', () => {
    render(
      <LauncherInputContextRow
        projects={[]}
        selectedProject={null}
        onSelectWorkspace={vi.fn()}
        onAddFolder={vi.fn()}
        showRuntime
        runtime="builtin"
        runtimeDetections={detections}
        onRuntimeChange={vi.fn()}
      />,
    );

    const divider = screen.getByTestId('launcher-context-divider');
    expect(divider).toHaveAttribute('aria-hidden', 'true');
    expect(divider).toHaveClass('w-px', 'bg-[var(--line-strong)]');
    expect(screen.getByTestId('launcher-context-header')).not.toHaveTextContent('·');

    const runtimeGroup = screen.getByRole('group', { name: '运行环境' });
    expect(runtimeGroup).toHaveTextContent('运行环境');
    expect(runtimeGroup).toContainElement(screen.getByRole('button', { name: 'Runtime' }));
  });
});
