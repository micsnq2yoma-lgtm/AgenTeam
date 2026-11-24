/**
 * Team office projection — turns projects and their conversations into the
 * pixel office snapshot. Pure and unit-tested; the canvas only draws it.
 *
 * Mapping (product vocabulary → office):
 *  - 项目 (Project)          → 项目部 (a department zone)
 *  - 助理模式的常驻助理       → the department lead at the head desk
 *  - 员工 (a conversation)   → a seated employee
 *
 * Action design follows what each conversation is actually doing, never a
 * random "busy" animation:
 *  - background completion running   → working   (typing, screen on)
 *  - scheduled task running          → thinking  (定时值守)
 *  - bound to an online IM channel   → reading   (值守聊天)
 *  - untouched for a week            → paused    (休息中, greyed)
 *  - otherwise                       → idle      (待命)
 * The lead types while any of its employees work, and otherwise waits.
 */

import type { SessionMetadata } from '@/api/sessionClient';
import {
  DEFAULT_BUNDLED_WORKSPACE_TEMPLATE_ID,
  isSystemPresetProject,
  type Project,
} from '@/config/types';
import type { SessionTag } from '@/hooks/useTaskCenterData';
import { normalizeWorkspacePathIdentity } from '../../../shared/workspacePath';

import type { PixelAction } from './pixel/characters';
import type { OfficePerson, OfficeSnapshot } from './pixel/office';
import { modelFamilyFor, type ModelFamily } from './pixel/types';

export const STAFF_PER_PROJECT = 16;
export const MAX_DEPARTMENTS = 20;
export const RESTING_AFTER_MS = 7 * 24 * 60 * 60 * 1000;

export type EmployeeStatus = 'working' | 'scheduled' | 'on-call' | 'idle' | 'resting';

export const STATUS_ACTION: Record<EmployeeStatus, PixelAction> = {
  working: 'working',
  scheduled: 'thinking',
  'on-call': 'reading',
  idle: 'idle',
  resting: 'paused',
};

export interface TeamOfficeEntry {
  personId: string;
  kind: 'lead' | 'employee';
  project: Project;
  session: SessionMetadata | null;
  status: EmployeeStatus;
}

export interface TeamOffice {
  snapshot: OfficeSnapshot;
  entries: ReadonlyMap<string, TeamOfficeEntry>;
  /** Projects with more conversations than seats, keyed by project id. */
  hiddenStaff: ReadonlyMap<string, number>;
  counts: { projects: number; employees: number; working: number };
}

export function isAssistantProject(project: Pick<Project, 'workspaceType' | 'systemPresetId' | 'templateId'>): boolean {
  return isSystemPresetProject(project) || project.templateId === DEFAULT_BUNDLED_WORKSPACE_TEMPLATE_ID;
}

export function employeeStatus(
  session: SessionMetadata,
  tags: readonly SessionTag[] | undefined,
  now: number,
): EmployeeStatus {
  if (tags?.some((tag) => tag.type === 'background')) return 'working';
  if (tags?.some((tag) => tag.type === 'cron')) return 'scheduled';
  if (tags?.some((tag) => tag.type === 'im')) return 'on-call';
  const lastActive = Date.parse(session.lastActiveAt);
  if (Number.isFinite(lastActive) && now - lastActive > RESTING_AFTER_MS) return 'resting';
  return 'idle';
}

function familyFor(model: string | null | undefined, runtime: string | undefined, fallback: ModelFamily): ModelFamily {
  const family = modelFamilyFor(runtime, model ?? undefined);
  return family === 'other' && !model && !runtime ? fallback : family;
}

function byRecentActivity(a: SessionMetadata, b: SessionMetadata): number {
  return (Date.parse(b.lastActiveAt) || 0) - (Date.parse(a.lastActiveAt) || 0);
}

export function buildTeamOffice({
  projects,
  sessions,
  sessionTags,
  now,
}: {
  projects: readonly Project[];
  sessions: readonly SessionMetadata[];
  sessionTags: ReadonlyMap<string, readonly SessionTag[]>;
  now: number;
}): TeamOffice {
  const departments = [...projects]
    .sort((a, b) => (Date.parse(b.lastOpened ?? '') || 0) - (Date.parse(a.lastOpened ?? '') || 0))
    .slice(0, MAX_DEPARTMENTS);
  const sessionsByProject = new Map<string, SessionMetadata[]>();
  for (const session of sessions) {
    if (session.systemMaintenanceKind) continue;
    const key = normalizeWorkspacePathIdentity(session.agentDir);
    const list = sessionsByProject.get(key) ?? [];
    list.push(session);
    sessionsByProject.set(key, list);
  }

  const people: OfficePerson[] = [];
  const entries = new Map<string, TeamOfficeEntry>();
  const hiddenStaff = new Map<string, number>();
  let employees = 0;
  let working = 0;

  for (const project of departments) {
    const all = (sessionsByProject.get(normalizeWorkspacePathIdentity(project.path)) ?? []).sort(byRecentActivity);
    const staff = all.slice(0, STAFF_PER_PROJECT);
    if (all.length > staff.length) hiddenStaff.set(project.id, all.length - staff.length);
    const projectName = project.displayName || project.name;
    const projectFamily = familyFor(project.model, undefined, 'claude');
    const staffStatuses = staff.map((session) => employeeStatus(session, sessionTags.get(session.id), now));

    if (isAssistantProject(project)) {
      const personId = `lead:${project.id}`;
      const status: EmployeeStatus = staffStatuses.includes('working') ? 'working' : 'idle';
      people.push({
        agentId: personId,
        departmentId: project.id,
        name: projectName,
        title: '',
        model: project.model ?? '',
        eye: projectFamily,
        isLead: true,
        anim: STATUS_ACTION[status],
        bubble: null,
        pendingCards: 0,
      });
      entries.set(personId, { personId, kind: 'lead', project, session: null, status });
    }

    staff.forEach((session, index) => {
      const status = staffStatuses[index]!;
      const personId = `session:${session.id}`;
      employees += 1;
      if (status === 'working') working += 1;
      people.push({
        agentId: personId,
        departmentId: project.id,
        name: session.title || projectName,
        title: session.title,
        model: session.model ?? '',
        eye: familyFor(session.model, session.runtime, projectFamily),
        isLead: false,
        anim: STATUS_ACTION[status],
        bubble: status === 'working' ? session.title || null : null,
        pendingCards: 0,
      });
      entries.set(personId, { personId, kind: 'employee', project, session, status });
    });
  }

  return {
    snapshot: {
      at: new Date(now).toISOString(),
      departments: departments.map((project) => ({
        id: project.id,
        name: project.displayName || project.name,
        icon: project.icon ?? '',
      })),
      people,
    },
    entries,
    hiddenStaff,
    counts: { projects: departments.length, employees, working },
  };
}
