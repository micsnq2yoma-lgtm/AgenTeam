/**
 * BrandSection - Left panel of the Launcher page
 * Layout: owns the whole Launcher page. A 对话 / 团队 ModeSegment is pinned to
 * the top edge as the page-level switch. In 对话, a three-row grid anchors the
 * composition to a fixed line at 42% of the page height: the brand sits on
 * that line and the input hangs from it. Neither moves when the input grows —
 * extra height only ever extends downward.
 *
 * 「团队」 replaces the composition with the pixel team office
 * (features/team-office): every project is a department and its employees
 * are its conversations. The composer stays mounted (hidden) so a draft
 * survives the switch.
 */

import { isImeComposingEvent } from '@/utils/imeKeyboard';
import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useTranslation } from 'react-i18next';

import { resolveSupportedLocale } from '../../../shared/i18n';
import { ProductWordmark } from '@/components/brand/ProductWordmark';
import DropZoneOverlay from '@/components/DropZoneOverlay';
import SimpleChatInput, {
  type ImageAttachment,
  type SimpleChatInputHandle,
} from '@/components/SimpleChatInput';
import CronTaskSettingsModal, {
  GOAL_SLASH_PRESET,
  type CronInitialConfig,
  type CronSettingsResult,
} from '@/components/cron/CronTaskSettingsModal';
import LauncherInputContextRow from './LauncherInputContextRow';
import type { RuntimeDetections } from '../../../shared/types/runtime';
import ModeSegment, {
  type InputMode,
} from '@/components/task-center/ModeSegment';
import { useToast } from '@/components/Toast';
import { track } from '@/analytics';
import { taskCenterAvailable } from '@/api/taskCenter';
import type { SessionMetadata } from '@/api/sessionClient';
import TeamOffice from '@/features/team-office/TeamOffice';
import { useFileDropZone } from '@/hooks/useFileDropZone';
import { useTauriFileDrop } from '@/hooks/useTauriFileDrop';
import { hasOverlayLayer } from '@/utils/closeLayer';
import { shouldShowBuiltinSdkSlashCommands } from '@/utils/runtimeUiProjection';
import { CUSTOM_EVENTS } from '@/../shared/constants';
import {
  type Project,
  type Provider,
  type PermissionMode,
  type ProviderVerifyStatus,
} from '@/config/types';
import type {
  RuntimeType,
  RuntimeModelInfo,
  RuntimePermissionMode,
} from '../../../shared/types/runtime';
import type {
  OfficialToolDefinition,
  OfficialToolId,
} from '../../../shared/official-tools';
import { useResolvedTheme } from '@/theme';

