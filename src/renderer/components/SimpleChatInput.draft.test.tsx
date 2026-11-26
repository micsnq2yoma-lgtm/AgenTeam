import { act, fireEvent, screen } from '@testing-library/react';
import { createRef } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Project, Provider } from '@/config/types';
import { renderWithTheme } from '@/test/renderWithTheme';
import { ThemeRuntimeProvider } from '@/theme';
import { TASK_ALIGNMENT_SKILL_REQUIREMENT } from '../../shared/systemSkills';
import {
  clearAllComposerDrafts,
  createSessionDraftKey,
  getComposerDraft,
  LAUNCHER_DRAFT_KEY,
  setComposerDraft,
} from './chat-input/composerDraftStore';
import SimpleChatInput from './SimpleChatInput';
import type { SimpleChatInputHandle, SimpleChatInputProps } from './chat-input/types';
import BrandSection from './launcher/BrandSection';
import type { AgentMentionSnapshot } from '../../shared/agentMentions';

const mocks = vi.hoisted(() => ({
  toast: {
    info: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
    success: vi.fn(),
  },
}));

vi.mock('@/components/Toast', () => ({
  useToast: () => mocks.toast,
  useToastOptional: () => mocks.toast,
}));

vi.mock('@/context/ImagePreviewContext', () => ({
  useImagePreview: () => ({ openPreview: vi.fn() }),
}));

vi.mock('@/config/useConfigData', () => ({
  useConfigData: () => ({
    config: { agents: [], chatSendShortcut: 'enter' },
    projects: [],
    managedCodexModelCatalogStatus: 'idle',
  }),
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

vi.mock('@/analytics', () => ({
  track: vi.fn(),
}));

const provider: Provider = {
  id: 'provider-1',
  name: 'Provider',
  vendor: 'Anthropic',
  cloudProvider: 'anthropic',
  type: 'api',
  primaryModel: 'test-model',
  isBuiltin: true,
  config: { baseUrl: 'https://example.com' },
  models: [{ model: 'test-model', modelName: 'Test Model', modelSeries: 'claude' }],
};

const minoProject: Project = {
  id: 'proj-mino',
  name: 'Mino',
  displayName: 'Mino',
  path: '/mock/Mino',
  internal: false,
  providerId: 'provider-1',
  permissionMode: 'auto',
};

const betaProject: Project = {
  id: 'proj-beta',
  name: 'Beta',
  displayName: 'Beta',
  path: '/mock/Beta',
  internal: false,
  providerId: 'provider-1',
  permissionMode: 'auto',
};

const sampleMention: AgentMentionSnapshot = {
  agent: {
    selector: 'mino-helper',
    name: 'Mino Helper',
    isLocal: true,
    deviceId: null,
    deviceName: null,
    platform: null,
    description: null,
    source: null,
  },
  authGeneration: 1,
  principalId: null,
  networkId: null,
};

function renderComposer(props: Partial<SimpleChatInputProps> = {}) {
  const ref = createRef<SimpleChatInputHandle>();
  const rendered = renderWithTheme(
    <SimpleChatInput
      ref={ref}
      isLoading={false}
      onSend={vi.fn()}
      provider={provider}
      providers={[provider]}
      providerAvailable
      selectedModel="test-model"
      {...props}
    />,
  );
  return { ...rendered, ref };
}

describe('SimpleChatInput draft persistence', () => {
  afterEach(() => {
    clearAllComposerDrafts();
    vi.clearAllMocks();
  });

  it('restores text and @-mention context across unmount and remount when draftKey is provided', () => {
    const key = createSessionDraftKey('sess-1');
    const first = renderComposer({ draftKey: key, workspacePath: '/mock/Mino' });

    act(() => {
      first.ref.current?.setValue('@Agent-id:mino-helper 帮我看下这个', {
        agentMentions: [sampleMention],
        requiredSystemSkill: TASK_ALIGNMENT_SKILL_REQUIREMENT,
      });
    });

    first.unmount();

    const second = renderComposer({ draftKey: key, workspacePath: '/mock/Mino' });
    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    expect(textarea.value).toBe('@Agent-id:mino-helper 帮我看下这个');
    expect(second.ref.current?.getQueryContext().agentMentions).toEqual([sampleMention]);
    expect(second.ref.current?.getQueryContext().requiredSystemSkill).toEqual(
      TASK_ALIGNMENT_SKILL_REQUIREMENT,
    );
  });

  it('keeps behavior unchanged when draftKey is omitted', () => {
    const first = renderComposer();
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '临时内容' } });
    first.unmount();

    renderComposer();
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('');
  });

  it('isolates drafts across different draftKeys (Launcher vs Chat and Session A vs Session B)', () => {
    const launcher = renderComposer({ draftKey: LAUNCHER_DRAFT_KEY });
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '首页草稿' } });
    launcher.unmount();

    const sessionA = renderComposer({ draftKey: createSessionDraftKey('sess-a') });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('');
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '会话A草稿' } });
    sessionA.unmount();

    const sessionB = renderComposer({ draftKey: createSessionDraftKey('sess-b') });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('');
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '会话B草稿' } });
    sessionB.unmount();

    const launcher2 = renderComposer({ draftKey: LAUNCHER_DRAFT_KEY });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('首页草稿');
    launcher2.unmount();

    const sessionA2 = renderComposer({ draftKey: createSessionDraftKey('sess-a') });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('会话A草稿');
    sessionA2.unmount();
  });

  it('clears stored draft on send success and does not resurrect on remount', async () => {
    const key = createSessionDraftKey('sess-send');
    const onSend = vi.fn<SimpleChatInputProps['onSend']>(async (_text, _images, _mode, _ctx, onSubmitted) => {
      onSubmitted?.();
      return true;
    });

    const first = renderComposer({ draftKey: key, onSend });
    const textarea = screen.getByRole('textbox');
    fireEvent.change(textarea, { target: { value: '要发送的内容' } });
    await act(async () => {
      fireEvent.keyDown(textarea, { key: 'Enter', code: 'Enter' });
    });

    expect(onSend).toHaveBeenCalledTimes(1);
    expect(getComposerDraft(key)).toBeUndefined();
    first.unmount();
    expect(getComposerDraft(key)).toBeUndefined();

    renderComposer({ draftKey: key, onSend });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('');
  });

  it('preserves draft when send fails (returns false)', async () => {
    const key = createSessionDraftKey('sess-fail');
    const onSend = vi.fn<SimpleChatInputProps['onSend']>(async () => false);

    const first = renderComposer({ draftKey: key, onSend });
    const textarea = screen.getByRole('textbox');
    fireEvent.change(textarea, { target: { value: '发送失败的草稿' } });
    await act(async () => {
      fireEvent.keyDown(textarea, { key: 'Enter', code: 'Enter' });
    });

    expect((textarea as HTMLTextAreaElement).value).toBe('发送失败的草稿');
    first.unmount();

    renderComposer({ draftKey: key, onSend });
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('发送失败的草稿');
  });

  it('C4: locks in-place realA -> realB draftKey switch behavior on unmount', () => {
    const keyA = createSessionDraftKey('real-a');
    const keyB = createSessionDraftKey('real-b');
    const view = renderComposer({ draftKey: keyA });

    fireEvent.change(screen.getByRole('textbox'), { target: { value: '原地切换草稿' } });
    view.rerender(
      <ThemeRuntimeProvider selection={{ themeId: 'myagents-default', appearanceMode: 'light' }}>
        <SimpleChatInput
          isLoading={false}
          onSend={vi.fn()}
          provider={provider}
          providers={[provider]}
          providerAvailable
          selectedModel="test-model"
          draftKey={keyB}
        />
      </ThemeRuntimeProvider>,
    );
    view.unmount();

    expect(getComposerDraft(keyA)).toBeUndefined();
    expect(getComposerDraft(keyB)?.text).toBe('原地切换草稿');
  });
});

