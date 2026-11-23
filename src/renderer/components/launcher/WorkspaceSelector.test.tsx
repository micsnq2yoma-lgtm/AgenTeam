import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import type { Project } from '@/config/types';
import WorkspaceSelector from './WorkspaceSelector';

const project = {
  id: 'p1', name: 'Test', displayName: 'Test', path: '/work/test', lastOpened: '2026-11-23T00:00:00Z',
  providerId: null, permissionMode: null,
} as unknown as Project;

describe('WorkspaceSelector (launcher variant)', () => {
  beforeEach(async () => { await i18n.changeLanguage('zh-CN'); });

  it('shows a dropdown chevron and starts flush with the message text', () => {
    render(<WorkspaceSelector projects={[project]} selectedProject={project} onSelect={vi.fn()} />);
    const trigger = screen.getByTitle('/work/test');
    expect(trigger.querySelector('svg.rotate-180')).not.toBeNull();
    expect(trigger).toHaveClass('-ml-1.5');
  });

  it('keeps the same alignment when there is no project yet', () => {
    render(<WorkspaceSelector projects={[]} selectedProject={null} onSelect={vi.fn()} onAddFolder={vi.fn()} />);
    expect(screen.getByRole('button', { name: '选择项目' })).toHaveClass('-ml-1.5');
  });
});
