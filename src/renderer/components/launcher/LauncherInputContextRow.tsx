// LauncherInputContextRow — the Launcher composer's context header.
//
// Rendered by SimpleChatInput's `contextHeader` slot as the first row INSIDE
// the composer frame, like the "To:" line of an email:
//
//   项目 [icon] AgenTeam  │  运行环境 [icon] DeepSeek Harness
//   ─────────────────────────────────────────────────────────
//   输入消息…
//
// Why inside the composer: the previous floating chips below the input read as
// loose parts, and "AgenTeam" alone did not tell users it was the current
// project. Explicit labels plus placement inside the frame make the row read as
// "where this message goes". The two triggers have no chevrons; the name itself
// is the click target that opens the existing picker list.
//
// The labels use the meta tier (`text-xs`), the values the ui tier
// (`text-sm`), per DESIGN.md §3.3. A vertical hairline separates the two
// fields; the row's bottom hairline separates context from the message body.

import { memo } from 'react';
import { useTranslation } from 'react-i18next';

import RuntimeSelector from '@/components/RuntimeSelector';
import type { Project } from '@/config/types';
import type { RuntimeType, RuntimeDetections } from '../../../shared/types/runtime';

import WorkspaceSelector from './WorkspaceSelector';

interface LauncherInputContextRowProps {
  // Workspace
  projects: Project[];
  selectedProject: Project | null;
  defaultWorkspacePath?: string;
  onSelectWorkspace: (project: Project) => void;
  onAddFolder: () => void;
  /** Promote a project to default workspace via the dropdown's hover-only
   *  "设为默认" button. Threaded straight through to WorkspaceSelector. */
  onSetDefaultWorkspace?: (project: Project) => void;

  // Runtime (only rendered when distribution permits selection AND callers
  // supply onRuntimeChange).
  showRuntime: boolean;
  runtime?: RuntimeType;
  runtimeDetections?: RuntimeDetections;
  onRuntimeChange?: (runtime: RuntimeType) => void;
}

const LABEL_CLASS = 'shrink-0 text-xs text-[var(--ink-muted)]';

export default memo(function LauncherInputContextRow({
  projects,
  selectedProject,
  defaultWorkspacePath,
  onSelectWorkspace,
  onAddFolder,
  onSetDefaultWorkspace,
  showRuntime,
  runtime,
  runtimeDetections,
  onRuntimeChange,
}: LauncherInputContextRowProps) {
  const { t } = useTranslation('launcher');
  const { t: tChat } = useTranslation('chat');
  const projectLabel = t('workspaceSelector.label');
  const runtimeLabel = tChat('runtime.header');

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
      {showRuntime && runtime && runtimeDetections && onRuntimeChange && (
        <>
          <span
            data-testid="launcher-context-divider"
            aria-hidden="true"
            className="mx-3 h-3.5 w-px shrink-0 bg-[var(--line-strong)]"
          />
          <div role="group" aria-label={runtimeLabel} className="flex min-w-0 items-center gap-1">
            <span className={LABEL_CLASS}>{runtimeLabel}</span>
            <RuntimeSelector
              value={runtime}
              detections={runtimeDetections}
              onChange={onRuntimeChange}
              variant="launcher"
            />
          </div>
        </>
      )}
    </div>
  );
});
