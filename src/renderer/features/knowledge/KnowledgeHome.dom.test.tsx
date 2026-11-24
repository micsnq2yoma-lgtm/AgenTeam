import { StrictMode } from 'react';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { ToastProvider } from '@/components/Toast';

import KnowledgeHome from './KnowledgeHome';
import type { KnowledgeBaseRecord } from './types';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

const base = (over: Partial<KnowledgeBaseRecord> = {}): KnowledgeBaseRecord => ({
  id: 'kb_one', name: '产品资料', description: '发布说明与设计文档', curationMode: 'manual',
  curationIntervalMinutes: 4320, agentWriteAccess: 'curator-only', wikiEnabled: true,
  pendingCategories: [], ...over,
});
const tl = (key: string, vars?: Record<string, unknown>) => String(i18n.t(`launcher:${key}`, vars));

function backend(state: { bases: KnowledgeBaseRecord[]; sources?: Record<string, number> }) {
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case 'cmd_kb_list': return state.bases;
      case 'cmd_kb_list_sources': return Array.from({ length: state.sources?.[args?.kbId as string] ?? 0 }, (_, i) => ({ id: `src_${i}` }));
      case 'cmd_kb_create': {
        const settings = args?.settings as { name: string; description?: string };
        const created = base({ id: 'kb_new', name: settings.name, description: settings.description ?? '' });
        state.bases = [...state.bases, created];
        return created;
      }
      case 'cmd_kb_delete': state.bases = state.bases.filter((b) => b.id !== args?.kbId); return undefined;
      default: throw new Error(`unexpected ${command}`);
    }
  });
}

const renderHome = () => render(<ToastProvider><KnowledgeHome /></ToastProvider>);

describe('KnowledgeHome', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage('zh-CN');
  });

  it('shows the empty state with a create entry when there is no library', async () => {
    backend({ bases: [] });
    renderHome();
    expect(await screen.findByTestId('knowledge-empty')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: tl('knowledge.create') })).toBeInTheDocument();
  });

  it('lists libraries with source count, curation mode and pending categories read from the Store', async () => {
    backend({ bases: [base({ pendingCategories: ['合同'] })], sources: { kb_one: 3 } });
    renderHome();
    const row = await screen.findByTestId('knowledge-row');
    expect(within(row).getByText('产品资料')).toBeInTheDocument();
    expect(within(row).getByText('发布说明与设计文档')).toBeInTheDocument();
    await waitFor(() => expect(within(row).getByText(tl('knowledge.sourceCount', { count: 3 }))).toBeInTheDocument());
    expect(within(row).getByText(tl('knowledge.curationLabel', { mode: tl('knowledge.curation.manual') }))).toBeInTheDocument();
    expect(within(row).getByText(tl('knowledge.pendingCategories', { count: 1 }))).toBeInTheDocument();
  });

  it('says so, and offers a retry, when the list cannot be read', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('store unavailable'));
    renderHome();
    expect(await screen.findByRole('alert')).toHaveTextContent(tl('knowledge.loadFailed'));
    backend({ bases: [base()] });
    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.retry') }));
    expect(await screen.findByTestId('knowledge-row')).toBeInTheDocument();
  });

  it('creates a library through cmd_kb_create and refetches the list instead of patching it locally', async () => {
    backend({ bases: [] });
    renderHome();
    await screen.findByTestId('knowledge-empty');
    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.create') }));
    const confirm = screen.getByRole('button', { name: tl('knowledge.createConfirm') });
    expect(confirm).toBeDisabled();
    fireEvent.change(screen.getByLabelText(tl('knowledge.nameLabel')), { target: { value: '  研究笔记 ' } });
    fireEvent.change(screen.getByLabelText(tl('knowledge.descriptionLabel')), { target: { value: '论文摘录' } });
    fireEvent.click(confirm);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_create', { settings: { name: '研究笔记', description: '论文摘录' } }));
    expect(await screen.findByText('研究笔记')).toBeInTheDocument();
    expect(mocks.invoke.mock.calls.filter(([command]) => command === 'cmd_kb_list').length).toBeGreaterThanOrEqual(2);
  });

  it('asks before deleting, deletes only on confirmation, and keeps the library when cancelled', async () => {
    const state = { bases: [base()] };
    backend(state);
    renderHome();
    fireEvent.click(await screen.findByRole('button', { name: tl('knowledge.deleteAria', { name: '产品资料' }) }));
    expect(await screen.findByText(tl('knowledge.deleteTitle', { name: '产品资料' }))).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.cancel') }));
    expect(mocks.invoke).not.toHaveBeenCalledWith('cmd_kb_delete', expect.anything());
    expect(screen.getByText('产品资料')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.deleteAria', { name: '产品资料' }) }));
    fireEvent.click(await screen.findByRole('button', { name: tl('knowledge.delete') }));
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_delete', { kbId: 'kb_one' }));
    expect(await screen.findByTestId('knowledge-empty')).toBeInTheDocument();
  });

  it('loads cleanly under React StrictMode and discards stale overlapping reloads', async () => {
    let resolveFirst!: (value: KnowledgeBaseRecord[]) => void;
    let call = 0;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'cmd_kb_list') {
        call += 1;
        if (call === 1) {
          return new Promise<KnowledgeBaseRecord[]>((resolve) => { resolveFirst = resolve; });
        }
        return Promise.resolve([base({ id: 'kb_fresh', name: '最新库' })]);
      }
      if (command === 'cmd_kb_list_sources') return Promise.resolve([]);
      throw new Error(`unexpected ${command}`);
    });
    render(<StrictMode><ToastProvider><KnowledgeHome /></ToastProvider></StrictMode>);
    expect(await screen.findByText('最新库')).toBeInTheDocument();
    resolveFirst([base({ id: 'kb_stale', name: '过期库' })]);
    await waitFor(() => {
      expect(screen.getByText('最新库')).toBeInTheDocument();
      expect(screen.queryByText('过期库')).not.toBeInTheDocument();
    });
  });
});
