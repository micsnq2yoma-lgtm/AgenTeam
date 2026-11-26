import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import { useContext } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { SessionMetadata } from '@/api/sessionClient';
import { SessionDeletionContext } from '@/context/SessionDeletionContext';
import { renderWithTheme } from '@/test/renderWithTheme';
import {
  chatComposerDraftKey,
  clearAllComposerDrafts,
  createSessionDraftKey,
  getComposerDraft,
  LAUNCHER_DRAFT_KEY,
} from '@/components/chat-input/composerDraftStore';
import SimpleChatInput from '@/components/SimpleChatInput';
import { isPendingSessionId } from '../shared/constants';
import type { InitialMessage } from '@/types/tab';

const tauriCoreMocks = vi.hoisted(() => ({
  invoke: vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(
    async () => undefined,
  ),
}));

vi.mock('@tauri-apps/api/core', () => tauriCoreMocks);

const mocks = vi.hoisted(() => {
  const project = {
    id: 'proj-mino',
    name: 'Mino',
    path: '/mock/Mino',
    displayName: 'Mino',
    agentId: 'agent-mino',
  };
  const agent = {
    id: 'agent-mino',
    name: 'Mino',
    workspacePath: project.path,
    providerId: 'provider-1',
    model: 'mimo-v2.5-pro',
    runtime: 'builtin',
    permissionMode: 'auto',
    reasoningEffort: 'default',
  };
  const provider = {
    id: 'provider-1',
    name: 'Provider',
    type: 'openai-compatible',
    config: { baseUrl: 'https://example.com' },
    primaryModel: 'mimo-v2.5-pro',
    models: [{ model: 'mimo-v2.5-pro', modelName: 'mimo-v2.5-pro', modelSeries: 'mimo' }],
  };

  return {
    project,
    agent,
    provider,
    deleteTargetSessionId: null as string | null,
    deleteResults: [] as Array<{ deleted: boolean; reason?: string }>,
    deleteSession: vi.fn(async () => ({ deleted: true as const })),
    ensureSessionSidecar: vi.fn(async () => ({ port: 31417, isNew: true })),
    releaseTabSession: vi.fn(async () => false),
    reconcileSessionTabActivation: vi.fn(async () => true),
    upgradeSessionId: vi.fn(async () => true),
    hasSessionSidecar: vi.fn(async () => true),
    getSessionGeneration: vi.fn(async () => 1 as number | null),
    startBackgroundCompletion: vi.fn(async () => ({ started: false, sessionId: 'session' })),
    querySessionHasPersistentOwners: vi.fn(async () => false),
    sessionHasPersistentOwners: vi.fn(async () => false),
    sidebarProps: [] as Array<Record<string, unknown>>,
    latestChatTabProvider: null as {
      tabId: string;
      sessionId: string | null;
      onSessionIdChange: (newSessionId: string) => Promise<boolean>;
    } | null,
    toast: {
      error: vi.fn(),
      success: vi.fn(),
      warning: vi.fn(),
      info: vi.fn(),
    },
  };
});

vi.mock('@/analytics', () => ({
  initAnalytics: vi.fn(async () => undefined),
  track: vi.fn(),
  setAnalyticsContext: vi.fn(),
  clearAnalyticsContext: vi.fn(),
  setPendingSurface: vi.fn(),
  clearPendingSurface: vi.fn(),
  setPendingSessionBirth: vi.fn(),
  clearPendingSessionBirth: vi.fn(),
  consumePendingSessionBirth: vi.fn(),
  peekPendingSessionBirth: vi.fn(),
  birthContextForSurface: vi.fn((surface: string) => ({
    surface,
    entryIntent: surface === 'new_chat_button' ? 'new_chat' : 'unknown',
    hasInitialMessage: surface !== 'new_chat_button',
  })),
  hashAgentName: vi.fn(async () => 'agent-hash'),
  hashAgentNameSync: vi.fn(() => 'agent-hash'),
}));

