/**
 * TeamOffice — the launcher's 「团队」 view. Every project is a 项目部 in one
 * pixel office: an assistant-mode project seats its resident assistant at the
 * head desk, and each recent conversation is an employee at a desk whose
 * action shows what that conversation is actually doing (see
 * teamOfficeModel.ts). Clicking an employee opens its conversation; the
 * 「新员工」 button on a department (or clicking its assistant) starts a new one.
 * The scenery and the wording follow the team skin chosen in Settings → 团队形象.
 *
 * Naming: project → assistant → 任务. A session is a 任务 everywhere in the UI
 * except here, where the same session is shown as a 员工.
 */

import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { PlusIcon } from '@/components/icons';
import type { SessionMetadata } from '@/api/sessionClient';
import { normalizeTeamOfficeSkin, type Project } from '@/config/types';
import { useConfig } from '@/hooks/useConfig';
import { usePassiveWorkspaceSessions } from '@/hooks/useTaskCenterData';
import { useResolvedTheme } from '@/theme';

import { layoutOffice, OfficeRenderer, seatedLabels } from './pixel/office';
import { OFFICE_SKINS } from './pixel/skins';
import { buildTeamOffice, type EmployeeStatus } from './teamOfficeModel';

const STATUS_ORDER: readonly EmployeeStatus[] = ['working', 'scheduled', 'on-call', 'idle', 'resting'];

/** Dot colours echo what the sprite does: screen-on green, amber for scheduled, blue for chat duty. */
const STATUS_DOT: Record<EmployeeStatus, string> = {
  working: 'bg-[#3E9C8C]',
  scheduled: 'bg-[#D9A441]',
  'on-call': 'bg-[#5B7FD6]',
  idle: 'bg-[var(--ink-subtle)]',
  resting: 'bg-[var(--line-strong)]',
};

const MIN_SCALE = 2;
const MAX_SCALE = 4;
const REFRESH_CLOCK_MS = 60_000;

interface TeamOfficeProps {
  projects: readonly Project[];
  onOpenEmployee: (session: SessionMetadata, project: Project) => void;
  onHireEmployee: (project: Project) => void;
}

