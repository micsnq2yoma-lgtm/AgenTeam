import { fireEvent, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { renderWithTheme } from '@/test/renderWithTheme';
import { ToastProvider } from '@/components/Toast';
import { DEFAULT_CONFIG, MANAGED_CODEX_PROVIDER, PRESET_PROVIDERS, type AppConfig, type Project, type Provider } from '@/config/types';
import { ImagePreviewProvider } from '@/context/ImagePreviewContext';
import Launcher from './Launcher';

// Page-level check of the launcher's runtime display: the saved preference versus the runtime that
// really executes the chosen model (specs/handoff/launcher-runtime-selection-design.md).

const mocks = vi.hoisted(() => ({
  config: {} as AppConfig,
  projects: [] as Project[],
  providers: [] as Provider[],
  invoke: vi.fn(),
  patchProject: vi.fn(),
  patchAgentConfig: vi.fn(),
  patchAgentProjectConfig: vi.fn(),
  apiGetJson: vi.fn(),
  persist: vi.fn(),
}));

const detections = {
  builtin: { installed: true },
  dsh: { installed: true, readiness: 'ready' },
  antigravity: { installed: false },
  'claude-code': { installed: true },
  codex: { installed: true },
};

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));
vi.mock('@/utils/tauriListen', () => ({ listenWithCleanup: vi.fn(async () => {}) }));
vi.mock('@/hooks/useBrowserResourceReady', () => ({ useBrowserResourceReady: () => true }));
vi.mock('@/api/persistInputOption', () => ({ persistInputOptionChange: mocks.persist }));
vi.mock('@/api/apiFetch', () => ({
  apiFetch: vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) }),
  apiGetJson: mocks.apiGetJson,
  apiPostJson: vi.fn().mockResolvedValue({ success: true }),
}));
vi.mock('@/config/useConfigData', () => ({
  useConfigData: () => ({ config: mocks.config, projects: mocks.projects, providers: mocks.providers, apiKeys: { deepseek: 'key' }, isLoading: false }),
}));
vi.mock('@/hooks/useConfig', () => ({
  useConfig: () => ({
    config: mocks.config,
    projects: mocks.projects,
    providers: mocks.providers,
    isLoading: false,
    error: null,
    patchProject: mocks.patchProject,
    patchAgentConfig: mocks.patchAgentConfig,
    patchAgentProjectConfig: mocks.patchAgentProjectConfig,
    touchProject: vi.fn(),
    apiKeys: { deepseek: 'key' },
    providerVerifyStatus: {},
    refreshProviders: vi.fn(),
    updateConfig: vi.fn(),
  }),
}));

const workspace = '/work/alpha';
function setup(agentExtra: Record<string, unknown> = {}, configExtra: Partial<AppConfig> = {}) {
  const deepseek = { ...PRESET_PROVIDERS.find(p => p.id === 'deepseek')!, enabled: true } as Provider;
  const codex = {
    ...MANAGED_CODEX_PROVIDER,
    runtimeReady: true,
    primaryModel: 'gpt-codex-1',
    models: [{ model: 'gpt-codex-1', modelName: 'Codex One', modelSeries: 'codex' }],
  } as Provider;
  mocks.providers = [deepseek, codex];
  mocks.projects = [{
    id: 'alpha', name: 'alpha', displayName: 'Alpha', path: workspace, agentId: 'agent-1',
    lastOpened: '2026-11-23T00:00:00Z', providerId: null, permissionMode: null,
  } as unknown as Project];
  mocks.config = {
    ...DEFAULT_CONFIG,
    defaultWorkspacePath: workspace,
    agents: [{
      id: 'agent-1', name: 'Alpha', workspacePath: workspace,
      runtime: 'dsh', runtimePreference: { family: 'integrated', id: 'dsh' },
      providerId: 'deepseek', model: 'deepseek-chat', ...agentExtra,
    }],
    ...configExtra,
  } as unknown as AppConfig;
}

function renderLauncher() {
  return renderWithTheme(
    <ToastProvider><ImagePreviewProvider>
      <Launcher onLaunchProject={vi.fn()} isStarting={false} isActive onOpenNewAgentPanel={vi.fn()} />
    </ImagePreviewProvider></ToastProvider>,
  );
}