describe('BrandSection draft restoration & send guards', () => {
  afterEach(() => {
    clearAllComposerDrafts();
    vi.clearAllMocks();
  });

  it('C2: preserves draft when recordingBusy blocks send', async () => {
    const onSend = vi.fn();
    const view = renderWithTheme(
      <BrandSection
        projects={[minoProject, betaProject]}
        selectedProject={minoProject}
        onSelectWorkspace={vi.fn()}
        onAddFolder={vi.fn()}
        onSend={onSend}
        recordingBusy
        isActive
        provider={provider}
        providers={[provider]}
        selectedModel="test-model"
      />,
    );

    const textarea = screen.getByRole('textbox');
    fireEvent.change(textarea, { target: { value: '录音中的草稿' } });
    await act(async () => {
      fireEvent.keyDown(textarea, { key: 'Enter', code: 'Enter' });
    });

    expect(onSend).not.toHaveBeenCalled();
    expect((textarea as HTMLTextAreaElement).value).toBe('录音中的草稿');
    view.unmount();
    expect(getComposerDraft(LAUNCHER_DRAFT_KEY)?.text).toBe('录音中的草稿');
  });

  it('C3: strips workspace-bound references and shows toast when selectedProject loads asynchronously to a different workspace', () => {
    setComposerDraft(LAUNCHER_DRAFT_KEY, {
      text: '打算找个人 @myagents_files/spec.pdf 看看',
      workspacePath: minoProject.path,
    });

    // Mount while config is still loading (selectedProject is null).
    const view = renderWithTheme(
      <BrandSection
        projects={[minoProject, betaProject]}
        selectedProject={null}
        onSelectWorkspace={vi.fn()}
        onAddFolder={vi.fn()}
        onSend={vi.fn()}
        isActive
        provider={provider}
        providers={[provider]}
        selectedModel="test-model"
      />,
    );

    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe(
      '打算找个人 @myagents_files/spec.pdf 看看',
    );
    expect(mocks.toast.info).not.toHaveBeenCalled();

    // Config finishes loading and resolves to Beta (different from saved Mino).
    view.rerender(
      <ThemeRuntimeProvider selection={{ themeId: 'myagents-default', appearanceMode: 'light' }}>
        <BrandSection
          projects={[minoProject, betaProject]}
          selectedProject={betaProject}
          onSelectWorkspace={vi.fn()}
          onAddFolder={vi.fn()}
          onSend={vi.fn()}
          isActive
          provider={provider}
          providers={[provider]}
          selectedModel="test-model"
        />
      </ThemeRuntimeProvider>,
    );

    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('打算找个人 看看');
    expect(mocks.toast.info).toHaveBeenCalledTimes(1);
  });

  it('keeps workspace-bound references intact when restoring into the same workspace', () => {
    setComposerDraft(LAUNCHER_DRAFT_KEY, {
      text: '打算找个人 @myagents_files/spec.pdf',
      workspacePath: minoProject.path,
    });

    renderWithTheme(
      <BrandSection
        projects={[minoProject, betaProject]}
        selectedProject={minoProject}
        onSelectWorkspace={vi.fn()}
        onAddFolder={vi.fn()}
        onSend={vi.fn()}
        isActive
        provider={provider}
        providers={[provider]}
        selectedModel="test-model"
      />,
    );

    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe(
      '打算找个人 @myagents_files/spec.pdf',
    );
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });
});