vi.mock('@/api/tauriClient', () => ({
  stopTabSidecar: vi.fn(async () => undefined),
  setAppActiveCorrelation: vi.fn(),
  startGlobalSidecar: vi.fn(async () => undefined),
  initGlobalSidecarReadyPromise: vi.fn(),
  markGlobalSidecarReady: vi.fn(),
  getGlobalServerUrl: vi.fn(async () => 'http://127.0.0.1:31415'),
  getSessionActivation: vi.fn(async () => null),
  updateSessionTab: vi.fn(async () => undefined),
  ensureSessionSidecar: mocks.ensureSessionSidecar,
  releaseTabSession: mocks.releaseTabSession,
  activateSession: vi.fn(async () => undefined),
  reconcileSessionTabActivation: mocks.reconcileSessionTabActivation,
  upgradeSessionId: mocks.upgradeSessionId,
  getSessionPort: vi.fn(async () => null),
  getTabServerUrl: vi.fn(async () => 'http://127.0.0.1:31417'),
  sessionSidecarFetch: vi.fn(),
  isTauri: () => false,
  resetTabServerUrlCache: vi.fn(),
  setActiveCorrelation: vi.fn(),
  hasSessionSidecar: mocks.hasSessionSidecar,
  getSessionGeneration: mocks.getSessionGeneration,
  stopSseProxy: vi.fn(async () => undefined),
  startBackgroundCompletion: mocks.startBackgroundCompletion,
  startBackgroundCompletionForDeletion: mocks.startBackgroundCompletion,
  cancelBackgroundCompletion: vi.fn(async () => undefined),
  updateGlobalServerUrl: vi.fn(),
  canRestoreSession: vi.fn(async () => true),
  getUserSchedulerLifecycleSnapshot: vi.fn(async () => ({ runningTaskCount: 0, deleteProtectedSessionIds: [] })),
  querySessionHasPersistentOwners: mocks.querySessionHasPersistentOwners,
  sessionHasPersistentOwners: mocks.sessionHasPersistentOwners,
}));

vi.mock('@/api/apiFetch', () => ({
  apiGetJson: vi.fn(async () => ({ success: true })),
}));

vi.mock('@/api/cronTaskClient', () => ({
  getAllCronTasks: vi.fn(async () => []),
  getTabCronTask: vi.fn(async () => null),
  updateCronTaskTab: vi.fn(async () => undefined),
  createCronTask: vi.fn(),
  startCronTask: vi.fn(),
}));

vi.mock('@/api/sessionClient', () => ({
  createSession: vi.fn(async () => ({
    id: 'prepared-session',
    agentDir: mocks.project.path,
    title: 'Prepared',
    createdAt: '2026-06-27T00:00:00.000Z',
    lastActiveAt: '2026-06-27T00:00:00.000Z',
  })),
  deleteSession: mocks.deleteSession,
  getSessions: vi.fn(async () => []),
  updateSession: vi.fn(async () => undefined),
}));

vi.mock('@/api/taskCenter', () => ({
  recordGet: vi.fn(async () => null),
}));

vi.mock('@/api/recording', () => ({
  recordingSnapshot: vi.fn(async () => null),
  recordingStart: vi.fn(),
  recordingStop: vi.fn(),
}));

vi.mock('@/components/ChatBootOverlay', () => ({
  default: () => <div data-testid="chat-boot-overlay" />,
}));

vi.mock('@/components/ConfirmDialog', () => ({
  default: () => <div data-testid="confirm-dialog" />,
}));

vi.mock('@/components/BugReportOverlay', () => ({
  default: () => <div data-testid="bug-report-overlay" />,
}));

vi.mock('@/components/CustomTitleBar', () => ({
  default: ({ children }: { children?: React.ReactNode }) => <div data-testid="titlebar">{children}</div>,
}));

