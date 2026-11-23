/**
 * BrandSection - Left panel of the Launcher page
 * Layout: owns the whole Launcher page. A three-row grid anchors the
 * composition to a fixed line at 42% of the page height: the brand sits on
 * that line and the input hangs from it. Neither moves when the input grows —
 * extra height only ever extends downward.
 *
 * The pixel team office is its own page (sidebar → 智能体团队), not a mode here.
 */

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
import LauncherRuntimeToolbarControl from './LauncherRuntimeToolbarControl';
import type { RuntimeDetections } from '../../../shared/types/runtime';
import { useToast } from '@/components/Toast';
import { track } from '@/analytics';
import { useFileDropZone } from '@/hooks/useFileDropZone';
import { useTauriFileDrop } from '@/hooks/useTauriFileDrop';
import { shouldShowBuiltinSdkSlashCommands } from '@/utils/runtimeUiProjection';
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
  runtimeModelsLoading?: boolean;
  runtimeModelsError?: boolean;
  onRuntimeModelsRetry?: () => void;
  runtimePermissionModes?: RuntimePermissionMode[];
  // PRD 0.2.7 Phase F: runtime selector lives in the row below the input
  // when permitted by the build distribution. Caller provides the
  // detection map + onChange just like in chat-tab.
  runtimeSelectorAvailable?: boolean;
  runtimeDetections?: RuntimeDetections;
  runtimeDetecting?: boolean;
  runtimeDetectionFailed?: boolean;
  onRuntimeDetectAgain?: () => void;
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
  runtimeModelsLoading,
  runtimeModelsError,
  onRuntimeModelsRetry,
  runtimePermissionModes,
  runtimeSelectorAvailable,
  runtimeDetections,
  runtimeDetecting,
  runtimeDetectionFailed,
  onRuntimeDetectAgain,
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
  // PRD 0.2.7 D1 + C6: cron settings staged in the launcher. The actual
  // `cmd_create_cron_task` does NOT run here — we only collect the params,
  // then carry them forward to Chat via InitialMessage.cron at send time.
  // Keeps "user closed launcher mid-edit → no orphan cron" the default.
  const [showCronSettings, setShowCronSettings] = useState(false);
  const [stagedCron, setStagedCron] = useState<CronSettingsResult | null>(null);
  const [cronOpenPreset, setCronOpenPreset] =
    useState<CronInitialConfig | null>(null);

  // `runtime` is present for user-managed external CLIs. Managed Codex keeps
  // provider-style input chrome, so its execution Runtime comes from the
  // selected Provider instead. Slash capabilities must follow that execution
  // identity, not the chrome projection.
  const inputExecutionRuntime: RuntimeType =
    runtime ??
    (provider?.execution?.kind === 'runtime-backed'
      ? provider.execution.runtime
      : 'builtin');

  // Ref for imperative focus; the composer takes the caret on mount.
  const inputRef = useRef<SimpleChatInputHandle>(null);
  const inputDropZoneRef = useRef<HTMLDivElement>(null);
  const canAcceptFileDrop = isActive;

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

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

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

  // Memoized so the memo'd SimpleChatInput does not re-render on every
  // BrandSection render just because a fresh element was created.
  const contextHeader = useMemo(() => (
    <LauncherInputContextRow
      projects={projects}
      selectedProject={selectedProject}
      defaultWorkspacePath={defaultWorkspacePath}
      onSelectWorkspace={onSelectWorkspace}
      onAddFolder={onAddFolder}
      onSetDefaultWorkspace={onSetDefaultWorkspace}
    />
  ), [
    projects,
    selectedProject,
    defaultWorkspacePath,
    onSelectWorkspace,
    onAddFolder,
    onSetDefaultWorkspace,
  ]);

  // Runtime picker sits in the toolbar next to Tools; same memo rationale.
  const runtimeToolbarControl = useMemo(() => (
    runtimeSelectorAvailable && activeRuntime && runtimeDetections && onRuntimeChange ? (
      <LauncherRuntimeToolbarControl
        runtime={activeRuntime}
        runtimeDetections={runtimeDetections}
        runtimeDetecting={runtimeDetecting}
        runtimeDetectionFailed={runtimeDetectionFailed}
        onRuntimeDetectAgain={onRuntimeDetectAgain}
        onRuntimeChange={onRuntimeChange}
      />
    ) : null
  ), [runtimeSelectorAvailable, activeRuntime, runtimeDetections, runtimeDetecting, runtimeDetectionFailed, onRuntimeDetectAgain, onRuntimeChange]);

  const handleSlashAction = useCallback((name: string) => {
    if (name !== 'goal' && name !== 'loop') return;
    setCronOpenPreset(GOAL_SLASH_PRESET);
    setShowCronSettings(true);
  }, []);


  return (
    <section
      className="theme-launcher-hero relative grid h-full w-full grid-rows-[minmax(10rem,42%)_auto_minmax(0,1fr)] justify-items-center px-12"
      style={heroStyle}
      data-theme-hero={resolvedTheme.themeId}
    >
      {/* Row 1 — brand group, bottom-aligned on the anchor line; `pb-8`
          separates "who we are" from the input below. */}
      <div
        className="flex min-h-0 flex-col items-center justify-end pb-8"
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

      {/* Row 2 — input, top edge on the anchor line. Its first inner row is
          the project/runtime context header; extra height extends downward. */}
      <div className="w-full max-w-[640px]">
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
                runtimeModelsLoading={runtimeModelsLoading}
                runtimeModelsError={runtimeModelsError}
                onRuntimeModelsRetry={onRuntimeModelsRetry}
                runtimePermissionModes={runtimePermissionModes}
                /* Project lives in the composer's context header (first row
                 * inside the frame); runtime sits in the toolbar after Tools. */
                contextHeader={contextHeader}
                toolbarSuffix={runtimeToolbarControl}
              />
              <DropZoneOverlay isVisible={showInputDropOverlay} />
            </div>
          </div>
        </div>
      </div>

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
