// LauncherInputContextRow — the Launcher composer's context header.
//
// Rendered by SimpleChatInput's `contextHeader` slot as the first row INSIDE
// the composer frame, like the "To:" line of an email:
//
//   项目 [icon] AgenTeam
//   ─────────────────────────────────────────────────────────
//   输入消息…
//
// Why inside the composer: the previous floating chips below the input read as
// loose parts, and "AgenTeam" alone did not tell users it was the current
// project. An explicit label plus placement inside the frame make the row read
// as "where this message goes". The trigger has no chevron; the name itself is
// the click target that opens the existing picker list. The runtime picker
// lives in the toolbar next to Tools (LauncherRuntimeToolbarControl), because
// it governs the same things the toolbar does: tools, permissions and models.
//
// The label uses the meta tier (`text-xs`), the value the ui tier (`text-sm`),
// per DESIGN.md §3.3. The row's bottom hairline separates context from the
// message body.

import { memo } from 'react';
import { useTranslation } from 'react-i18next';

import type { Project } from '@/config/types';

import WorkspaceSelector from './WorkspaceSelector';

interface LauncherInputContextRowProps {
  projects: Project[];
  selectedProject: Project | null;
  defaultWorkspacePath?: string;
  onSelectWorkspace: (project: Project) => void;
  onAddFolder: () => void;
  /** Promote a project to default workspace via the dropdown's hover-only
   *  "设为默认" button. Threaded straight through to WorkspaceSelector. */
  onSetDefaultWorkspace?: (project: Project) => void;
}

const LABEL_CLASS = 'shrink-0 text-xs text-[var(--ink-muted)]';

export default memo(function LauncherInputContextRow({
  projects,
  selectedProject,
  defaultWorkspacePath,
  onSelectWorkspace,
  onAddFolder,
  onSetDefaultWorkspace,
}: LauncherInputContextRowProps) {
  const { t } = useTranslation('launcher');
  const projectLabel = t('workspaceSelector.label');

  return (
    <div
      data-testid="launcher-context-header"
      className="flex min-w-0 items-center border-b border-[var(--line-subtle)] px-4 py-1.5"
    >
      <div role="group" aria-label={projectLabel} className="flex min-w-0 items-center gap-1">
        <span className={LABEL_CLASS}>{projectLabel}</span>
        <WorkspaceSelector
          projects={projects}
          selectedProject={selectedProject}
          defaultWorkspacePath={defaultWorkspacePath}
          onSelect={onSelectWorkspace}
          onAddFolder={onAddFolder}
          onSetDefault={onSetDefaultWorkspace}
        />
      </div>
    </div>
  );
});
