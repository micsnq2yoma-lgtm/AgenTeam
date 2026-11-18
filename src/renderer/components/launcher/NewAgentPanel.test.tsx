import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Project, WorkspaceTemplate } from '@/config/types';
import { ProjectAlreadyExistsError } from '@/config/services/projectService';
import { i18n } from '@/i18n';

const mocks = vi.hoisted(() => ({
  projects: [] as Project[],
  userTemplates: [] as WorkspaceTemplate[],
  addProject: vi.fn(),
  openDialog: vi.fn(),
  invoke: vi.fn(),
  checkPaths: vi.fn(),
  openPathExternal: vi.fn(),
  toastError: vi.fn(),
  track: vi.fn(),
  existingPaths: new Set<string>(),
  updateUserTemplate: vi.fn(),
}));

vi.mock('@/hooks/useConfig', () => ({
  useConfig: () => ({ projects: mocks.projects, addProject: mocks.addProject }),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.openDialog }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/path', () => ({
  join: async (...parts: string[]) => parts.join('/'),
  basename: async (path: string) => path.split('/').filter(Boolean).pop() ?? '',
  homeDir: async () => '/Users/me',
}));
vi.mock('@tauri-apps/plugin-fs', () => ({ exists: async (path: string) => mocks.existingPaths.has(path) }));
vi.mock('@/config/services/templateService', () => ({
  loadUserTemplates: async () => mocks.userTemplates,
  addUserTemplate: vi.fn(),
  removeUserTemplate: vi.fn(),
  updateUserTemplate: mocks.updateUserTemplate,
}));
vi.mock('@/hooks/useWorkspaceFileService', () => ({
  useWorkspaceFileService: () => ({ checkPaths: mocks.checkPaths, openPathExternal: mocks.openPathExternal }),
}));
vi.mock('@/components/Toast', () => ({
  useToast: () => ({ error: mocks.toastError, success: vi.fn(), info: vi.fn(), warning: vi.fn() }),
}));
vi.mock('@/analytics', () => ({ track: mocks.track }));
vi.mock('@/utils/browserMock', () => ({ isBrowserDevMode: () => false, pickFolderForDialog: vi.fn() }));

import NewAgentPanel from './NewAgentPanel';

const tl = (key: string, options?: Record<string, unknown>) => String(i18n.t(`launcher:${key}`, options));

function project(overrides: Partial<Project> & Pick<Project, 'id' | 'path'>): Project {
  return { name: 'p', lastOpened: '', providerId: null, permissionMode: null, ...overrides };
}

function renderPanel() {
  const onClose = vi.fn();
  const onCreated = vi.fn();
  render(<NewAgentPanel onClose={onClose} onCreated={onCreated} />);
  return { onClose, onCreated };
}

function createButton() {
  return screen.getByRole('button', { name: tl('newAgentPanel.create') });
}

beforeEach(() => {
  mocks.projects = [];
  mocks.userTemplates = [];
  mocks.existingPaths = new Set();
  mocks.addProject.mockReset().mockImplementation(async (path: string) => project({ id: 'new', path }));
  mocks.openDialog.mockReset();
  mocks.invoke.mockReset().mockResolvedValue(undefined);
  mocks.checkPaths.mockReset().mockResolvedValue({ results: {} });
  mocks.openPathExternal.mockReset();
  mocks.toastError.mockReset();
  mocks.track.mockReset();
  mocks.updateUserTemplate.mockReset().mockResolvedValue(undefined);
});

