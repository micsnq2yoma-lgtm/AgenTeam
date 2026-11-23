import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import LauncherInputContextRow from './LauncherInputContextRow';

vi.mock('./WorkspaceSelector', () => ({
  default: () => <button type="button">Workspace</button>,
}));

function renderRow() {
  return render(
    <LauncherInputContextRow
      projects={[]}
      selectedProject={null}
      onSelectWorkspace={vi.fn()}
      onAddFolder={vi.fn()}
    />,
  );
}

describe('LauncherInputContextRow', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('zh-CN');
  });

  it('labels the project field in English too', async () => {
    await i18n.changeLanguage('en-US');
    renderRow();

    expect(screen.getByRole('group', { name: 'Project' })).toBeInTheDocument();
  });

  it('renders as a labelled header row separated from the message body by a bottom hairline', () => {
    renderRow();

    const header = screen.getByTestId('launcher-context-header');
    expect(header).toHaveClass('border-b', 'border-[var(--line-subtle)]', 'px-4');
    // No floating-chip chrome from the previous below-input design.
    expect(header.innerHTML).not.toContain('shadow-xs');

    // The field is still labelled for assistive tech, but shows no visible "项目" caption.
    const projectGroup = screen.getByRole('group', { name: '项目' });
    expect(projectGroup).not.toHaveTextContent('项目');
    expect(projectGroup).toContainElement(screen.getByRole('button', { name: 'Workspace' }));
  });

  it('names only the project; the runtime picker lives in the toolbar', () => {
    renderRow();

    const header = screen.getByTestId('launcher-context-header');
    expect(screen.getAllByRole('group')).toHaveLength(1);
    expect(header).not.toHaveTextContent('环境');
    expect(header).not.toHaveTextContent('运行环境');
  });
});
