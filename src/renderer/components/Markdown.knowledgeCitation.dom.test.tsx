import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { FileActionContext, type FileActionContextValue } from '@/context/fileActionState';
import { ToastProvider } from '@/components/Toast';
import { i18n } from '@/i18n';
import refsFixture from '../../shared/knowledge/__fixtures__/refs.json';

import Markdown from './Markdown';
import {
  clearKnowledgeCitationCache,
  type ResolvedKnowledgeRefPayload,
} from './markdown/KnowledgeCitationChip';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

function makeFileAction(openFileTarget = vi.fn()): FileActionContextValue {
  return {
    checkPath: () => null,
    checkFileTarget: () => null,
    subscribeFileTarget: () => () => {},
    cacheVersion: 0,
    refreshFileTarget: () => {},
    openFileTargetMenu: () => () => {},
    openFileTarget,
    workspacePath: '/workspace',
  };
}

describe('Markdown knowledge citation chips (K10)', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    clearKnowledgeCitationCache();
    await i18n.changeLanguage('zh-CN');
  });

  afterEach(() => {
    vi.useRealTimers();
    clearKnowledgeCitationCache();
  });

  it('AC-K10-1: renders every valid K0 fixture ref as a chip and keeps every invalid ref as plain text', () => {
    const fixtures = refsFixture as ReadonlyArray<{
      input: string;
      expected: { kbId: string; sourceId: string; page?: number; chunkId?: string } | null;
    }>;
    const validEntries = fixtures.filter(
      (entry): entry is { input: string; expected: { kbId: string; sourceId: string; page?: number; chunkId?: string } } =>
        entry.expected !== null,
    );
    const invalidEntries = fixtures.filter((entry) => entry.expected === null);
    expect(validEntries.length).toBeGreaterThanOrEqual(7);
    expect(invalidEntries.length).toBeGreaterThanOrEqual(30);

    for (const entry of validEntries) {
      const { container, unmount } = render(
        <Markdown>{`参见 [架构文档](${entry.input}) 以及裸写 ${entry.input}`}</Markdown>,
      );
      const chips = container.querySelectorAll('[data-testid="kb-citation-chip"]');
      expect(chips.length, `expected 2 chips for valid ref ${entry.input}`).toBe(2);
      expect(container.querySelectorAll('a').length).toBe(0);
      if (typeof entry.expected.page === 'number') {
        const pages = container.querySelectorAll('[data-testid="kb-citation-page"]');
        expect(pages.length).toBeGreaterThanOrEqual(1);
        expect(pages[0]).toHaveTextContent(`p.${entry.expected.page}`);
      }
      unmount();
    }

    for (const entry of invalidEntries) {
      const bad = entry.input;
      // Skip whitespace-padded variants of an otherwise valid ref in bare prose
      // (Markdown trims link destinations and prose separates tokens by spaces).
      if (bad.trim() !== bad) continue;

      // Explicit Markdown link form: must never render as a citation chip,
      // and any kb: scheme variant must degrade to plain text (no <a>).
      const explicit = render(<Markdown>{`[坏引用](${bad})`}</Markdown>);
      expect(
        explicit.container.querySelectorAll('[data-testid="kb-citation-chip"]').length,
        `unexpected chip for invalid explicit ref ${JSON.stringify(bad)}`,
      ).toBe(0);
      if (/^kb:/i.test(bad)) {
        expect(
          explicit.container.querySelectorAll('a').length,
          `unexpected <a> link for invalid explicit kb ref ${JSON.stringify(bad)}`,
        ).toBe(0);
      }
      explicit.unmount();

      // Bare text form: must never render as a citation chip, and kb: variants must not become <a>
      const bare = render(<Markdown>{`文本 ${bad} 结束`}</Markdown>);
      expect(
        bare.container.querySelectorAll('[data-testid="kb-citation-chip"]').length,
        `unexpected chip for invalid bare ref ${JSON.stringify(bad)}`,
      ).toBe(0);
      if (/^kb:/i.test(bad)) {
        expect(
          bare.container.querySelectorAll('a').length,
          `unexpected <a> link for invalid bare kb ref ${JSON.stringify(bad)}`,
        ).toBe(0);
      }
      bare.unmount();
    }

    // Code spans must remain literal and never turn into chips
    const validRef = validEntries[0].input;
    const codeRender = render(<Markdown>{`\`${validRef}\``}</Markdown>);
    expect(codeRender.container.querySelectorAll('[data-testid="kb-citation-chip"]').length).toBe(0);
    codeRender.unmount();
  });

  it('AC-K10-2: cancels hover resolution on fast leave, caches hits, and ignores late replies after unmount', async () => {
    vi.useFakeTimers();
    const validRef = 'kb://kb_0123456789ab/src_0123456789abcdef?page=2#fedcba9876543210';

    // 1. Fast hover pass (<120ms) cancels debounce before invoking Tauri IPC
    const { rerender, unmount } = render(
      <Markdown>{`请看 [合同条款](${validRef})`}</Markdown>,
    );
    const chip = screen.getByTestId('kb-citation-chip');
    fireEvent.mouseEnter(chip);
    await act(async () => {
      vi.advanceTimersByTime(60);
    });
    fireEvent.mouseLeave(chip);
    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    expect(mocks.invoke).not.toHaveBeenCalled();

    // 2. Hover long enough to fire IPC, then unmount while request is in-flight -> no setState
    let resolveInFlight!: (value: ResolvedKnowledgeRefPayload) => void;
    mocks.invoke.mockImplementationOnce(
      () =>
        new Promise<ResolvedKnowledgeRefPayload>((resolve) => {
          resolveInFlight = resolve;
        }),
    );

    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    fireEvent.mouseEnter(chip);
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_resolve_ref', { reference: validRef });

    unmount();
    await act(async () => {
      resolveInFlight({
        reference: {
          kbId: 'kb_0123456789ab',
          sourceId: 'src_0123456789abcdef',
          page: 2,
          chunkId: 'fedcba9876543210',
        },
        source: {
          id: 'src_0123456789abcdef',
          displayName: 'contract.pdf',
          status: 'ready',
        },
        excerpt: '第二页合同核心条款摘要内容。',
        preview: {
          path: '/data/knowledge/kb_0123456789ab/sources/src_0123456789abcdef/original.pdf',
          name: 'contract.pdf',
          isContentFallback: false,
          page: 2,
        },
      });
      await Promise.resolve();
    });
    expect(errorSpy).not.toHaveBeenCalled();
    errorSpy.mockRestore();

    // 3. Re-mounting reuses module cache without issuing another IPC call
    const second = render(<Markdown>{`再次查看 [合同条款](${validRef})`}</Markdown>);
    const chip2 = second.getByTestId('kb-citation-chip');
    fireEvent.mouseEnter(chip2);
    expect(second.getByTestId('kb-citation-excerpt')).toHaveTextContent('第二页合同核心条款摘要内容。');
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    void rerender;
    second.unmount();
  });

  it('opens PDF page and Markdown utf16Range via cmd_kb_resolve_ref preview without synthesizing paths, and handles deleted/updated refs gracefully', async () => {
    const openFileTarget = vi.fn();
    const pdfRef = 'kb://kb_0123456789ab/src_0123456789abcdef?page=3';
    const mdRef = 'kb://kb_0123456789ab/src_1111111111111111#aaaaaaaaaaaaaaaa';
    const deletedRef = 'kb://kb_0123456789ab/src_2222222222222222';

    mocks.invoke.mockImplementation(async (cmd: string, args?: { reference?: string }) => {
      if (cmd !== 'cmd_kb_resolve_ref') throw new Error(`unexpected ${cmd}`);
      if (args?.reference === pdfRef) {
        return {
          reference: { kbId: 'kb_0123456789ab', sourceId: 'src_0123456789abcdef', page: 3 },
          source: { id: 'src_0123456789abcdef', displayName: 'manual.pdf', status: 'ready' },
          excerpt: '第三页安装步骤。',
          preview: {
            path: '/managed/kb/original.pdf',
            name: 'manual.pdf',
            isContentFallback: false,
            page: 3,
          },
        } satisfies ResolvedKnowledgeRefPayload;
      }
      if (args?.reference === mdRef) {
        return {
          reference: {
            kbId: 'kb_0123456789ab',
            sourceId: 'src_1111111111111111',
            chunkId: 'aaaaaaaaaaaaaaaa',
          },
          source: { id: 'src_1111111111111111', displayName: 'notes.md', status: 'ready' },
          fragment: { status: 'unavailable' },
          excerpt: '已更新来源摘要。',
          preview: {
            path: '/managed/kb/content.md',
            name: 'notes.md',
            isContentFallback: true,
            lineNumber: 5,
            utf16Range: [18, 42],
          },
        } satisfies ResolvedKnowledgeRefPayload;
      }
      throw 'KB_NOT_FOUND';
    });

    render(
      <ToastProvider>
        <FileActionContext.Provider value={makeFileAction(openFileTarget)}>
          <Markdown>
            {`[手册](${pdfRef}) 和 [笔记](${mdRef}) 以及 [已删来源](${deletedRef})`}
          </Markdown>
        </FileActionContext.Provider>
      </ToastProvider>,
    );

    const chips = screen.getAllByTestId('kb-citation-chip');
    expect(chips).toHaveLength(3);

    // Click PDF ref
    fireEvent.click(chips[0]);
    await waitFor(() =>
      expect(openFileTarget).toHaveBeenCalledWith(
        { scope: 'local', path: '/managed/kb/original.pdf', initialLineNumber: undefined },
        expect.objectContaining({ displayName: 'manual.pdf', page: 3 }),
      ),
    );

    // Click updated-fragment Markdown ref -> opens preview + shows notice
    fireEvent.click(chips[1]);
    await waitFor(() =>
      expect(openFileTarget).toHaveBeenCalledWith(
        { scope: 'local', path: '/managed/kb/content.md', initialLineNumber: 5 },
        expect.objectContaining({ displayName: 'notes.md', utf16Range: [18, 42] }),
      ),
    );
    expect(await screen.findByText('片段已更新，已为你打开来源文档。')).toBeInTheDocument();

    // Click deleted source ref -> chip switches to unavailable without throwing
    fireEvent.click(chips[2]);
    await waitFor(() => expect(chips[2]).toHaveAttribute('data-kb-unavailable', 'true'));
    expect(chips[2]).toHaveTextContent('来源不可用');
  });

  it('highlights only the matching Markdown block when highlightUtf16Range is provided', () => {
    const doc = '# 标题\n\n第一段普通内容。\n\n第二段命中目标内容。\n\n第三段无关内容。\n';
    const start = doc.indexOf('第二段命中目标内容。');
    const end = start + '第二段命中目标内容。'.length;

    const { container } = render(
      <Markdown raw preserveNewlines highlightUtf16Range={[start, end]}>
        {doc}
      </Markdown>,
    );

    const highlighted = container.querySelectorAll('[data-kb-highlight="true"]');
    expect(highlighted).toHaveLength(1);
    expect(highlighted[0]).toHaveTextContent('第二段命中目标内容。');
    expect(highlighted[0]).toHaveClass('kb-citation-highlight');
  });
});