describe('NewAgentPanel list', () => {
  it('offers one "new team" entry, then my templates and the add-template row', async () => {
    mocks.userTemplates = [{ id: 'research', name: '研究助手', description: '', isBuiltin: false, path: '/t/research', icon: 'book' }];
    renderPanel();

    const list = await screen.findByText('研究助手').then((node) => node.closest('[data-new-agent-panel-list]') as HTMLElement);
    const labels = within(list).getAllByRole('button').map((button) => button.textContent ?? '');
    expect(labels).toHaveLength(3);
    expect(labels[0]).toContain(tl('newAgentPanel.team.title'));
    expect(labels[1]).toContain('研究助手');
    expect(labels[1]).toContain(tl('newAgentPanel.noDescription'));
    expect(labels[2]).toContain(tl('newAgentPanel.addTemplate'));
    expect(within(list).queryByText(tl('newAgentPanel.mino.title'))).not.toBeInTheDocument();
    expect(within(list).getByText(tl('newAgentPanel.myTemplates'))).toBeInTheDocument();
  });

  it('closes on Escape from the list', () => {
    const { onClose } = renderPanel();
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});

async function openTeam(name?: string) {
  const handlers = renderPanel();
  fireEvent.click(screen.getByRole('button', { name: new RegExp(tl('newAgentPanel.team.title')) }));
  const nameInput = await screen.findByRole('textbox', { name: tl('newAgentPanel.nameLabel') });
  if (name !== undefined) fireEvent.change(nameInput, { target: { value: name } });
  return { ...handlers, nameInput };
}

function locationRadio(key: 'locationDefault' | 'locationNew' | 'locationExisting') {
  return screen.getByRole('radio', { name: tl(`newAgentPanel.team.${key}`) });
}

function pathText() {
  return document.querySelector('[data-new-agent-panel-path]')?.textContent ?? '';
}

describe('NewAgentPanel new team', () => {
  it('opens on the default location with assistant mode on', async () => {
    const { nameInput } = await openTeam();

    expect(nameInput).toHaveValue('');
    expect(createButton()).toBeDisabled();
    expect(locationRadio('locationDefault')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('switch', { name: tl('newAgentPanel.team.assistantTitle') })).toHaveAttribute('aria-checked', 'true');
    const capabilities = document.querySelector('[data-new-agent-panel-capabilities]') as HTMLElement;
    expect(within(capabilities).getAllByRole('listitem')).toHaveLength(3);
    expect(within(capabilities).getByText(tl('newAgentPanel.mino.capEvolveTitle'))).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: tl('newAgentPanel.changePath') })).not.toBeInTheDocument();
  });

  it('creates an assistant team at the default location with assistant defaults', async () => {
    const { onCreated, onClose } = await openTeam('alpha');
    await waitFor(() => expect(pathText()).toContain('.agenteam/projects/alpha'));

    fireEvent.click(createButton());

    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    expect(mocks.invoke).toHaveBeenCalledWith('cmd_prepare_team_folder', {
      destPath: '/Users/me/.agenteam/projects/alpha',
      adoptExisting: false,
      assistantTemplateId: 'mino',
    });
    const [path, options] = mocks.addProject.mock.calls[0];
    expect(path).toBe('/Users/me/.agenteam/projects/alpha');
    expect(options).toMatchObject({
      displayName: 'alpha',
      templateId: 'mino',
      templateSource: 'builtin',
      onExisting: 'reject',
    });
    expect(options.agentDefaults?.heartbeat?.enabled).toBe(true);
    expect(mocks.track).toHaveBeenCalledWith('workspace_create', { source: 'team', location: 'default', assistant: true });
    expect(onClose).toHaveBeenCalled();
  });

  it('previews the suffixed folder when the default name is taken', async () => {
    mocks.existingPaths = new Set(['/Users/me/.agenteam/projects/alpha']);
    await openTeam('alpha');
    await waitFor(() => expect(pathText()).toContain('alpha-2'));
  });

  it('creates a plain team in a new folder under the picked parent', async () => {
    mocks.openDialog.mockResolvedValue('/work');
    await openTeam('beta');

    fireEvent.click(locationRadio('locationNew'));
    await waitFor(() => expect(locationRadio('locationNew')).toHaveAttribute('aria-checked', 'true'));
    await waitFor(() => expect(pathText()).toContain('beta'));
    fireEvent.click(screen.getByRole('switch', { name: tl('newAgentPanel.team.assistantTitle') }));
    expect(screen.getByText(tl('newAgentPanel.team.plainIntro'))).toBeInTheDocument();
    fireEvent.click(createButton());

    await waitFor(() => expect(mocks.addProject).toHaveBeenCalled());
    expect(mocks.invoke).toHaveBeenCalledWith('cmd_prepare_team_folder', {
      destPath: '/work/beta',
      adoptExisting: false,
      assistantTemplateId: null,
    });
    expect(mocks.addProject).toHaveBeenCalledWith('/work/beta', {
      displayName: 'beta',
      icon: undefined,
      onExisting: 'reject',
    });
  });

  it('keeps the previous location when the parent picker is cancelled', async () => {
    mocks.openDialog.mockResolvedValue(null);
    await openTeam('beta');
    fireEvent.click(locationRadio('locationNew'));
    await waitFor(() => expect(mocks.openDialog).toHaveBeenCalled());
    expect(locationRadio('locationDefault')).toHaveAttribute('aria-checked', 'true');
  });

  it('adopts an existing folder without file work when assistant mode is off', async () => {
    mocks.openDialog.mockResolvedValue('/work/my-app');
    mocks.checkPaths.mockResolvedValue({ results: { 'AGENTS.md': { exists: true, type: 'file' } } });
    const { nameInput, onCreated } = await openTeam();

    fireEvent.click(locationRadio('locationExisting'));
    await waitFor(() => expect(nameInput).toHaveValue('my-app'));
    fireEvent.click(screen.getByRole('switch', { name: tl('newAgentPanel.team.assistantTitle') }));
    expect(await screen.findByText(tl('newAgentPanel.local.capInstructionsFoundTitle', { file: 'AGENTS.md' }))).toBeInTheDocument();
    fireEvent.click(createButton());

    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(mocks.addProject).toHaveBeenCalledWith('/work/my-app', {
      displayName: 'my-app',
      icon: undefined,
      onExisting: 'reject',
    });
  });

  it('adds assistant files to an existing folder without overwriting', async () => {
    mocks.openDialog.mockResolvedValue('/work/my-app');
    await openTeam('助理');

    fireEvent.click(locationRadio('locationExisting'));
    expect(await screen.findByText(tl('newAgentPanel.team.capKeepFilesTitle'))).toBeInTheDocument();
    fireEvent.click(createButton());

    await waitFor(() => expect(mocks.addProject).toHaveBeenCalled());
    expect(mocks.invoke).toHaveBeenCalledWith('cmd_prepare_team_folder', {
      destPath: '/work/my-app',
      adoptExisting: true,
      assistantTemplateId: 'mino',
    });
    expect(mocks.addProject.mock.calls[0][1]).toMatchObject({ displayName: '助理', templateId: 'mino' });
  });

  it('rejects an already registered folder and keeps the current location', async () => {
    mocks.projects = [project({ id: 'a', path: '/work/app' })];
    mocks.openDialog.mockResolvedValue('/work/app');
    await openTeam('alpha');

    fireEvent.click(locationRadio('locationExisting'));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith(tl('newAgentPanel.errors.exists')));
    expect(locationRadio('locationDefault')).toHaveAttribute('aria-checked', 'true');
  });

  it('reports archived folders distinctly', async () => {
    mocks.projects = [project({ id: 'a', path: '/work/old', archivedAt: '2026-01-01T00:00:00.000Z' })];
    mocks.openDialog.mockResolvedValue('/work/old');
    await openTeam('alpha');

    fireEvent.click(locationRadio('locationExisting'));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith(tl('newAgentPanel.errors.archived')));
  });

  it('keeps the detail page when the folder was registered meanwhile', async () => {
    const { onCreated } = await openTeam('alpha');
    mocks.addProject.mockRejectedValue(new ProjectAlreadyExistsError(project({ id: 'a', path: '/work/alpha' })));
    fireEvent.click(createButton());

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith(tl('newAgentPanel.errors.exists')));
    expect(onCreated).not.toHaveBeenCalled();
    expect(createButton()).toBeInTheDocument();
  });

  it('disables creation for an empty name and goes back on Escape', async () => {
    const { nameInput } = await openTeam('   ');
    expect(createButton()).toBeDisabled();

    fireEvent.keyDown(nameInput, { key: 'Escape' });
    expect(await screen.findByText(tl('newAgentPanel.myTemplates'))).toBeInTheDocument();
  });

  it('uses the shared icon library and passes the chosen icon', async () => {
    await openTeam('alpha');
    fireEvent.click(screen.getByRole('button', { name: tl('newAgentPanel.changeIcon') }));
    const fox = await screen.findByTitle('fox');
    await act(async () => {
      fireEvent.click(fox);
    });
    fireEvent.click(createButton());

    await waitFor(() => expect(mocks.addProject).toHaveBeenCalled());
    expect(mocks.addProject.mock.calls[0][1]).toMatchObject({ icon: 'fox' });
  });
});

