import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastProvider } from '@/components/Toast';
import { i18n } from '@/i18n';

import KnowledgeDetail from './KnowledgeDetail';
import type { KnowledgeBaseRecord, KnowledgeSourceView } from './types';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  openDialog: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.openDialog }));
vi.mock('@/hooks/useConfig', () => ({
  useConfig: () => ({
    config: {
      agents: [{ id: 'agent_1', name: 'Researcher' }],
    },
  }),
}));
vi.mock('@/components/markdown-editor/MarkdownEditor', () => ({
  default: ({ initialSource, onChange }: { initialSource: string; onChange: () => void }) => (
    <textarea
      data-testid="mock-markdown-editor"
      defaultValue={initialSource}
      onChange={() => onChange()}
    />
  ),
}));

const tl = (key: string, vars?: Record<string, unknown>) =>
  String(i18n.t(`launcher:${key}`, vars));

function makeKb(over: Partial<KnowledgeBaseRecord> = {}): KnowledgeBaseRecord {
  return {
    id: 'kb_111111111111',
    name: '工程文档',
    description: '架构与发布资料',
    curatorAgentId: 'agent_1',
    curationMode: 'manual',
    curationIntervalMinutes: 4320,
    agentWriteAccess: 'curator-only',
    wikiEnabled: true,
    pendingCategories: ['研发/后端'],
    pendingCategoryReasons: { '研发/后端': '后端设计文档增多' },
    ...over,
  };
}

function makeSource(
  id: string,
  over: Partial<KnowledgeSourceView> = {},
): KnowledgeSourceView {
  return {
    id,
    displayName: `${id}.md`,
    kind: 'file',
    status: 'ready',
    metadata: {},
    curationPending: false,
    searchStatus: 'indexed',
    ...over,
  };
}