interface BrandSectionProps {
  // Workspace
  projects: Project[];
  selectedProject: Project | null;
  defaultWorkspacePath?: string;
  onSelectWorkspace: (project: Project) => void;
  onAddFolder: () => void;
  /** Promote a project to the global default workspace (writes
   *  `config.defaultWorkspacePath`). Threaded through to the chip row's
   *  WorkspaceSelector so its hover-only "设为默认" button has somewhere
   *  to write. */
  onSetDefaultWorkspace?: (project: Project) => void;
  // Input — `cron` carries the launcher-staged cron config forward so the
  // Launcher → InitialMessage handoff can include it (PRD 0.2.7 Cron handoff).
  onSend: (
    text: string,
    images?: ImageAttachment[],
    cron?: import('@/types/tab').InitialMessageCron,
    context?: import('../../../shared/agentMentions').QueryMentionContext,
  ) => void;
  recordingBusy?: boolean;
  /** Pending session id used for launcher-staged user attachments. */
  attachmentSessionId?: string | null;
  isStarting?: boolean;
  // Provider/Model (pass-through to SimpleChatInput)
  provider?: Provider | null;
  providers?: Provider[];
  selectedModel?: string;
  onProviderChange?: (id: string, targetModel?: string) => void;
  onModelChange?: (id: string) => void;
  /** #324 — 推理强度 (pass-through to SimpleChatInput) */
  reasoningEffort?: string;
  onReasoningEffortChange?: (effort: string) => void;
  permissionMode?: PermissionMode;
  onPermissionModeChange?: (mode: PermissionMode) => void;
  apiKeys?: Record<string, string>;
  providerVerifyStatus?: Record<string, ProviderVerifyStatus>;
  // MCP
  workspaceMcpEnabled?: string[];
  globalMcpEnabled?: string[];
  mcpServers?: Array<{ id: string; name: string; description?: string }>;
  onWorkspaceMcpToggle?: (serverId: string, enabled: boolean) => void;
  officialTools?: readonly OfficialToolDefinition[];
  workspaceOfficialToolEnabled?: OfficialToolId[];
  globalOfficialToolEnabled?: OfficialToolId[];
  officialToolNeedsConfig?: Partial<Record<OfficialToolId, boolean>>;
  onWorkspaceOfficialToolToggle?: (
    toolId: OfficialToolId,
    enabled: boolean,
  ) => void;
  // PRD 0.2.17 — Claude Plugins (passthrough to SimpleChatInput).
  globallyVisiblePlugins?: Array<{
    id: string;
    name: string;
    description?: string;
  }>;
  workspaceEnabledPlugins?: string[];
  onWorkspacePluginToggle?: (pluginId: string, enabled: boolean) => void;
  onRefreshProviders?: () => void;
  // Runtime (external runtimes adapt model/permission selectors)
  runtime?: RuntimeType;
  usesExternalRuntimeControls?: boolean;
  runtimeModels?: RuntimeModelInfo[];
  runtimePermissionModes?: RuntimePermissionMode[];
  // PRD 0.2.7 Phase F: runtime selector lives in the row below the input
  // when permitted by the build distribution. Caller provides the
  // detection map + onChange just like in chat-tab.
  runtimeSelectorAvailable?: boolean;
  runtimeDetections?: RuntimeDetections;
  onRuntimeChange?: (runtime: RuntimeType) => void;
  /** All runtimes (builtin + external) so the row's chip shows the full picture.
   *  Distinct from `runtime` which is the *external* runtime when in external mode. */
  activeRuntime?: RuntimeType;
  /** Only the visible Launcher tab may respond to window-wide Tauri drag events. */
  isActive: boolean;
}

const LAUNCHER_INPUT_DROP_ZONE_ID = 'launcher-input';

// AgenTeam brand: the slogan's key phrase carries a coral marker, echoing the
// coral T in the wordmark. Themes with custom slogans simply render plain text.
const SLOGAN_HIGHLIGHTS = ['真正有价值', 'Valuable Things'];

function HighlightedSlogan({ text }: { text: string }) {
  const phrase = SLOGAN_HIGHLIGHTS.find((p) => text.includes(p));
  if (!phrase) return <>{text}</>;
  const at = text.indexOf(phrase);
  return (
    <>
      {text.slice(0, at)}
      <em className="launcher-slogan-mark">{phrase}</em>
      {text.slice(at + phrase.length)}
    </>
  );
}