describe('NewAgentPanel keyboard and busy states', () => {
  it('goes back then closes on Escape even when focus fell to <body>', async () => {
    const { onClose } = await openTeam('alpha');
    (document.activeElement as HTMLElement | null)?.blur();

    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(await screen.findByText(tl('newAgentPanel.myTemplates'))).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('closes only the icon popover on the first Escape', async () => {
    await openTeam('alpha');
    fireEvent.click(screen.getByRole('button', { name: tl('newAgentPanel.changeIcon') }));
    await screen.findByTitle('fox');

    fireEvent.keyDown(document.body, { key: 'Escape' });

    await waitFor(() => expect(screen.queryByTitle('fox')).not.toBeInTheDocument());
    expect(createButton()).toBeInTheDocument();
  });

  it('cannot be closed or resubmitted while creating', async () => {
    const { onClose } = await openTeam('alpha');
    mocks.addProject.mockReturnValue(new Promise(() => undefined));
    fireEvent.click(createButton());
    await waitFor(() => expect(mocks.addProject).toHaveBeenCalledTimes(1));

    expect(createButton()).toBeDisabled();
    fireEvent.keyDown(document.body, { key: 'Escape' });
    fireEvent.click(screen.getByRole('button', { name: tl('newAgentPanel.close') }));
    fireEvent.click(createButton());
    expect(onClose).not.toHaveBeenCalled();
    expect(mocks.addProject).toHaveBeenCalledTimes(1);
  });

  it('stays on the detail page with an error when preparing the folder fails', async () => {
    const { onCreated } = await openTeam('alpha');
    mocks.invoke.mockRejectedValue(new Error('disk full'));
    fireEvent.click(createButton());

    expect(await screen.findByRole('alert')).toHaveTextContent('disk full');
    expect(mocks.addProject).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it('follows the name in the previewed folder path', async () => {
    await openTeam('My: Agent');
    await waitFor(() => expect(pathText()).toContain('My- Agent'));
  });
});

describe('NewAgentPanel user template', () => {
  it('shows detected template contents and copies the template on create', async () => {
    mocks.userTemplates = [{ id: 'research', name: '研究助手', description: '读论文', isBuiltin: false, path: '/t/research', icon: 'book' }];
    mocks.checkPaths.mockResolvedValue({ results: {
      'CLAUDE.md': { exists: true, type: 'file' },
      '.claude/skills': { exists: true, type: 'dir' },
    } });
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /研究助手/ }));
    expect(await screen.findByText('.claude/skills/')).toBeInTheDocument();
    expect(screen.getByText('CLAUDE.md')).toBeInTheDocument();
    expect(screen.getByText('读论文')).toBeInTheDocument();

    fireEvent.click(createButton());
    await waitFor(() => expect(mocks.addProject).toHaveBeenCalled());
    expect(mocks.invoke).toHaveBeenCalledWith('cmd_create_workspace_from_template', {
      sourcePath: '/t/research',
      destPath: '/Users/me/.agenteam/projects/研究助手',
    });
    expect(mocks.addProject.mock.calls[0][1]).toMatchObject({
      displayName: '研究助手',
      templateId: 'research',
      templateSource: 'user',
      agentDefaults: undefined,
    });
  });
});

describe('NewAgentPanel template editing page', () => {
  async function openTemplate() {
    mocks.userTemplates = [{ id: 'tftpboot', name: 'tftpboot', description: '', isBuiltin: false, path: '/t/tftpboot', icon: '' }];
    const handlers = renderPanel();
    fireEvent.click(await screen.findByRole('button', { name: /tftpboot/ }));
    await screen.findByRole('textbox', { name: tl('newAgentPanel.nameLabel') });
    return handlers;
  }

  it('opens as its own page from "add description" and saves back to the detail page', async () => {
    await openTemplate();
    fireEvent.click(screen.getByRole('button', { name: tl('newAgentPanel.template.addDescription') }));

    expect(await screen.findByText(tl('newAgentPanel.template.editTitle'))).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: tl('newAgentPanel.create') })).not.toBeInTheDocument();
    const description = screen.getByPlaceholderText(tl('newAgentPanel.template.descriptionPlaceholder'));
    expect(screen.getByPlaceholderText(tl('newAgentPanel.template.namePlaceholder'))).toHaveValue('tftpboot');

    fireEvent.change(description, { target: { value: '  TFTP 启动服务  ' } });
    fireEvent.click(screen.getByRole('button', { name: tl('newAgentPanel.template.save') }));

    await waitFor(() => expect(mocks.updateUserTemplate).toHaveBeenCalledWith('tftpboot', {
      name: 'tftpboot',
      description: 'TFTP 启动服务',
      icon: '',
    }));
    expect(await screen.findByText('TFTP 启动服务')).toBeInTheDocument();
    expect(createButton()).toBeInTheDocument();
  });

  it('opens from the more menu, blocks an empty name, and returns on Escape', async () => {
    await openTemplate();
    fireEvent.click(screen.getByRole('button', { name: tl('newAgentPanel.template.more') }));
    fireEvent.click(await screen.findByRole('button', { name: tl('newAgentPanel.template.edit') }));

    const name = await screen.findByPlaceholderText(tl('newAgentPanel.template.namePlaceholder'));
    fireEvent.change(name, { target: { value: '  ' } });
    expect(screen.getByRole('button', { name: tl('newAgentPanel.template.save') })).toBeDisabled();

    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(await screen.findByRole('button', { name: tl('newAgentPanel.create') })).toBeInTheDocument();
    expect(mocks.updateUserTemplate).not.toHaveBeenCalled();
  });
});
