/**
 * TeamOffice — the launcher's 「团队」 view. Every project is a 项目部 in one
 * pixel office: an assistant-mode project seats its resident assistant at the
 * head desk, and each recent conversation is an employee at a desk whose
 * action shows what that conversation is actually doing (see
 * teamOfficeModel.ts). Clicking an employee opens its conversation; the
 * 「新员工」 button on a department (or clicking its assistant) starts a new one.
 * The scenery and the wording follow the team skin chosen in Settings → 团队形象.
 *
 * Adaptive layout: as projects and staff grow, the office grows automatically
 * from a single room → compact room → multi-floor building → office tower →
 * campus so the whole team stays visible without scrolling. In compact/mini
 * tiers, clicking a department name zooms into that department up close.
 *
 * Naming: project → assistant → 任务. A session is a 任务 everywhere in the UI
 * except here, where the same session is shown as a 员工.
 */

import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { ArrowLeftIcon, PlusIcon } from '@/components/icons';
import type { SessionMetadata } from '@/api/sessionClient';
import { normalizeTeamOfficeSkin, type Project } from '@/config/types';
import { useConfig } from '@/hooks/useConfig';
import { usePassiveWorkspaceSessions } from '@/hooks/useTaskCenterData';
import { useResolvedTheme } from '@/theme';