export default memo(function BrandSection({
  projects,
  selectedProject,
  defaultWorkspacePath,
  onSelectWorkspace,
  onAddFolder,
  onSetDefaultWorkspace,
  onSend,
  recordingBusy = false,
  attachmentSessionId,
  isStarting,
  provider,
  providers,
  selectedModel,
  onProviderChange,
  onModelChange,
  reasoningEffort,
  onReasoningEffortChange,
  permissionMode,
  onPermissionModeChange,
  apiKeys,
  providerVerifyStatus,
  workspaceMcpEnabled,
  globalMcpEnabled,
  mcpServers,
  onWorkspaceMcpToggle,
  officialTools,
  workspaceOfficialToolEnabled,
  globalOfficialToolEnabled,
  officialToolNeedsConfig,
  onWorkspaceOfficialToolToggle,
  globallyVisiblePlugins,
  workspaceEnabledPlugins,
  onWorkspacePluginToggle,
  onRefreshProviders,
  runtime,
  usesExternalRuntimeControls,
  runtimeModels,
  runtimePermissionModes,
  runtimeSelectorAvailable,
  runtimeDetections,
  onRuntimeChange,
  activeRuntime,
  isActive,
}: BrandSectionProps) {
  const { t, i18n } = useTranslation('launcher');
  const resolvedTheme = useResolvedTheme();
  const sloganLocale = resolveSupportedLocale(i18n.resolvedLanguage);
  const heroBackground = resolvedTheme.hero.background;
  const heroAssetCssUrl = heroBackground.assetUrl
    ? JSON.stringify(heroBackground.assetUrl)
    : null;
  const heroStyle = heroBackground.assetUrl
    ? {
        backgroundImage: heroBackground.mask
          ? `linear-gradient(${heroBackground.mask}, ${heroBackground.mask}), url(${heroAssetCssUrl})`
          : `url(${heroAssetCssUrl})`,
        backgroundPosition: heroBackground.position,
        backgroundSize: heroBackground.size,
        backgroundRepeat: heroBackground.repeat,
      }
    : undefined;
  const toast = useToast();
  // Project convention: keep `toast` behind a ref so it stays out of
  // useCallback dep arrays and doesn't re-trigger memoization (see
  // specs/tech_docs/react_stability_rules.md). Updated via effect to
  // satisfy the `react-hooks/refs` no-mutate-during-render rule.
  const toastRef = useRef(toast);
  const tRef = useRef(t);
  useEffect(() => {
    toastRef.current = toast;
  }, [toast]);
  useEffect(() => {
    tRef.current = t;
  }, [t]);
  const [mode, setMode] = useState<InputMode>('chat');
  // PRD 0.2.7 D1 + C6: cron settings staged in the launcher. The actual
  // `cmd_create_cron_task` does NOT run here — we only collect the params,
  // then carry them forward to Chat via InitialMessage.cron at send time.
  // Keeps "user closed launcher mid-edit → no orphan cron" the default.
  const [showCronSettings, setShowCronSettings] = useState(false);
  const [stagedCron, setStagedCron] = useState<CronSettingsResult | null>(null);
  const [cronOpenPreset, setCronOpenPreset] =
    useState<CronInitialConfig | null>(null);
  // Gracefully degrade in browser dev mode — ModeSegment is Tauri-only.
  const modeSegmentEnabled = taskCenterAvailable();

  // `runtime` is present for user-managed external CLIs. Managed Codex keeps
  // provider-style input chrome, so its execution Runtime comes from the
  // selected Provider instead. Slash capabilities must follow that execution
  // identity, not the chrome projection.
  const inputExecutionRuntime: RuntimeType =
    runtime ??
    (provider?.execution?.kind === 'runtime-backed'
      ? provider.execution.runtime
      : 'builtin');

  // Ref for imperative focus. The composer stays mounted (hidden via CSS
  // in 团队) so typed-but-not-yet-sent text survives mode switches.
  // Focus is driven by a mode-effect below so the caret returns to the
  // composer whenever 对话 is shown again.
  //
  // ModeSegment buttons use `retainFocusOnMouseDown` (see
  // `utils/focusRetention.ts`) so the textarea never loses focus on
  // click in the first place — no rAF, no race with macOS WebKit
  // touchpad-tap synthesis. The effect below handles the programmatic
  // hand-off when a keyboard chord (Tab / Cmd+Shift+T) switches modes.
  const inputRef = useRef<SimpleChatInputHandle>(null);
  const inputDropZoneRef = useRef<HTMLDivElement>(null);
  const canAcceptFileDrop = isActive && mode === 'chat';

  const handleDroppedFiles = useCallback((files: File[]) => {
    void inputRef.current?.processDroppedFiles(files);
  }, []);
  const handleDroppedFilePaths = useCallback((paths: string[]) => {
    void inputRef.current?.processDroppedFilePaths?.(paths);
  }, []);
  const {
    isDragActive: isHtmlFileDragActive,
    dragHandlers: htmlFileDragHandlers,
    resetDragState: resetHtmlFileDragState,
  } = useFileDropZone({ onFilesDropped: handleDroppedFiles });
  const {
    isDragging: isNativeFileDragging,
    activeZoneId: nativeDropZoneId,
    registerZone,
    unregisterZone,
  } = useTauriFileDrop({ enabled: canAcceptFileDrop });

  useLayoutEffect(() => {
    registerZone(
      LAUNCHER_INPUT_DROP_ZONE_ID,
      inputDropZoneRef.current,
      handleDroppedFilePaths,
    );
    return () => unregisterZone(LAUNCHER_INPUT_DROP_ZONE_ID);
  }, [handleDroppedFilePaths, registerZone, unregisterZone]);

  useEffect(() => {
    if (canAcceptFileDrop) return;
    resetHtmlFileDragState();
  }, [canAcceptFileDrop, resetHtmlFileDragState]);

  const showInputDropOverlay =
    canAcceptFileDrop &&
    (isHtmlFileDragActive ||
      (isNativeFileDragging &&
        nativeDropZoneId === LAUNCHER_INPUT_DROP_ZONE_ID));
  // Single helper for BOTH the segment click path (explicit mode) and
  // the keyboard paths (Tab / Cmd+Shift+T toggle). Without this the
  // two call sites diverged on `setMode(next)` vs `setMode((m) => …)`
  // and future work added to one would silently skip the other.
  //
  // `modeRef` mirrors `mode` so we can read the current value outside
  // a setState updater — track() fires once per real switch, even though
  // React strict-mode invokes the updater function twice.
  const modeRef = useRef<InputMode>(mode);
  useEffect(() => {
    modeRef.current = mode;
  }, [mode]);
  const setModeAndFocus = useCallback((next: InputMode | 'toggle') => {
    const prev = modeRef.current;
    const resolved: InputMode =
      next === 'toggle' ? (prev === 'chat' ? 'team' : 'chat') : next;
    if (resolved !== prev) {
      track('launcher_mode_switch', {
        to: resolved,
        via: next === 'toggle' ? 'shortcut' : 'click',
      });
    }
    if (next === 'toggle') {
      setMode((m) => (m === 'chat' ? 'team' : 'chat'));
    } else {
      setMode(next);
    }
  }, []);

  // Focus handoff — follows the visible input. Fires on mount (initial
  // mode) and every mode change; `retainFocusOnMouseDown` already keeps
  // focus on the correct textarea for mouse-driven ModeSegment clicks,
  // so this effect's job is the keyboard-chord path (Tab / Cmd+Shift+T).
  // Running in an effect keeps it out of the click-handler frame — no
  // touchpad-tap race.
  useEffect(() => {
    if (mode === 'chat') inputRef.current?.focus();
  }, [mode]);

  // PRD 0.2.7 D3: switching workspaces in the launcher invalidates any
  // workspace-bound draft state — `@myagents_files/...` references point to
  // files in the previous workspace's `myagents_files/`, `images[]`
  // captured via Tauri drag-drop / copyPaths similarly belong to the prior
  // tree, and a staged cron task that referenced those files would now
  // execute against an inconsistent prompt. Strip them silently and surface
  // a toast (text outside references survives, so this is a soft reset).
  // Cross-review (CC) caught the stagedCron-not-cleared subtle UX trap.
  const lastWorkspacePathRef = useRef<string | null>(
    selectedProject?.path ?? null,
  );
  useEffect(() => {
    const next = selectedProject?.path ?? null;
    if (lastWorkspacePathRef.current === next) return;
    // Skip the first run (initial selection — no draft to clear yet).
    if (lastWorkspacePathRef.current !== null) {
      const result = inputRef.current?.clearWorkspaceBoundDraft();
      const total =
        (result?.strippedReferences ?? 0) + (result?.clearedImages ?? 0);
      const hadCron = stagedCron !== null;
      if (hadCron) setStagedCron(null);
      if (total > 0 || hadCron) {
        toastRef.current.info(
          tRef.current('toasts.workspaceAttachmentsCleared'),
        );
      }
    }
    lastWorkspacePathRef.current = next;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- stagedCron read intentionally; we don't want this effect to re-fire when the user merely stages a new cron, only when workspace changes
  }, [selectedProject?.path]);

  // Dialogue-mode submit is a straight pass-through to the parent's `onSend`.
  const handleSend = useCallback(
    (text: string, images?: ImageAttachment[], _permissionMode?: PermissionMode, context?: import('../../../shared/agentMentions').QueryMentionContext) => {
      if (recordingBusy) return;
      // Repackage staged cron config into the InitialMessageCron shape so
      // Chat's autoSend can dispatch to startCronTask without poking back
      // into the modal's `CronSettingsResult` schema. Schedule fallback
      // to a fixed-interval shape preserves the modal's plain-interval
      // config (it leaves `schedule` undefined when the user picks the
      // simple cadence dial).
      const cron = stagedCron
        ? {
            taskKind: stagedCron.taskKind,
            schedule:
              stagedCron.schedule ??
              ({ kind: 'every', minutes: stagedCron.intervalMinutes } as const),
            runMode: stagedCron.runMode,
            endConditions: stagedCron.endConditions,
            notifyEnabled: stagedCron.notifyEnabled,
            delivery: stagedCron.delivery,
            name: stagedCron.name,
            intervalMinutes: stagedCron.intervalMinutes,
            // Carry executionTarget through to Launcher (which
            // short-circuits on `new_task` to create a background
            // task) and to the chat-side cron state (so re-opening
            // the editor shows the user's actual choice, not the
            // 'current_session' default).
            executionTarget: stagedCron.executionTarget,
          }
        : undefined;
      onSend(text, images, cron, context);
    },
    [onSend, recordingBusy, stagedCron],
  );

  // Cron config for the StatusBar — derived from staged (immutable while
  // dialog is closed). Built per the SimpleChatInput contract: { intervalMinutes, schedule? }.
  const cronStatusBarConfig = useMemo(
    () =>
      stagedCron
        ? {
            taskKind: stagedCron.taskKind,
            intervalMinutes: stagedCron.intervalMinutes,
            schedule: stagedCron.schedule,
          }
        : null,
    [stagedCron],
  );

  const handleOpenCronSettings = useCallback(() => {
    setCronOpenPreset(null);
    setShowCronSettings(true);
  }, []);

  const handleCloseCronSettings = useCallback(() => {
    setShowCronSettings(false);
    setCronOpenPreset(null);
  }, []);

  const handleConfirmCronSettings = useCallback(
    (config: CronSettingsResult) => {
      setStagedCron(config);
      setShowCronSettings(false);
      setCronOpenPreset(null);
      track('launcher_cron_stage', {
        interval_minutes: config.intervalMinutes,
        run_mode: config.runMode,
        execution_target: config.executionTarget,
      });
    },
    [],
  );

  const handleCancelCronDraft = useCallback(() => {
    setStagedCron(null);
  }, []);

  const handleSlashAction = useCallback((name: string) => {
    if (name !== 'goal' && name !== 'loop') return;
    setCronOpenPreset(GOAL_SLASH_PRESET);
    setShowCronSettings(true);
  }, []);

  // 团队: an employee opens its conversation through the same App-owned
  // new-or-jump path as every other surface; hiring switches back to 对话
  // with that project selected so the next message starts the employee.
  const handleOpenEmployee = useCallback((session: SessionMetadata, project: Project) => {
    window.dispatchEvent(new CustomEvent(CUSTOM_EVENTS.OPEN_SESSION_IN_NEW_TAB, {
      detail: { sessionId: session.id, workspacePath: project.path },
    }));
  }, []);
  const handleHireEmployee = useCallback((project: Project) => {
    onSelectWorkspace(project);
    setModeAndFocus('chat');
  }, [onSelectWorkspace, setModeAndFocus]);

  // Scoping ref for the Tab handler below — we only hijack Tab when the
  // focus is inside this Launcher subtree, so Chat tabs / settings /
  // modals keep their native focus navigation.
  const sectionRef = useRef<HTMLElement | null>(null);

  // PRD §4.1.1 hotkeys:
  //   • Cmd/Ctrl+Shift+T toggles mode globally while the Launcher is
  //     mounted — an explicit chord, safe to listen on `window`.
  //   • Plain Tab toggles too, and — crucially — fires even when the
  //     textarea is focused. The tooltip on the ModeSegment buttons
  //     ("按 Tab 切换到「对话」") promises this behaviour; guarding
  //     against editable targets like earlier iterations did made the
  //     tooltip a lie the moment mount-time focus landed the caret in
  //     the textarea. Child components that legitimately need to
  //     consume Tab (SimpleChatInput's slash-menu / file-search
  //     autocomplete) call `event.stopPropagation()` inside their
  //     onKeyDown handlers — React's `stopPropagation` halts the
  //     underlying native bubble, so this window listener truly won't
  //     fire when a child has first claim on the Tab keystroke.
  useEffect(() => {
    if (!modeSegmentEnabled) return;
    const handler = (e: KeyboardEvent) => {
      if (isImeComposingEvent(e)) return;
      if (
        (e.metaKey || e.ctrlKey) &&
        e.shiftKey &&
        (e.key === 'T' || e.key === 't')
      ) {
        e.preventDefault();
        setModeAndFocus('toggle');
        return;
      }
      if (
        e.key !== 'Tab' ||
        e.metaKey ||
        e.ctrlKey ||
        e.altKey ||
        e.shiftKey ||
        hasOverlayLayer()
      ) {
        return;
      }
      const section = sectionRef.current;
      const target = e.target as Node | null;
      const inScope =
        !target ||
        target === document.body ||
        (section?.contains(target) ?? false);
      if (!inScope) return;
      e.preventDefault();
      setModeAndFocus('toggle');
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [modeSegmentEnabled, setModeAndFocus]);

  return (
    <section
      ref={sectionRef}
      className="theme-launcher-hero relative grid h-full w-full grid-rows-[minmax(10rem,42%)_auto_minmax(0,1fr)] justify-items-center px-12"
      style={heroStyle}
      data-theme-hero={resolvedTheme.themeId}
    >
      {/* Page-level mode switch: 对话 / 团队 (see DESIGN.md §5.6, PRD §4.1).
          Out of flow so it never takes a grid row. */}
      {modeSegmentEnabled && (
        <div className="absolute inset-x-0 top-6 z-10 flex justify-center">
          <ModeSegment
            value={mode}
            onChange={setModeAndFocus}
            disabled={recordingBusy}
          />
        </div>
      )}

      {/* Row 1 — brand group, bottom-aligned on the anchor line; `pb-8`
          separates "who we are" from the input below. */}
      <div
        className={`flex min-h-0 flex-col items-center justify-end pb-8 ${mode === 'team' ? 'hidden' : ''}`}
        aria-hidden={mode === 'team'}
      >
        <h1 className="theme-product-wordmark theme-launcher-hero-title">
          <ProductWordmark name={resolvedTheme.hero.productName} showMark={false} />
        </h1>
        {/* 品牌 slogan 的 15px/17px 是 DESIGN.md §3.3 立档的展示型字号（display 用途），
                    不属于正文 Type Scale；这是全仓唯一豁免点（PRD 0.2.34）。 */}
        <p className="theme-launcher-hero-slogan">
          <HighlightedSlogan text={resolvedTheme.hero.slogans[sloganLocale]} />
        </p>
      </div>

      {/* Row 2 — input, top edge on the anchor line. The context row is
          absolute-positioned below it so it hangs in row 3 without affecting
          the anchor (PRD §4.2). Hidden, not unmounted, in 团队. */}
      <div
        className={`w-full max-w-[640px] ${mode === 'team' ? 'hidden' : ''}`}
        aria-hidden={mode === 'team'}
        inert={mode === 'team'}
      >
        <div className="relative w-full">
          <div className="grid *:col-start-1 *:row-start-1">
            <div
              ref={inputDropZoneRef}
              className="relative"
              {...(canAcceptFileDrop ? htmlFileDragHandlers : {})}
            >
              <SimpleChatInput
                ref={inputRef}
                mode="launcher"
                active={canAcceptFileDrop}
                onSend={handleSend}
                isLoading={!!isStarting || recordingBusy}
                provider={provider}
                providers={providers}
                selectedModel={selectedModel}
                onProviderChange={onProviderChange}
                onModelChange={onModelChange}
                reasoningEffort={reasoningEffort}
                onReasoningEffortChange={onReasoningEffortChange}
                permissionMode={permissionMode}
                onPermissionModeChange={onPermissionModeChange}
                /* PRD 0.2.7: workspace_files invokes need a path; selectedProject
                 * is the launcher's "current workspace" from WorkspaceSelector. */
                workspacePath={selectedProject?.path ?? null}
                sessionId={attachmentSessionId}
                /* PRD 0.2.7 cron staging — StatusBar shows iff a config is staged. */
                cronModeEnabled={stagedCron !== null}
                cronConfig={cronStatusBarConfig}
                onCronButtonClick={handleOpenCronSettings}
                onCronSettings={handleOpenCronSettings}
                onCronCancel={handleCancelCronDraft}
                onSlashAction={handleSlashAction}
                showBuiltinSdkSlashCommands={shouldShowBuiltinSdkSlashCommands(
                  inputExecutionRuntime,
                )}
                apiKeys={apiKeys}
                providerVerifyStatus={providerVerifyStatus}
                workspaceMcpEnabled={workspaceMcpEnabled}
                globalMcpEnabled={globalMcpEnabled}
                mcpServers={mcpServers}
                onWorkspaceMcpToggle={onWorkspaceMcpToggle}
                officialTools={officialTools}
                workspaceOfficialToolEnabled={workspaceOfficialToolEnabled}
                globalOfficialToolEnabled={globalOfficialToolEnabled}
                officialToolNeedsConfig={officialToolNeedsConfig}
                onWorkspaceOfficialToolToggle={onWorkspaceOfficialToolToggle}
                globallyVisiblePlugins={globallyVisiblePlugins}
                workspaceEnabledPlugins={workspaceEnabledPlugins}
                onWorkspacePluginToggle={onWorkspacePluginToggle}
                onRefreshProviders={onRefreshProviders}
                runtime={runtime}
                usesExternalRuntimeControls={usesExternalRuntimeControls}
                runtimeModels={runtimeModels}
                runtimePermissionModes={runtimePermissionModes}
                /* PRD 0.2.7 Phase F: workspace + runtime selectors moved out of
                 * the toolbar to the row below — toolbarPrefix dropped here. */
              />
              <DropZoneOverlay isVisible={showInputDropOverlay} />
            </div>
          </div>
          {/* PRD 0.2.7 Phase F: launcher-only chip row that surfaces
           *  the project + Runtime below the composer. */}
          {mode === 'chat' && (
            <div className="launcher-below-input-stack absolute left-0 right-0 top-full mt-2 flex flex-col gap-4">
              <div className="w-full">
                <LauncherInputContextRow
                  projects={projects}
                  selectedProject={selectedProject}
                  defaultWorkspacePath={defaultWorkspacePath}
                  onSelectWorkspace={onSelectWorkspace}
                  onAddFolder={onAddFolder}
                  onSetDefaultWorkspace={onSetDefaultWorkspace}
                  showRuntime={!!runtimeSelectorAvailable}
                  runtime={activeRuntime}
                  runtimeDetections={runtimeDetections}
                  onRuntimeChange={onRuntimeChange}
                />
              </div>
            </div>
          )}
        </div>
      </div>

      {mode === 'team' && (
        <div className="col-start-1 row-start-1 row-end-4 flex min-h-0 w-full flex-col pb-6 pt-20" data-launcher-team>
          <TeamOffice
            projects={projects}
            onOpenEmployee={handleOpenEmployee}
            onHireEmployee={handleHireEmployee}
          />
        </div>
      )}

      {/* PRD 0.2.7 D1: launcher cron settings modal — confirming stages
       *  the config locally; the actual cron task is created by Chat
       *  after handoff. We pass `workspacePath` so the modal can
       *  populate workspace-relative defaults the same way it does in
       *  the chat tab. */}
      <CronTaskSettingsModal
        isOpen={showCronSettings}
        onClose={handleCloseCronSettings}
        initialPrompt=""
        initialConfig={cronOpenPreset ?? stagedCron ?? undefined}
        workspacePath={selectedProject?.path ?? ''}
        onConfirm={handleConfirmCronSettings}
      />
    </section>
  );
});
