import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { SessionMetadata } from '@/api/sessionClient';
import type { Project } from '@/config/types';
import { i18n } from '@/i18n';

const store = vi.hoisted(() => ({
  sessions: [] as SessionMetadata[],
  sessionTagsMap: new Map(),
  paths: [] as string[][],
  skin: undefined as string | undefined,
}));

vi.mock('@/hooks/useConfig', () => ({
  useConfig: () => ({ config: { teamOfficeSkin: store.skin } }),
}));

vi.mock('@/hooks/useTaskCenterData', () => ({
  usePassiveWorkspaceSessions: (paths: string[]) => {
    store.paths.push(paths);
    return { sessions: store.sessions, sessionTagsMap: store.sessionTagsMap };
  },
}));
vi.mock('@/theme', () => ({
  useResolvedTheme: () => ({ resolvedColorScheme: 'light' }),
}));

import TeamOffice from './TeamOffice';

const tl = (key: string, options?: Record<string, unknown>) => String(i18n.t(`launcher:${key}`, options));

const mino: Project = { id: 'mino', name: 'mino', path: '/home/mino', displayName: 'Mino', templateId: 'mino', providerId: null, permissionMode: null };
const app: Project = { id: 'app', name: 'app', path: '/work/app', displayName: 'App', providerId: null, permissionMode: null };

function session(id: string, agentDir: string, title: string): SessionMetadata {
  return { id, agentDir, title, createdAt: new Date().toISOString(), lastActiveAt: new Date().toISOString() };
}

beforeEach(async () => {
  await i18n.changeLanguage('zh-CN');
  store.sessions = [session('s1', '/work/app', '修登录'), session('s2', '/home/mino', '订机票')];
  store.sessionTagsMap = new Map([['s1', [{ type: 'background' }]]]);
  store.paths = [];
  store.skin = undefined;
});

describe('TeamOffice', () => {
  it('seats each project as a department with its assistant and employees', () => {
    render(<TeamOffice projects={[mino, app]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);

    expect(store.paths.at(-1)).toEqual(['/home/mino', '/work/app']);
    expect(screen.getByText(tl('teamOffice.skins.office.summary', { projects: 2, employees: 2, working: 1 }))).toBeInTheDocument();
    expect(document.querySelector('[data-team-office-department="mino"]')).toHaveTextContent('Mino');
    expect(document.querySelector('[data-team-office-department="app"]')).toHaveTextContent('App');

    const lead = document.querySelector('[data-team-office-person="lead:mino"]') as HTMLElement;
    expect(lead).toHaveTextContent(tl('teamOffice.skins.office.lead', { name: 'Mino' }));
    const worker = document.querySelector('[data-team-office-person="session:s1"]') as HTMLElement;
    expect(worker).toHaveAttribute('data-status', 'working');
    expect(screen.getAllByText('修登录').some((node) => node.closest('[data-team-office-bubble]'))).toBe(true);
  });

  it('opens an employee conversation and hires from a department or its assistant', () => {
    const onOpenEmployee = vi.fn();
    const onHireEmployee = vi.fn();
    render(<TeamOffice projects={[mino, app]} onOpenEmployee={onOpenEmployee} onHireEmployee={onHireEmployee} />);

    fireEvent.click(document.querySelector('[data-team-office-person="session:s1"]')!);
    expect(onOpenEmployee).toHaveBeenCalledWith(expect.objectContaining({ id: 's1' }), app);

    const department = document.querySelector('[data-team-office-department="app"]') as HTMLElement;
    fireEvent.click(within(department).getByRole('button', { name: tl('teamOffice.skins.office.hireIn', { project: 'App' }) }));
    expect(onHireEmployee).toHaveBeenLastCalledWith(app);

    fireEvent.click(document.querySelector('[data-team-office-person="lead:mino"]')!);
    expect(onHireEmployee).toHaveBeenLastCalledWith(mino);
  });

  it('speaks the chosen skin, falling back to the office for unknown values', () => {
    store.skin = 'court';
    const { container, unmount } = render(<TeamOffice projects={[mino, app]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);
    expect(container.querySelector('[data-team-office]')).toHaveAttribute('data-team-office-skin', 'court');
    expect(screen.getByText(tl('teamOffice.skins.court.summary', { projects: 2, employees: 2, working: 1 }))).toBeInTheDocument();
    expect(document.querySelector('[data-team-office-person="lead:mino"]')).toHaveTextContent(tl('teamOffice.skins.court.lead', { name: 'Mino' }));
    const department = document.querySelector('[data-team-office-department="app"]') as HTMLElement;
    expect(within(department).getByRole('button', { name: tl('teamOffice.skins.court.hireIn', { project: 'App' }) })).toBeInTheDocument();
    unmount();

    store.skin = 'vaporwave';
    const fallback = render(<TeamOffice projects={[mino]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);
    expect(fallback.container.querySelector('[data-team-office]')).toHaveAttribute('data-team-office-skin', 'office');
  });

  it('explains the empty office when there are no projects', () => {
    render(<TeamOffice projects={[]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);
    expect(screen.getByText(tl('teamOffice.emptyTitle'))).toBeInTheDocument();
  });
});