import { layoutOffice, OfficeRenderer, seatedLabels, type OfficeSnapshot, type OfficeViewport } from './pixel/office';
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

  const [focusedProjectId, setFocusedProjectId] = useState<string | null>(null);
  const effectiveFocusedId = useMemo(
    () => (focusedProjectId && office.snapshot.departments.some((d) => d.id === focusedProjectId) ? focusedProjectId : null),
    [focusedProjectId, office.snapshot.departments],
  );

  const activeSnapshot = useMemo<OfficeSnapshot>(() => {
    if (!effectiveFocusedId) return office.snapshot;
    return {
      at: office.snapshot.at,
      departments: office.snapshot.departments.filter((d) => d.id === effectiveFocusedId),
      people: office.snapshot.people.filter((p) => p.departmentId === effectiveFocusedId),
    };
  }, [effectiveFocusedId, office.snapshot]);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<OfficeRenderer | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [viewport, setViewport] = useState<OfficeViewport | null>(null);

  useLayoutEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const measure = () => {
      const w = el.clientWidth;
      const h = el.clientHeight;
      if (w <= 0 || h <= 0) return;
      setViewport((prev) => (prev && prev.width === w && prev.height === h ? prev : { width: w, height: h }));
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [projects.length]);

  const layout = useMemo(() => layoutOffice(activeSnapshot, viewport), [activeSnapshot, viewport]);
  const labels = useMemo(() => seatedLabels(activeSnapshot, layout), [activeSnapshot, layout]);

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
    rendererRef.current?.setSnapshot(activeSnapshot, viewport);
  }, [activeSnapshot, viewport]);

  useEffect(() => {
    if (rendererRef.current) rendererRef.current.selected = hovered;
  }, [hovered]);

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

  const scale = layout.scale;
  const width = Math.round(layout.width * scale);
  const height = Math.round(layout.height * scale);
  const backing = layout.density === 'compact' ? 2 : 1;
  const isCompact = layout.density !== 'full';
  const isMini = layout.density === 'mini';
  const statusText = (status: EmployeeStatus) => t(`teamOffice.status.${status}`);

  const hoveredLabel = hovered && isCompact ? labels.find((l) => l.agentId === hovered) ?? null : null;
  const hoveredEntry = hoveredLabel ? office.entries.get(hoveredLabel.agentId) ?? null : null;

  return (
    <section
      className="flex h-full min-h-0 w-full flex-col"
      aria-label={t('teamOffice.title')}
      data-team-office
      data-team-office-skin={skin}
      data-team-office-tier={layout.tier}
    >
      <header className="mx-auto flex w-full max-w-[1040px] flex-wrap items-end justify-between gap-x-6 gap-y-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold text-[var(--ink)]">{t('teamOffice.title')}</h2>
            {projects.length > 0 && effectiveFocusedId ? (
              <button
                type="button"
                onClick={() => setFocusedProjectId(null)}
                className="inline-flex items-center gap-1 rounded-md border border-[var(--line)] bg-[var(--paper-elevated)] px-2 py-0.5 text-xs text-[var(--ink-muted)] transition-colors hover:text-[var(--ink)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
                data-team-office-back
              >
                <ArrowLeftIcon className="h-3 w-3" />
                {t('teamOffice.backToOverview')}
              </button>
            ) : projects.length > 0 && viewport ? (
              <span
                className="rounded-full border border-[var(--line)] bg-[var(--paper-elevated)] px-2 py-0.5 text-xs text-[var(--ink-muted)]"
                data-team-office-tier-badge
              >
                {t(`teamOffice.tiers.${layout.tier}`)}
              </span>
            ) : null}
          </div>
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
          className="mx-auto mt-4 flex min-h-0 w-full max-w-[1040px] flex-1 items-center justify-center overflow-auto rounded-xl border border-[var(--line)] bg-[var(--paper-inset)]"
          style={{ scrollbarGutter: 'stable' }}
        >
          <div className="relative shrink-0" style={{ width, height }}>
            <canvas
              ref={canvasRef}
              width={Math.max(1, Math.ceil(layout.width * backing))}
              height={Math.max(1, Math.ceil(layout.height * backing))}
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
              const canZoom = isCompact && !effectiveFocusedId;
              const zoneLeft = zone.x * scale;
              const zoneTop = zone.y * scale + (isMini ? 1 : 2 * scale);
              const zoneWidthPx = zone.w * scale;

              return (
                <div
                  key={zone.id}
                  className={`absolute flex items-center justify-between gap-1 ${isMini ? 'px-1' : 'px-2'}`}
                  style={{ left: zoneLeft, top: zoneTop, width: zoneWidthPx }}
                  data-team-office-department={project.id}
                >
                  {canZoom ? (
                    <button
                      type="button"
                      onClick={() => setFocusedProjectId(project.id)}
                      title={t('teamOffice.zoomDepartment', { project: zone.name })}
                      className={`min-w-0 truncate rounded bg-[var(--paper-elevated)]/90 font-semibold text-[var(--ink)] shadow-xs transition-colors hover:text-[var(--accent)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] ${isMini ? 'px-1 py-0 text-xs leading-tight' : 'px-1.5 py-0.5 text-xs'}`}
                    >
                      {zone.name}
                      {hidden > 0 && (
                        <span className="ml-1 font-normal text-[var(--ink-muted)]">{t('teamOffice.more', { count: hidden })}</span>
                      )}
                    </button>
                  ) : (
                    <span className="min-w-0 truncate rounded-md bg-[var(--paper-elevated)]/90 px-2 py-0.5 text-xs font-semibold text-[var(--ink)] shadow-xs">
                      {zone.name}
                      {hidden > 0 && (
                        <span className="ml-1 font-normal text-[var(--ink-muted)]">{t('teamOffice.more', { count: hidden })}</span>
                      )}
                    </span>
                  )}

                  <button
                    type="button"
                    onClick={() => onHireEmployee(project)}
                    className={`inline-flex shrink-0 items-center gap-1 rounded bg-[var(--paper-elevated)]/90 text-[var(--ink-muted)] shadow-xs transition-colors hover:text-[var(--accent)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] ${isMini ? 'p-0.5' : isCompact ? 'p-0.5 text-xs' : 'rounded-md px-1.5 py-0.5 text-xs'}`}
                    aria-label={t(`${words}.hireIn`, { project: zone.name })}
                    title={t(`${words}.hireIn`, { project: zone.name })}
                  >
                    <PlusIcon className={isMini ? 'h-2.5 w-2.5' : 'h-3 w-3'} />
                    {!isCompact && t(`${words}.hire`)}
                  </button>
                </div>
              );
            })}

            {labels.map((label) => {
              const entry = office.entries.get(label.agentId);
              if (!entry) return null;
              const x = label.left * width;
              const name = entry.kind === 'lead' ? t(`${words}.lead`, { name: label.name }) : label.name;

              if (isCompact) {
                return (
                  <button
                    key={label.agentId}
                    type="button"
                    onClick={() => activate(label.agentId)}
                    onMouseEnter={() => setHovered(label.agentId)}
                    onMouseLeave={() => setHovered(null)}
                    onFocus={() => setHovered(label.agentId)}
                    onBlur={() => setHovered(null)}
                    title={`${name} · ${statusText(entry.status)}`}
                    className="absolute rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
                    style={{
                      left: label.hitLeft * width,
                      top: label.hitTop * height,
                      width: Math.max(12, label.hitWidth * width),
                      height: Math.max(12, label.hitHeight * height),
                    }}
                    data-team-office-person={label.agentId}
                    data-status={entry.status}
                  >
                    <span className="sr-only">{name}</span>
                  </button>
                );
              }

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

            {hoveredLabel && hoveredEntry && (
              <div
                className="pointer-events-none absolute z-20 inline-flex max-w-[220px] -translate-x-1/2 -translate-y-full items-center gap-1.5 rounded-md border border-[var(--line)] bg-[var(--paper-elevated)] px-2 py-1 text-xs leading-tight text-[var(--ink)] shadow-md"
                style={{
                  left: Math.max(80, Math.min(width - 80, hoveredLabel.left * width)),
                  top: Math.max(28, hoveredLabel.hitTop * height - 6),
                }}
                data-team-office-tooltip
              >
                <span className={`h-2 w-2 shrink-0 rounded-full ${STATUS_DOT[hoveredEntry.status]}`} aria-hidden="true" />
                <span className="truncate font-medium">
                  {hoveredEntry.kind === 'lead'
                    ? t(`${words}.lead`, { name: hoveredLabel.name })
                    : hoveredLabel.name}
                </span>
                <span className="shrink-0 text-[var(--ink-muted)]">· {statusText(hoveredEntry.status)}</span>
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
});