vi.mock('@/components/LinkContextMenuProvider', () => ({
  default: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

vi.mock('@/context/ImagePreviewContext', () => ({
  useImagePreview: () => ({ openPreview: vi.fn() }),
}));

vi.mock('@/components/global-sidebar/GlobalSidebar', () => ({
  default: function MockGlobalSidebar(props: Record<string, unknown>) {
    mocks.sidebarProps.push(props);
    const deleteSession = useContext(SessionDeletionContext);
    const onNewTab = props.onNewTab as (() => void) | undefined;
    const onOpenTeamOffice = props.onOpenTeamOffice as (() => void) | undefined;
    const onOpenSettings = props.onOpenSettings as (() => void) | undefined;
    const onOpenWorkspace = props.onOpenWorkspace as (
      (project: typeof mocks.project, initialMessage?: InitialMessage) => Promise<boolean>
    ) | undefined;
    const onOpenSession = props.onOpenSession as (
      (session: SessionMetadata, project: typeof mocks.project) => Promise<boolean>
    ) | undefined;

    const makeSession = (id: string): SessionMetadata => ({
      id,
      agentDir: mocks.project.path,
      title: id,
      createdAt: '2026-06-27T00:00:00.000Z',
      lastActiveAt: '2026-06-27T00:00:00.000Z',
    });

    return (
      <aside data-testid="global-sidebar">
        <button data-testid="sidebar-new-chat" onClick={() => onNewTab?.()}>new-chat</button>
        <button data-testid="sidebar-team-office" onClick={() => onOpenTeamOffice?.()}>team-office</button>
        <button data-testid="sidebar-settings" onClick={() => onOpenSettings?.()}>settings</button>
        <button
          data-testid="sidebar-open-workspace"
          onClick={() => { void onOpenWorkspace?.(mocks.project); }}
        >
          open-workspace
        </button>
        <button
          data-testid="sidebar-open-workspace-with-msg"
          onClick={() => {
            void onOpenWorkspace?.(mocks.project, { text: '来自侧边栏的初始消息' });
          }}
        >
          open-workspace-with-msg
        </button>
        <button
          data-testid="sidebar-open-session-a"
          onClick={() => { void onOpenSession?.(makeSession('real-session-a'), mocks.project); }}
        >
          open-session-a
        </button>
        <button
          data-testid="sidebar-open-session-b"
          onClick={() => { void onOpenSession?.(makeSession('real-session-b'), mocks.project); }}
        >
          open-session-b
        </button>
        <button
          data-testid="sidebar-open-born-session"
          onClick={() => { void onOpenSession?.(makeSession('born-session-1'), mocks.project); }}
        >
          open-born-session
        </button>
        <button
          data-testid="sidebar-delete-session"
          onClick={() => {
            if (!mocks.deleteTargetSessionId || !deleteSession) return;
            void deleteSession(mocks.deleteTargetSessionId).then((result) => {
              mocks.deleteResults.push(result);
            });
          }}
        >
          delete-session
        </button>
      </aside>
    );
  },
}));

vi.mock('@/features/team-office/TeamOfficePage', () => ({
  default: () => <div data-testid="team-office-page" />,
}));

vi.mock('@/pages/Settings', () => ({
  default: () => <div data-testid="settings-page" />,
}));

vi.mock('@/pages/TaskCenter', () => ({
  default: () => <div data-testid="taskcenter-page" />,
}));

vi.mock('@/features/knowledge/KnowledgeHome', () => ({
  default: () => <div data-testid="knowledge-home" />,
}));

// Thin Chat test shell that mirrors Chat.tsx's exact draftKey prop contract
// around the real SimpleChatInput without booting the full 7.8k-LOC Chat page.
vi.mock('@/pages/Chat', () => ({
  default: function ChatDraftShell() {
    const tab = mocks.latestChatTabProvider;
    const sessionId = tab?.sessionId ?? null;
    return (
      <div data-testid="chat-page" data-session-id={sessionId ?? ''}>
        <SimpleChatInput
          mode="chat"
          isLoading={false}
          onSend={vi.fn(async () => true)}
          provider={mocks.provider as never}
          providers={[mocks.provider as never]}
          providerAvailable
          selectedModel="mimo-v2.5-pro"
          workspacePath={mocks.project.path}
          sessionId={sessionId}
          draftKey={chatComposerDraftKey(sessionId)}
        />
      </div>
    );
  },
}));

vi.mock('@/context/TabProvider', () => ({
  default: ({
    tabId,
    sessionId,
    onSessionIdChange,
    children,
  }: {
    tabId: string;
    sessionId: string | null;
    onSessionIdChange: (newSessionId: string) => Promise<boolean>;
    children: React.ReactNode;
  }) => {
    mocks.latestChatTabProvider = { tabId, sessionId, onSessionIdChange };
    return (
      <div data-testid="tab-provider" data-tab-id={tabId} data-session-id={sessionId ?? ''}>
        {children}
      </div>
    );
  },
}));

vi.mock('@/components/Toast', () => ({
  useToastOptional: () => mocks.toast,
  useToast: () => mocks.toast,
}));

vi.mock('@/hooks/useUpdater', () => ({
  useUpdater: () => ({
    updateReady: false,
    updateVersion: null,
    updateChecking: false,
    updateDownloading: false,
    updateInstalling: false,
    updatePreparing: false,
    pendingUpdateOnStartup: null,
    dismissPendingUpdate: vi.fn(),
    checkForUpdate: vi.fn(async () => 'up-to-date'),
    restartAndUpdate: vi.fn(),
  }),
}));

vi.mock('@/hooks/useTrayEvents', () => ({
  useTrayEvents: vi.fn(),
}));

vi.mock('@/hooks/useHelperAgentModelDefaults', () => ({
  useHelperAgentModelDefaults: () => ({
    providerId: mocks.provider.id,
    model: 'mimo-v2.5-pro',
    setDefaults: vi.fn(),
  }),
}));

vi.mock('@/hooks/useBrowserResourceReady', () => ({
  useBrowserResourceReady: () => true,
}));

vi.mock('@/hooks/useRuntimeDetections', () => ({
  useRuntimeDetections: () => ({
    detections: {
      builtin: { installed: true, readiness: 'ready' },
      dsh: { installed: false, readiness: 'unavailable' },
      'claude-code': { installed: false, readiness: 'unavailable' },
      codex: { installed: false, readiness: 'unavailable' },
      antigravity: { installed: false, readiness: 'unavailable' },
    },
    detecting: false,
    detectionFailed: false,
    retry: vi.fn(),
  }),
}));

vi.mock('@/hooks/useRuntimeModelCatalog', () => ({
  useRuntimeModelCatalog: () => ({ models: [], loading: false, error: false, retry: vi.fn() }),
}));

vi.mock('@/hooks/useWorkspaceFileService', () => ({
  useWorkspaceFileService: () => ({
    isAvailable: false,
    listFiles: vi.fn(async () => []),
    searchFiles: vi.fn(async () => []),
    listSlashCommands: vi.fn(async () => []),
    importBase64Files: vi.fn(),
    addGitignore: vi.fn(),
    prepareUserImageAttachments: vi.fn(),
    copyPaths: vi.fn(),
  }),
}));

vi.mock('@/hooks/useTauriFileDrop', () => ({
  useTauriFileDrop: () => ({
    isDragging: false,
    activeZoneId: null,
    registerZone: vi.fn(),
    unregisterZone: vi.fn(),
  }),
}));

vi.mock('@/hooks/useConfig', () => ({
  useConfig: () => ({
    config: {
      projects: [mocks.project],
      agents: [mocks.agent],
      defaultWorkspacePath: mocks.project.path,
      defaultPermissionMode: 'auto',
      defaultProviderId: mocks.provider.id,
      enabledOfficialToolIds: [],
      plugins: [],
      enabledPlugins: {},
    },
    isLoading: false,
    error: null,
    projects: [mocks.project],
    providers: [mocks.provider],
    apiKeys: { [mocks.provider.id]: 'key' },
    providerVerifyStatus: { [mocks.provider.id]: { status: 'valid' } },
    addProject: vi.fn(async () => mocks.project),
    updateProject: vi.fn(async () => undefined),
    patchProject: vi.fn(async () => undefined),
    removeProject: vi.fn(async () => undefined),
    touchProject: vi.fn(async () => undefined),
    addCustomProvider: vi.fn(async () => undefined),
    updateCustomProvider: vi.fn(async () => undefined),
    deleteCustomProvider: vi.fn(async () => undefined),
    refreshProviders: vi.fn(async () => undefined),
    savePresetCustomModels: vi.fn(async () => undefined),
    removePresetCustomModel: vi.fn(async () => undefined),
    savePrimaryModel: vi.fn(async () => undefined),
    saveProviderModelAliases: vi.fn(async () => undefined),
    saveApiKey: vi.fn(async () => undefined),
    deleteApiKey: vi.fn(async () => undefined),
    saveProviderVerifyStatus: vi.fn(async () => undefined),
    updateConfig: vi.fn(async () => undefined),
    patchProxySettings: vi.fn(async () => undefined),
    refreshConfig: vi.fn(async () => undefined),
    reload: vi.fn(async () => undefined),
    refreshProviderData: vi.fn(async () => undefined),
  }),
}));

vi.mock('@/config/useConfigData', () => ({
  useConfigData: () => ({
    config: { agents: [mocks.agent], chatSendShortcut: 'enter' },
    projects: [mocks.project],
    managedCodexModelCatalogStatus: 'idle',
  }),
}));

vi.mock('@/config/services/appConfigService', () => ({
  atomicModifyConfig: vi.fn(async (modifier: (config: Record<string, unknown>) => Record<string, unknown>) => modifier({})),
  notifyConfigChanged: vi.fn(),
}));

vi.mock('@/config/configService', () => ({
  ensureSelfAwarenessWorkspace: vi.fn(async () => mocks.project),
  resolveBuiltinSelection: vi.fn(() => ({ provider: mocks.provider, model: 'mimo-v2.5-pro' })),
  pairBuiltinSelection: vi.fn((_provider, model) => ({ providerId: mocks.provider.id, model })),
  isProviderAvailable: vi.fn(() => true),
  getAllMcpServers: vi.fn(async () => []),
  getEnabledMcpServerIds: vi.fn(async () => []),
  isImageUnderstandingSelectionAvailable: vi.fn(() => false),
  resolveProvider: vi.fn(() => mocks.provider),
}));

vi.mock('@/config/services/agentConfigService', () => ({
  getProjectAgent: vi.fn(() => mocks.agent),
  getAgentById: vi.fn(() => mocks.agent),
  patchAgentConfig: vi.fn(async () => undefined),
  patchAgentProjectConfig: vi.fn(async () => undefined),
}));

vi.mock('@/utils/browserMock', () => ({
  isBrowserDevMode: () => false,
  isTauriEnvironment: () => false,
}));

vi.mock('@/utils/frontendLogger', () => ({
  forceFlushLogs: vi.fn(async () => undefined),
  setLogServerReady: vi.fn(),
  clearLogServerUrl: vi.fn(),
  setAppActiveTabId: vi.fn(),
  subscribeFrontendLogs: () => () => undefined,
  setCurrentTabId: vi.fn(),
}));

vi.mock('@/utils/lastExitMarker', () => ({
  consumeCleanExitMarker: vi.fn(async () => true),
}));

vi.mock('@/utils/tabPersistenceDurable', () => ({
  persistOpenTabsDurable: vi.fn(async () => undefined),
  loadAndClearOpenTabsDurable: vi.fn(async () => null),
  clearOpenTabsDurable: vi.fn(async () => undefined),
}));

vi.mock('@/utils/tauriListen', () => ({
  listenWithCleanup: vi.fn(async () => ({ isRegistered: () => true, unlisten: vi.fn() })),
}));

import '@/pages/Launcher';
import App from './App';

describe('App composer draft preservation (single-page shell)', () => {
  afterEach(() => {
    clearAllComposerDrafts();
    vi.clearAllMocks();
    localStorage.clear();
    mocks.sidebarProps.length = 0;
    mocks.latestChatTabProvider = null;
    mocks.deleteTargetSessionId = null;
    mocks.deleteResults.length = 0;
  });

  it('preserves Launcher draft across single-page shell close to TeamOffice and return via New Chat', async () => {
    renderWithTheme(<App />);

    const initialTextarea = await screen.findByRole('textbox', {}, { timeout: 5000 });
    fireEvent.change(initialTextarea, { target: { value: '打算找个人' } });
    expect((initialTextarea as HTMLTextAreaElement).value).toBe('打算找个人');

    // Navigate to TeamOffice — single-page shell closes and unmounts the old Launcher tab
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-team-office'));
    });
    await waitFor(() => {
      expect(screen.queryByRole('textbox')).toBeNull();
    });

    // Click "创建新任务" — creates a brand new Launcher tab with a new tab.id
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-new-chat'));
    });

    const restoredTextarea = await screen.findByRole('textbox', {}, { timeout: 5000 });
    expect((restoredTextarea as HTMLTextAreaElement).value).toBe('打算找个人');
  });

  it('C1: preserves Launcher draft when launching from sidebar project (with or without initialMessage)', async () => {
    renderWithTheme(<App />);

    const launcherTextarea = await screen.findByRole('textbox', {}, { timeout: 5000 });
    fireEvent.change(launcherTextarea, { target: { value: '打算找个人' } });

    // Launch project from sidebar without initialMessage
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-workspace'));
    });
    await screen.findByTestId('chat-page', {}, { timeout: 5000 });
    expect(getComposerDraft(LAUNCHER_DRAFT_KEY)?.text).toBe('打算找个人');

    // Launch project from another entry point WITH initialMessage
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-workspace-with-msg'));
    });
    await screen.findByTestId('chat-page', {}, { timeout: 5000 });
    expect(getComposerDraft(LAUNCHER_DRAFT_KEY)?.text).toBe('打算找个人');

    // Return to Launcher — draft must still be intact
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-new-chat'));
    });
    const restoredTextarea = await screen.findByRole('textbox', {}, { timeout: 5000 });
    expect((restoredTextarea as HTMLTextAreaElement).value).toBe('打算找个人');
  });

  it('C1: clears Launcher draft after Launcher send flushSync replacement and does not resurrect', async () => {
    renderWithTheme(<App />);

    const launcherTextarea = await screen.findByRole('textbox', {}, { timeout: 5000 });
    fireEvent.change(launcherTextarea, { target: { value: '从首页发送的消息' } });

    await act(async () => {
      fireEvent.keyDown(launcherTextarea, { key: 'Enter', code: 'Enter' });
    });
    await screen.findByTestId('chat-page', {}, { timeout: 5000 });
    expect(getComposerDraft(LAUNCHER_DRAFT_KEY)).toBeUndefined();

    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-new-chat'));
    });
    const nextLauncherTextarea = await screen.findByRole('textbox', {}, { timeout: 5000 });
    expect((nextLauncherTextarea as HTMLTextAreaElement).value).toBe('');
  });

  it('does NOT preserve unsent pending Chat draft across leave and reopen', async () => {
    renderWithTheme(<App />);
    await screen.findByRole('textbox', {}, { timeout: 5000 });

    // Open a pending Chat via sidebar project "在此项目创建新任务"
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-workspace'));
    });
    const chatPage = await screen.findByTestId('chat-page', {}, { timeout: 5000 });
    expect(isPendingSessionId(chatPage.getAttribute('data-session-id'))).toBe(true);

    const chatTextarea = screen.getByRole('textbox');
    fireEvent.change(chatTextarea, { target: { value: '未发送的pending草稿' } });

    // Switch away to Settings (single-page shell closes pending Chat)
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-settings'));
    });
    await waitFor(() => {
      expect(screen.queryByTestId('chat-page')).toBeNull();
    });

    // Click project "在此项目创建新任务" again — input must be empty
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-workspace'));
    });
    await screen.findByTestId('chat-page', {}, { timeout: 5000 });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('');
  });

  it('preserves and isolates drafts across real Chat sessions and pending->real birth', async () => {
    renderWithTheme(<App />);
    await screen.findByRole('textbox', {}, { timeout: 5000 });

    // Real session A
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-session-a'));
    });
    await waitFor(() => {
      expect(screen.getByTestId('chat-page').getAttribute('data-session-id')).toBe('real-session-a');
    });
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '会话A草稿' } });

    // Switch to Settings, then open real session B (must not see A's draft)
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-settings'));
    });
    await waitFor(() => {
      expect(screen.queryByTestId('chat-page')).toBeNull();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-session-b'));
    });
    await waitFor(() => {
      expect(screen.getByTestId('chat-page').getAttribute('data-session-id')).toBe('real-session-b');
    });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('');
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '会话B草稿' } });

    // Reopen session A — A's draft is restored intact
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-session-a'));
    });
    await waitFor(() => {
      expect(screen.getByTestId('chat-page').getAttribute('data-session-id')).toBe('real-session-a');
    });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('会话A草稿');

    // Pending -> real birth preserves draft after subsequent leave & return
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-workspace'));
    });
    await waitFor(() => {
      expect(isPendingSessionId(screen.getByTestId('chat-page').getAttribute('data-session-id'))).toBe(true);
    });
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '出生后的草稿' } });
    await act(async () => {
      await mocks.latestChatTabProvider?.onSessionIdChange('born-session-1');
    });
    await waitFor(() => {
      expect(screen.getByTestId('chat-page').getAttribute('data-session-id')).toBe('born-session-1');
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-settings'));
    });
    await waitFor(() => {
      expect(screen.queryByTestId('chat-page')).toBeNull();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-born-session'));
    });
    await waitFor(() => {
      expect(screen.getByTestId('chat-page').getAttribute('data-session-id')).toBe('born-session-1');
    });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('出生后的草稿');
  });

  it('deletes the session draft when handleDeleteSession succeeds', async () => {
    renderWithTheme(<App />);
    await screen.findByRole('textbox', {}, { timeout: 5000 });

    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-open-session-a'));
    });
    await waitFor(() => {
      expect(screen.getByTestId('chat-page').getAttribute('data-session-id')).toBe('real-session-a');
    });
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '待删除草稿' } });

    mocks.deleteTargetSessionId = 'real-session-a';
    await act(async () => {
      fireEvent.click(screen.getByTestId('sidebar-delete-session'));
    });
    await waitFor(() => {
      expect(mocks.deleteResults).toEqual([{ deleted: true }]);
    });

    expect(getComposerDraft(createSessionDraftKey('real-session-a'))).toBeUndefined();
  });
});
