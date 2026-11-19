import { lazy, memo, Suspense } from 'react';

import type { SessionMetadata } from '@/api/sessionClient';
import type { Project } from '@/config/types';
import type { TabModuleDefinition } from '@/tab-workspace/contracts';
import { PAGE_FALLBACK } from '@/tab-workspace/PageFallback';

import type { TeamOfficeTab } from './tabContract';

const TeamOfficePage = lazy(() => import('./TeamOfficePage'));

export interface TeamOfficeBinding {
  /** Open the employee's conversation (replaces this page in the single-page shell). */
  onOpenEmployee: (session: SessionMetadata, project: Project) => unknown;
  /** Start a new conversation in the project. */
  onHireEmployee: (project: Project) => unknown;
}

const Renderer = memo(function TeamOfficeTabRenderer({ isDeferred, binding }: {
  tab: TeamOfficeTab;
  isActive: boolean;
  isDeferred: boolean;
  binding: TeamOfficeBinding;
}) {
  if (isDeferred) return PAGE_FALLBACK;
  return (
    <Suspense fallback={PAGE_FALLBACK}>
      <TeamOfficePage {...binding} />
    </Suspense>
  );
});

export const teamOfficeTabModule = {
  kind: 'teamoffice',
  render: Renderer,
  chrome: (_tab, { t }) => ({ title: t('tabs.teamOffice'), subtitle: t('tabs.teamOffice') }),
  identity: () => 'teamoffice',
  open: {
    findExisting: (tabs) => tabs[0],
    create: (intent, { id }) => ({ id, view: 'teamoffice', title: intent.title }),
  },
  initialMount: () => 'deferred-content',
} satisfies TabModuleDefinition<TeamOfficeTab, { title: string }, TeamOfficeBinding>;