describe('KnowledgeDetail', () => {
  let state: {
    kb: KnowledgeBaseRecord;
    sources: KnowledgeSourceView[];
    categories: string[];
  };

  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage('zh-CN');
    state = {
      kb: makeKb(),
      categories: ['参考资料', '项目'],
      sources: [
        makeSource('src_ready', {
          displayName: 'architecture.pdf',
          metadata: {
            category: { value: '参考资料', origin: 'user', updatedAt: '2026-11-24T00:00:00Z' },
            summary: { value: '系统架构说明', origin: 'agent', updatedAt: '2026-11-24T00:00:00Z' },
            tags: { value: ['架构', '设计'], origin: 'agent', updatedAt: '2026-11-24T00:00:00Z' },
          },
        }),
        makeSource('src_busy', {
          displayName: 'handbook.docx',
          status: 'converting',
          jobId: 'job_20261124_000000000001',
          searchStatus: 'pending',
        }),
        makeSource('src_fail', {
          displayName: 'locked.pdf',
          status: 'failed',
          ingestError: 'KB_SOURCE_IN_USE',
          ingestSuggestion: 'closeFileAndRetry',
          searchStatus: 'pending',
        }),
      ],
    };

    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case 'cmd_kb_list':
          return [state.kb];
        case 'cmd_kb_list_source_views':
          return state.sources;
        case 'cmd_kb_list_categories':
          return state.categories;
        case 'cmd_kb_retry_source':
          state.sources = state.sources.map((s) =>
            s.id === args?.sourceId
              ? { ...s, status: 'queued', ingestError: undefined, ingestSuggestion: undefined }
              : s,
          );
          return { id: 'job_20261124_000000000002', kbId: state.kb.id, sourceId: args?.sourceId, status: 'queued' };
        case 'cmd_kb_cancel_job':
          state.sources = state.sources.map((s) =>
            s.jobId === args?.jobId ? { ...s, status: 'cancelled' } : s,
          );
          return { id: args?.jobId, kbId: state.kb.id, sourceId: 'src_busy', status: 'cancelled' };
        case 'cmd_kb_remove_source':
          state.sources = state.sources.filter((s) => s.id !== args?.sourceId);
          return undefined;
        case 'cmd_kb_set_metadata': {
          const fields = args?.fields as Record<string, unknown>;
          state.sources = state.sources.map((s) => {
            if (s.id !== args?.sourceId) return s;
            const nextMeta = { ...s.metadata };
            for (const [k, v] of Object.entries(fields)) {
              nextMeta[k as keyof typeof nextMeta] = {
                value: v,
                origin: 'user',
                updatedAt: '2026-11-24T01:00:00Z',
              };
            }
            return { ...s, metadata: nextMeta };
          });
          return { category: { status: 'updated' } };
        }
        case 'cmd_kb_unlock_metadata': {
          const key = args?.key as keyof KnowledgeSourceView['metadata'];
          state.sources = state.sources.map((s) => {
            if (s.id !== args?.sourceId) return s;
            const existing = s.metadata[key];
            if (!existing) return s;
            return {
              ...s,
              metadata: { ...s.metadata, [key]: { ...existing, origin: 'agent' } },
            };
          });
          return state.sources.find((s) => s.id === args?.sourceId);
        }
        case 'cmd_kb_list_chunks':
          return [
            {
              chunkId: 'chunk_aaaaaaaaaaaa',
              ordinal: 0,
              headingPath: ['Overview'],
              page: 3,
              prefix: 'architecture.pdf › Overview',
              text: 'Core owner boundary details.',
              byteRange: [0, 28],
              utf16Range: [0, 28],
            },
            {
              chunkId: 'chunk_bbbbbbbbbbbb',
              ordinal: 1,
              headingPath: ['Notes'],
              page: null,
              prefix: 'architecture.pdf › Notes',
              text: 'Markdown note without page number.',
              byteRange: [28, 62],
              utf16Range: [28, 62],
            },
          ];
        case 'cmd_kb_search':
          return {
            mode: 'bm25',
            hits: [
              {
                reference: `kb://${state.kb.id}/src_ready#chunk_aaaaaaaaaaaa`,
                kbId: state.kb.id,
                sourceId: 'src_ready',
                chunkId: 'chunk_aaaaaaaaaaaa',
                title: 'architecture.pdf',
                prefix: 'architecture.pdf › Overview',
                page: 3,
                snippet: 'Core owner boundary details.',
                score: 0.0325,
              },
              {
                reference: `kb://${state.kb.id}/src_ready#chunk_bbbbbbbbbbbb`,
                kbId: state.kb.id,
                sourceId: 'src_ready',
                chunkId: 'chunk_bbbbbbbbbbbb',
                title: 'notes.md',
                prefix: 'notes.md › Notes',
                page: null,
                snippet: 'Markdown note without page number.',
                score: 0.021,
              },
            ],
          };
        case 'cmd_kb_list_wiki_pages':
          return [{ path: 'index.md', sizeBytes: 120 }];
        case 'cmd_kb_read_wiki_page':
          return '# Wiki Index\n\nOverview content.';
        case 'cmd_kb_list_lint_reports':
          return [
            {
              id: '20261124T010000Z',
              report: {
                version: 1,
                generatedAt: '2026-11-24T01:00:00Z',
                items: [
                  {
                    kind: 'orphan',
                    page: 'index.md',
                    message: 'No inbound links',
                    refs: [],
                  },
                ],
              },
            },
          ];
        case 'cmd_kb_confirm_category':
          state.kb = {
            ...state.kb,
            pendingCategories: state.kb.pendingCategories.filter((c) => c !== args?.category),
          };
          state.categories = [...state.categories, args?.category as string];
          return undefined;
        case 'cmd_kb_dismiss_category':
          state.kb = {
            ...state.kb,
            pendingCategories: state.kb.pendingCategories.filter((c) => c !== args?.category),
          };
          return undefined;
        case 'cmd_kb_read_rule_doc':
          return '# 整理规则\n\n## 分类体系\n\n- `参考资料`：手册\n';
        case 'cmd_kb_write_rule_doc':
          return undefined;
        case 'cmd_kb_update':
          state.kb = { ...state.kb, ...(args?.settings as Partial<KnowledgeBaseRecord>) };
          return state.kb;
        case 'cmd_kb_curate_now':
          return 'task_1';
        case 'cmd_kb_delete':
          return undefined;
        case 'cmd_kb_job_status':
          return { id: args?.jobId, kbId: state.kb.id, sourceId: 'src_busy', status: 'converting' };
        default:
          throw new Error(`unexpected ${command}`);
      }
    });
  });

  const renderDetail = (onBack = vi.fn(), onDeleted = vi.fn()) =>
    render(
      <ToastProvider>
        <KnowledgeDetail
          kbId={state.kb.id}
          initialBase={state.kb}
          isActive
          onBack={onBack}
          onDeleted={onDeleted}
        />
      </ToastProvider>,
    );

  it('shows ingest progress, retries failed sources, and does not cancel jobs when navigating back', async () => {
    const onBack = vi.fn();
    renderDetail(onBack);

    expect(await screen.findByTestId('kb-ingest-progress')).toHaveTextContent(
      tl('knowledge.sources.inProgressBanner', { count: 1 }),
    );
    expect(screen.getByText('locked.pdf')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.sources.retryJob') }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_retry_source', {
        kbId: state.kb.id,
        sourceId: 'src_fail',
      }),
    );

    // Leaving the detail page must not cancel background ingestion.
    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.detail.back') }));
    expect(onBack).toHaveBeenCalled();
    expect(mocks.invoke).not.toHaveBeenCalledWith('cmd_kb_cancel_job', expect.anything());
  });

  it('shows field lock indicator in drawer, unlocks it, and previews chunks', async () => {
    renderDetail();
    fireEvent.click(await screen.findByRole('button', { name: 'architecture.pdf' }));

    const drawer = await screen.findByTestId('source-detail-drawer');
    expect(within(drawer).getByTestId('field-lock-category')).toBeInTheDocument();

    fireEvent.click(
      within(drawer).getByRole('button', {
        name: `${tl('knowledge.drawer.unlock')} ${tl('knowledge.drawer.fields.category')}`,
      }),
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_unlock_metadata', {
        kbId: state.kb.id,
        sourceId: 'src_ready',
        key: 'category',
      }),
    );
    await waitFor(() =>
      expect(within(drawer).queryByTestId('field-lock-category')).not.toBeInTheDocument(),
    );

    fireEvent.click(within(drawer).getByRole('tab', { name: tl('knowledge.drawer.chunksTab') }));
    expect(await within(drawer).findByText('Core owner boundary details.')).toBeInTheDocument();
    expect(within(drawer).getByText('architecture.pdf › Overview')).toBeInTheDocument();
    expect(within(drawer).getByText(tl('knowledge.drawer.chunkPage', { page: 3 }))).toBeInTheDocument();
    expect(within(drawer).queryByText('第页')).not.toBeInTheDocument();
  });

  it('runs retrieval test and jumps into the matching chunk drawer', async () => {
    renderDetail();
    await screen.findByText('architecture.pdf');

    fireEvent.click(screen.getByRole('tab', { name: tl('knowledge.detail.tabs.search') }));
    fireEvent.change(screen.getByLabelText(tl('knowledge.search.queryLabel')), {
      target: { value: 'owner boundary' },
    });
    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.search.run') }));

    expect(await screen.findByTestId('kb-search-mode')).toHaveTextContent('BM25');
    expect(screen.getByText('Core owner boundary details.')).toBeInTheDocument();
    expect(screen.queryByText('第页')).not.toBeInTheDocument();

    const openButtons = screen.getAllByRole('button', { name: tl('knowledge.search.openSource') });
    fireEvent.click(openButtons[0]);
    const drawer = await screen.findByTestId('source-detail-drawer');
    expect(await within(drawer).findByText('#chunk_aaaaaaaaaaaa')).toBeInTheDocument();
  });

  it('confirms pending categories and browses wiki pages and lint reports', async () => {
    renderDetail();
    await screen.findByText('architecture.pdf');

    fireEvent.click(screen.getByRole('tab', { name: /Wiki/ }));
    expect(await screen.findByTestId('kb-pending-categories')).toHaveTextContent('研发/后端');
    expect(await screen.findByText('Overview content.')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.wiki.confirmCategory') }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_confirm_category', {
        kbId: state.kb.id,
        category: '研发/后端',
      }),
    );

    fireEvent.click(screen.getByRole('tab', { name: new RegExp(tl('knowledge.wiki.lintTab')) }));
    expect(await screen.findByText('No inbound links')).toBeInTheDocument();
  });

  it('triggers curate now and deletes library behind ConfirmDialog in settings tab', async () => {
    const onDeleted = vi.fn();
    renderDetail(vi.fn(), onDeleted);
    await screen.findByText('architecture.pdf');

    fireEvent.click(screen.getByRole('tab', { name: tl('knowledge.detail.tabs.settings') }));
    fireEvent.click(await screen.findByRole('button', { name: tl('knowledge.settings.curateNow') }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_curate_now', { kbId: state.kb.id }),
    );

    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.delete') }));
    expect(await screen.findByText(tl('knowledge.deleteTitle', { name: '工程文档' }))).toBeInTheDocument();
    const deleteButtons = screen.getAllByRole('button', { name: tl('knowledge.delete') });
    fireEvent.click(deleteButtons[deleteButtons.length - 1]);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_delete', { kbId: state.kb.id }),
    );
    expect(onDeleted).toHaveBeenCalled();
  });
});