describe('Launcher runtime display', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    mocks.invoke.mockImplementation(async (command: string) => (command === 'cmd_detect_runtimes' ? detections : undefined));
    mocks.apiGetJson.mockResolvedValue({ success: true, models: [], data: {} });
    mocks.persist.mockResolvedValue(undefined);
    await i18n.changeLanguage('en-US');
    setup();
  });

  async function pickCodexModel() {
    fireEvent.click(await screen.findByTitle('Switch model'));
    fireEvent.click(await screen.findByText('Codex (订阅)'));
    fireEvent.click(await screen.findByText('Codex One'));
  }
  async function pickDeepseekModel() {
    fireEvent.click(await screen.findByTitle('Switch model'));
    const group = screen.queryByText('DeepSeek V4 Pro') ?? (fireEvent.click(await screen.findByText('DeepSeek')), await screen.findByText('DeepSeek V4 Pro'));
    fireEvent.click(group);
  }

  it('L1/L2: a Codex model shows the real executor with a toast, keeps the saved preference, and returns on a DSH model', async () => {
    renderLauncher();
    await screen.findByTitle(/Runtime: .*DeepSeek Harness/);
    await pickCodexModel();
    expect(await screen.findByTitle(/Runtime: .*Codex/)).toBeInTheDocument();
    expect(await screen.findByText(/only runs on .*Codex/)).toBeInTheDocument();
    expect(screen.getByTestId('launcher-runtime-execution-note')).toBeInTheDocument();
    expect(mocks.patchAgentConfig).not.toHaveBeenCalled();
    expect(mocks.patchAgentProjectConfig).not.toHaveBeenCalled();

    await pickDeepseekModel();
    expect(await screen.findByTitle(/Runtime: .*DeepSeek Harness/)).toBeInTheDocument();
    expect(await screen.findByText(/Back to .*your saved Runtime/)).toBeInTheDocument();
    expect(screen.queryByTestId('launcher-runtime-execution-note')).not.toBeInTheDocument();
    expect(mocks.patchAgentConfig).not.toHaveBeenCalled();
  });

  it('L4: clicking the saved runtime while a model pins another executor explains it and rewrites nothing', async () => {
    renderLauncher();
    await screen.findByTitle(/Runtime: .*DeepSeek Harness/);
    await pickCodexModel();
    await screen.findByText(/only runs on .*Codex/);
    fireEvent.click(screen.getByTitle(/Runtime: .*Codex/));
    const items = await screen.findAllByText(/DeepSeek Harness/);
    fireEvent.click(items[items.length - 1]);
    expect(await screen.findByText(/already DeepSeek Harness|already .*DeepSeek/)).toBeInTheDocument();
    expect(mocks.patchAgentConfig).not.toHaveBeenCalled();
    expect(mocks.patchAgentProjectConfig).not.toHaveBeenCalled();
  });

  it('L8: an external CLI preference is displayed as saved', async () => {
    setup({ runtime: 'claude-code', runtimePreference: { family: 'external', id: 'claude-code' }, providerId: undefined, model: undefined });
    renderLauncher();
    expect(await screen.findByTitle(/Runtime: .*Claude/)).toBeInTheDocument();
    expect(screen.queryByTestId('launcher-runtime-execution-note')).not.toBeInTheDocument();
  });

  it('L6: an inherited default Codex model shows the executor note without any toast', async () => {
    setup({ providerId: 'codex-sub', model: 'gpt-codex-1' });
    renderLauncher();
    expect(await screen.findByTitle(/Runtime: .*Codex/)).toBeInTheDocument();
    expect(screen.getByTestId('launcher-runtime-execution-note')).toBeInTheDocument();
    expect(screen.queryByText(/only runs on/)).not.toBeInTheDocument();
  });

  it('renders the saved DSH preference', async () => {
    renderLauncher();
    expect(await screen.findByTitle(/Runtime: .*DeepSeek Harness/)).toBeInTheDocument();
    expect(screen.queryByTestId('launcher-runtime-execution-note')).not.toBeInTheDocument();
  });
});