export default memo(function TeamOffice({ projects, onOpenEmployee, onHireEmployee }: TeamOfficeProps) {
  const { t } = useTranslation('launcher');
  const resolvedTheme = useResolvedTheme();
  const { config } = useConfig();
  const skin = normalizeTeamOfficeSkin(config.teamOfficeSkin);
  const words = `teamOffice.skins.${skin}`;
  const projectPaths = useMemo(() => projects.map((project) => project.path), [projects]);
  const data = usePassiveWorkspaceSessions(projectPaths);

  // Resting is time-based, so re-derive once a minute even without new data.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), REFRESH_CLOCK_MS);
    return () => window.clearInterval(timer);
  }, []);

  const office = useMemo(
    () => buildTeamOffice({ projects, sessions: data.sessions, sessionTags: data.sessionTagsMap, now }),
    [data.sessionTagsMap, data.sessions, now, projects],
  );
  const layout = useMemo(() => layoutOffice(office.snapshot), [office.snapshot]);
  const labels = useMemo(() => seatedLabels(office.snapshot, layout), [layout, office.snapshot]);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<OfficeRenderer | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [scale, setScale] = useState(MIN_SCALE);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || typeof canvas.getContext !== 'function') return;
    let renderer: OfficeRenderer;
    try {
      renderer = new OfficeRenderer(canvas);
    } catch {
      return; // No 2D context (e.g. a test DOM); the HTML overlay still works.
    }
    rendererRef.current = renderer;
    renderer.start();
    return () => {
      renderer.stop();
      rendererRef.current = null;
    };
  }, []);

  useEffect(() => {
    rendererRef.current?.setSkin(OFFICE_SKINS[skin], resolvedTheme.resolvedColorScheme === 'dark');
  }, [resolvedTheme.resolvedColorScheme, skin]);

  useEffect(() => {
    rendererRef.current?.setSnapshot(office.snapshot);
  }, [office.snapshot]);

  useEffect(() => {
    if (rendererRef.current) rendererRef.current.selected = hovered;
  }, [hovered]);

  // Largest whole-number scale that fits the viewport width (crisp pixels);
  // wider offices keep the minimum scale and scroll horizontally.
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const measure = () => {
      const fit = Math.floor((viewport.clientWidth - 2) / layout.width);
      setScale(Math.max(MIN_SCALE, Math.min(MAX_SCALE, fit)));
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [layout.width]);

  const activate = useCallback((personId: string) => {
    const entry = office.entries.get(personId);
    if (!entry) return;
    if (entry.kind === 'employee' && entry.session) onOpenEmployee(entry.session, entry.project);
    else onHireEmployee(entry.project);
  }, [office.entries, onHireEmployee, onOpenEmployee]);

  const hitTest = useCallback((event: React.MouseEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (!rect.width || !rect.height) return null;
    return rendererRef.current?.hitTest(
      (event.clientX - rect.left) / rect.width,
      (event.clientY - rect.top) / rect.height,
    ) ?? null;
  }, []);

  const width = layout.width * scale;
  const height = layout.height * scale;
  const statusText = (status: EmployeeStatus) => t(`teamOffice.status.${status}`);

  return (
    <section className="flex h-full min-h-0 w-full flex-col" aria-label={t('teamOffice.title')} data-team-office data-team-office-skin={skin}>
      <header className="mx-auto flex w-full max-w-[1040px] flex-wrap items-end justify-between gap-x-6 gap-y-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold text-[var(--ink)]">{t('teamOffice.title')}</h2>
          <p className="mt-0.5 text-sm text-[var(--ink-muted)]" data-team-office-summary>
            {t(`${words}.summary`, office.counts)}
          </p>
        </div>
        <ul className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-[var(--ink-muted)]" aria-label={t('teamOffice.legend')}>
          {STATUS_ORDER.map((status) => (
            <li key={status} className="inline-flex items-center gap-1.5">
              <span className={`h-2 w-2 rounded-full ${STATUS_DOT[status]}`} aria-hidden="true" />
              {statusText(status)}
            </li>
          ))}
        </ul>
      </header>

      {projects.length === 0 ? (
        <div className="mx-auto mt-10 max-w-sm text-center" data-team-office-empty>
          <p className="text-sm font-medium text-[var(--ink)]">{t('teamOffice.emptyTitle')}</p>
          <p className="mt-1 text-xs text-[var(--ink-muted)]">{t('teamOffice.emptyDescription')}</p>
        </div>
      ) : (
        <div
          ref={viewportRef}
          className="mx-auto mt-4 min-h-0 w-full max-w-[1040px] flex-1 overflow-auto rounded-xl border border-[var(--line)] bg-[var(--paper-inset)]"
          style={{ scrollbarGutter: 'stable' }}
        >
          <div className="relative mx-auto" style={{ width, height }}>
            <canvas
              ref={canvasRef}
              width={layout.width}
              height={layout.height}
              aria-hidden="true"
              className="block"
              style={{ width, height, imageRendering: 'pixelated', cursor: hovered ? 'pointer' : 'default' }}
              onMouseMove={(event) => setHovered(hitTest(event))}
              onMouseLeave={() => setHovered(null)}
              onClick={(event) => {
                const personId = hitTest(event);
                if (personId) activate(personId);
              }}
            />

            {layout.zones.map((zone) => {
              const project = projects.find((candidate) => candidate.id === zone.id);
              if (!project) return null;
              const hidden = office.hiddenStaff.get(project.id) ?? 0;
              return (
                <div
                  key={zone.id}
                  className="absolute flex items-center justify-between gap-1 px-2"
                  style={{ left: zone.x * scale, top: 2 * scale, width: zone.w * scale }}
                  data-team-office-department={project.id}
                >
                  <span className="min-w-0 truncate rounded-md bg-[var(--paper-elevated)]/90 px-2 py-0.5 text-xs font-semibold text-[var(--ink)] shadow-xs">
                    {zone.name}
                    {hidden > 0 && (
                      <span className="ml-1 font-normal text-[var(--ink-muted)]">{t('teamOffice.more', { count: hidden })}</span>
                    )}
                  </span>
                  <button
                    type="button"
                    onClick={() => onHireEmployee(project)}
                    className="inline-flex shrink-0 items-center gap-1 rounded-md bg-[var(--paper-elevated)]/90 px-1.5 py-0.5 text-xs text-[var(--ink-muted)] shadow-xs transition-colors hover:text-[var(--accent)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
                    aria-label={t(`${words}.hireIn`, { project: zone.name })}
                  >
                    <PlusIcon className="h-3 w-3" />
                    {t(`${words}.hire`)}
                  </button>
                </div>
              );
            })}

            {labels.map((label) => {
              const entry = office.entries.get(label.agentId);
              if (!entry) return null;
              const x = label.left * width;
              const name = entry.kind === 'lead' ? t(`${words}.lead`, { name: label.name }) : label.name;
              return (
                <div key={label.agentId}>
                  {label.bubble && (
                    <div
                      className="pointer-events-none absolute max-w-[160px] -translate-x-1/2 -translate-y-full rounded-lg border border-[var(--line)] bg-[var(--paper-elevated)] px-2 py-1 text-xs leading-snug text-[var(--ink)] shadow-sm"
                      style={{ left: x, top: label.bubbleTop * height }}
                      data-team-office-bubble
                    >
                      <span className="line-clamp-2">{label.bubble}</span>
                    </div>
                  )}
                  <button
                    type="button"
                    onClick={() => activate(label.agentId)}
                    onMouseEnter={() => setHovered(label.agentId)}
                    onMouseLeave={() => setHovered(null)}
                    onFocus={() => setHovered(label.agentId)}
                    onBlur={() => setHovered(null)}
                    title={`${name} · ${statusText(entry.status)}`}
                    className="absolute inline-flex max-w-[132px] -translate-x-1/2 items-center gap-1 rounded-md bg-[var(--paper-elevated)]/90 px-1.5 py-0.5 text-xs leading-tight text-[var(--ink)] shadow-xs transition-colors hover:bg-[var(--paper-elevated)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
                    style={{ left: x, top: label.top * height }}
                    data-team-office-person={label.agentId}
                    data-status={entry.status}
                  >
                    <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${STATUS_DOT[entry.status]}`} aria-hidden="true" />
                    <span className={`truncate ${entry.kind === 'lead' ? 'font-semibold' : ''}`}>{name}</span>
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
});
