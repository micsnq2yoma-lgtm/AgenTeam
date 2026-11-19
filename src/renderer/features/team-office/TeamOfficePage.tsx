/**
 * 智能体团队 page, opened from the global sidebar. Seats every active project
 * (the same set the sidebar's 项目管理 list shows) in the pixel team office.
 */

import { useMemo } from 'react';

import type { SessionMetadata } from '@/api/sessionClient';
import { isProjectActiveForUser, type Project } from '@/config/types';
import { useConfig } from '@/hooks/useConfig';

import TeamOffice from './TeamOffice';

export default function TeamOfficePage({ onOpenEmployee, onHireEmployee }: {
  onOpenEmployee: (session: SessionMetadata, project: Project) => unknown;
  onHireEmployee: (project: Project) => unknown;
}) {
  const { projects } = useConfig();
  const activeProjects = useMemo(() => projects.filter(isProjectActiveForUser), [projects]);

  return (
    <div className="flex h-full min-h-0 w-full flex-col px-6 pb-6 pt-14" data-team-office-page>
      <TeamOffice
        projects={activeProjects}
        onOpenEmployee={(session, project) => { void onOpenEmployee(session, project); }}
        onHireEmployee={(project) => { void onHireEmployee(project); }}
      />
    </div>
  );
}
