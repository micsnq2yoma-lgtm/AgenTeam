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

const atmer: Project = { id: 'atmer', name: 'atmer', path: '/home/atmer', displayName: 'Atmer', templateId: 'atmer', providerId: null, permissionMode: null };
const app: Project = { id: 'app', name: 'app', path: '/work/app', displayName: 'App', providerId: null, permissionMode: null };

function session(id: string, agentDir: string, title: string): SessionMetadata {
  return { id, agentDir, title, createdAt: new Date().toISOString(), lastActiveAt: new Date().toISOString() };
}

beforeEach(async () => {
  await i18n.changeLanguage('zh-CN');
  store.sessions = [session('s1', '/work/app', '修登录'), session('s2', '/home/atmer', '订机票')];
  store.sessionTagsMap = new Map([['s1', [{ type: 'background' }]]]);
  store.paths = [];
  store.skin = undefined;
});

describe('TeamOffice', () => {
  it('seats each project as a department with its assistant and employees', () => {
    render(<TeamOffice projects={[atmer, app]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);

    expect(store.paths.at(-1)).toEqual(['/home/atmer', '/work/app']);
    expect(screen.getByText(tl('teamOffice.skins.office.summary', { projects: 2, employees: 2, working: 1 }))).toBeInTheDocument();
    expect(document.querySelector('[data-team-office-department="atmer"]')).toHaveTextContent('Atmer');
    expect(document.querySelector('[data-team-office-department="app"]')).toHaveTextContent('App');

    const lead = document.querySelector('[data-team-office-person="lead:atmer"]') as HTMLElement;
    expect(lead).toHaveTextContent(tl('teamOffice.skins.office.lead', { name: 'Atmer' }));
    const worker = document.querySelector('[data-team-office-person="session:s1"]') as HTMLElement;
    expect(worker).toHaveAttribute('data-status', 'working');
    expect(screen.getAllByText('修登录').some((node) => node.closest('[data-team-office-bubble]'))).toBe(true);
  });

  it('opens an employee conversation and hires from a department or its assistant', () => {
    const onOpenEmployee = vi.fn();
    const onHireEmployee = vi.fn();
    render(<TeamOffice projects={[atmer, app]} onOpenEmployee={onOpenEmployee} onHireEmployee={onHireEmployee} />);

    fireEvent.click(document.querySelector('[data-team-office-person="session:s1"]')!);
    expect(onOpenEmployee).toHaveBeenCalledWith(expect.objectContaining({ id: 's1' }), app);

    const department = document.querySelector('[data-team-office-department="app"]') as HTMLElement;
    fireEvent.click(within(department).getByRole('button', { name: tl('teamOffice.skins.office.hireIn', { project: 'App' }) }));
    expect(onHireEmployee).toHaveBeenLastCalledWith(app);

    fireEvent.click(document.querySelector('[data-team-office-person="lead:atmer"]')!);
    expect(onHireEmployee).toHaveBeenLastCalledWith(atmer);
  });

  it('speaks the chosen skin, falling back to the office for unknown values', () => {
    store.skin = 'court';
    const { container, unmount } = render(<TeamOffice projects={[atmer, app]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);
    expect(container.querySelector('[data-team-office]')).toHaveAttribute('data-team-office-skin', 'court');
    expect(screen.getByText(tl('teamOffice.skins.court.summary', { projects: 2, employees: 2, working: 1 }))).toBeInTheDocument();
    expect(document.querySelector('[data-team-office-person="lead:atmer"]')).toHaveTextContent(tl('teamOffice.skins.court.lead', { name: 'Atmer' }));
    const department = document.querySelector('[data-team-office-department="app"]') as HTMLElement;
    expect(within(department).getByRole('button', { name: tl('teamOffice.skins.court.hireIn', { project: 'App' }) })).toBeInTheDocument();
    unmount();

    store.skin = 'vaporwave';
    const fallback = render(<TeamOffice projects={[atmer]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);
    expect(fallback.container.querySelector('[data-team-office]')).toHaveAttribute('data-team-office-skin', 'office');
  });

  it('explains the empty office when there are no projects', () => {
    render(<TeamOffice projects={[]} onOpenEmployee={vi.fn()} onHireEmployee={vi.fn()} />);
    expect(screen.getByText(tl('teamOffice.emptyTitle'))).toBeInTheDocument();
  });

  it('adapts to multi-floor mode on large teams and lets users zoom into a department', () => {
    const widthSpy = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1040);
    const heightSpy = vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(700);
    try {
      const manyProjects: Project[] = Array.from({ length: 8 }, (_, i) => ({
        id: `p${i}`,
        name: `p${i}`,
        path: `/work/p${i}`,
        displayName: `项目 ${i + 1}`,
        providerId: null,
        permissionMode: null,
      }));
      store.sessions = manyProjects.flatMap((p) =>
        Array.from({ length: 4 }, (_, j) => session(`${p.id}-s${j}`, p.path, `${p.displayName} 任务 ${j + 1}`)),
      );
      const onOpenEmployee = vi.fn();
      const { container } = render(
        <TeamOffice projects={manyProjects} onOpenEmployee={onOpenEmployee} onHireEmployee={vi.fn()} />,
      );

      expect(container.querySelector('[data-team-office]')).toHaveAttribute('data-team-office-tier', 'floors');
      expect(container.querySelector('[data-team-office-tier-badge]')).toHaveTextContent(tl('teamOffice.tiers.floors'));

      const empBtn = document.querySelector('[data-team-office-person="session:p0-s0"]') as HTMLElement;
      expect(empBtn).toBeInTheDocument();
      fireEvent.mouseEnter(empBtn);
      expect(document.querySelector('[data-team-office-tooltip]')).toHaveTextContent('项目 1 任务 1');
      fireEvent.click(empBtn);
      expect(onOpenEmployee).toHaveBeenCalledWith(expect.objectContaining({ id: 'p0-s0' }), manyProjects[0]);

      const dept = document.querySelector('[data-team-office-department="p0"]') as HTMLElement;
      fireEvent.click(within(dept).getByRole('button', { name: /^项目 1$|^放大查看「项目 1」$/ }));
      expect(container.querySelector('[data-team-office]')).toHaveAttribute('data-team-office-tier', 'room');
      expect(document.querySelector('[data-team-office-back]')).toBeInTheDocument();

      fireEvent.click(document.querySelector('[data-team-office-back]')!);
      expect(container.querySelector('[data-team-office]')).toHaveAttribute('data-team-office-tier', 'floors');
    } finally {
      widthSpy.mockRestore();
      heightSpy.mockRestore();
    }
  });
});
