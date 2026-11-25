import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import CustomSelect, { type SelectOption } from '@/components/CustomSelect';
import { LoaderIcon, SearchIcon, SlidersIcon } from '@/components/icons';

import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type { KnowledgeSearchHit, KnowledgeSearchResult } from './types';

interface KnowledgeSearchTabProps {
  kbId: string;
  approvedCategories: string[];
  onInspectHit: (sourceId: string, chunkId: string) => void;
}

export default function KnowledgeSearchTab({
  kbId,
  approvedCategories,
  onInspectHit,
}: KnowledgeSearchTabProps) {
  const { t } = useTranslation('launcher');
  const [queryText, setQueryText] = useState('');
  const [showFilters, setShowFilters] = useState(false);
  const [category, setCategory] = useState('');
  const [tag, setTag] = useState('');
  const [docType, setDocType] = useState('');
  const [limit, setLimit] = useState(8);
  const [searching, setSearching] = useState(false);
  const [compareMode, setCompareMode] = useState(false);
  const [result, setResult] = useState<KnowledgeSearchResult | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);

  const mounted = useRef(false);
  const searchSeq = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const runSearch = async () => {
    const queries = queryText
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean);
    if (queries.length === 0) return;

    const seq = ++searchSeq.current;
    setSearching(true);
    setErrorCode(null);
    try {
      const res = await knowledgeClient.search(
        [kbId],
        queries,
        {
          category: category.trim() || undefined,
          tag: tag.trim() || undefined,
          docType: docType.trim() || undefined,
        },
        limit,
      );
      if (!mounted.current || searchSeq.current !== seq) return;
      setResult(res);
    } catch (error) {
      if (!mounted.current || searchSeq.current !== seq) return;
      setErrorCode(knowledgeErrorCode(error));
      setResult(null);
    } finally {
      if (mounted.current && searchSeq.current === seq) setSearching(false);
    }
  };

  const categoryOptions = useMemo<SelectOption[]>(
    () => [
      { value: '', label: t('knowledge.search.allOption') },
      ...approvedCategories.map((cat) => ({ value: cat, label: cat })),
    ],
    [approvedCategories, t],
  );
  const limitOptions = useMemo<SelectOption[]>(
    () => [5, 8, 12, 20].map((n) => ({ value: String(n), label: String(n) })),
    [],
  );

  const renderHit = (hit: KnowledgeSearchHit) => (
    <li
      key={`${hit.sourceId}-${hit.chunkId}`}
      data-testid="kb-search-hit"
      className="rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4 transition-colors hover:border-[var(--line-strong)]"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-[var(--ink)]">{hit.title}</div>
          {hit.prefix && <p className="mt-0.5 truncate text-xs text-[var(--accent)]">{hit.prefix}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-2 text-xs text-[var(--ink-muted)]">
          {typeof hit.page === 'number' && (
            <span className="rounded bg-[var(--paper-inset)] px-1.5 py-0.5 text-[var(--ink-secondary)]">
              {t('knowledge.search.pageLabel', { page: hit.page })}
            </span>
          )}
          <span className="font-mono">{t('knowledge.search.scoreLabel', { score: hit.score.toFixed(4) })}</span>
        </div>
      </div>

      <p className="mt-2 whitespace-pre-wrap break-words text-sm text-[var(--ink-secondary)]">
        {hit.snippet}
      </p>

      <div className="mt-2.5 flex flex-wrap items-center justify-between gap-2 text-xs">
        <span className="truncate font-mono text-[var(--ink-subtle)]">{hit.reference}</span>
        <button
          type="button"
          onClick={() => onInspectHit(hit.sourceId, hit.chunkId)}
          className="rounded-md bg-[var(--button-secondary-bg)] px-2.5 py-1 text-xs text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--button-secondary-bg-hover)]"
        >
          {t('knowledge.search.openSource')}
        </button>
      </div>
    </li>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden" data-testid="knowledge-search-tab">
      <form
        className="shrink-0 space-y-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4"
        onSubmit={(event) => {
          event.preventDefault();
          void runSearch();
        }}
      >
        <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
          <textarea
            rows={2}
            value={queryText}
            onChange={(e) => setQueryText(e.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                event.preventDefault();
                void runSearch();
              }
            }}
            aria-label={t('knowledge.search.queryLabel')}
            placeholder={t('knowledge.search.queryPlaceholder')}
            className="min-w-0 flex-1 resize-none rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-3 py-2 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
          />
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              onClick={() => setShowFilters((prev) => !prev)}
              aria-expanded={showFilters}
              className="inline-flex h-10 items-center gap-1.5 rounded-lg bg-[var(--button-secondary-bg)] px-3 text-sm text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--button-secondary-bg-hover)]"
            >
              <SlidersIcon className="h-4 w-4" />
              {t('knowledge.search.filtersToggle')}
            </button>
            <button
              type="submit"
              disabled={searching || !queryText.trim()}
              className="inline-flex h-10 items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-4 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
            >
              {searching ? (
                <LoaderIcon className="h-4 w-4 animate-spin" />
              ) : (
                <SearchIcon className="h-4 w-4" />
              )}
              {searching ? t('knowledge.search.searching') : t('knowledge.search.run')}
            </button>
          </div>
        </div>

        {showFilters && (
          <div className="grid grid-cols-1 gap-3 border-t border-[var(--line)] pt-3 sm:grid-cols-4">
            <div className="block text-xs text-[var(--ink-muted)]">
              <span className="mb-1 block">{t('knowledge.search.categoryFilter')}</span>
              <CustomSelect
                value={category}
                options={categoryOptions}
                onChange={setCategory}
                ariaLabel={t('knowledge.search.categoryFilter')}
                size="sm"
                className="w-full"
              />
            </div>

            <label className="block text-xs text-[var(--ink-muted)]">
              <span className="mb-1 block">{t('knowledge.search.tagFilter')}</span>
              <input
                value={tag}
                onChange={(e) => setTag(e.target.value)}
                className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-2.5 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
              />
            </label>

            <label className="block text-xs text-[var(--ink-muted)]">
              <span className="mb-1 block">{t('knowledge.search.docTypeFilter')}</span>
              <input
                value={docType}
                onChange={(e) => setDocType(e.target.value)}
                className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-2.5 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
              />
            </label>

            <div className="block text-xs text-[var(--ink-muted)]">
              <span className="mb-1 block">{t('knowledge.search.limitLabel')}</span>
              <CustomSelect
                value={String(limit)}
                options={limitOptions}
                onChange={(next) => setLimit(Number(next))}
                ariaLabel={t('knowledge.search.limitLabel')}
                size="sm"
                className="w-full"
              />
            </div>
          </div>
        )}
      </form>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {errorCode === 'KB_INDEXING' ? (
          <div role="status" className="rounded-xl border border-[var(--line)] p-6 text-center text-sm text-[var(--ink-muted)]">
            <p>{t('knowledge.search.indexingHint')}</p>
            <button
              type="button"
              onClick={() => void runSearch()}
              className="mt-3 rounded-lg bg-[var(--button-secondary-bg)] px-3 py-1.5 text-sm text-[var(--button-secondary-text)]"
            >
              {t('knowledge.retry')}
            </button>
          </div>
        ) : errorCode ? (
          <div role="alert" className="rounded-xl border border-[var(--line)] p-6 text-center text-sm text-[var(--error)]">
            {t(`knowledge.errors.${errorCode}`, { defaultValue: t('knowledge.errors.unknown') })}
          </div>
        ) : result === null ? (
          <p className="py-12 text-center text-sm text-[var(--ink-muted)]">
            {t('knowledge.search.emptyPrompt')}
          </p>
        ) : (
          <div className="space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-[var(--ink-muted)]">
              <span>{t('knowledge.search.hitsSummary', { count: result.hits.length })}</span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => setCompareMode((prev) => !prev)}
                  aria-pressed={compareMode}
                  data-testid="kb-search-compare-toggle"
                  className={`rounded-md border px-2 py-0.5 text-xs font-medium transition-colors ${
                    compareMode
                      ? 'border-[var(--accent)] bg-[var(--paper-inset)] text-[var(--ink)]'
                      : 'border-[var(--line)] text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]'
                  }`}
                >
                  {t('knowledge.search.compareColumnsToggle')}
                </button>
                <span
                  data-testid="kb-search-mode"
                  className="rounded-md bg-[var(--paper-inset)] px-2 py-0.5 font-medium text-[var(--ink-secondary)]"
                >
                  {t('knowledge.search.modeBadge', {
                    mode: t(`knowledge.searchModes.${result.mode}`, {
                      defaultValue: result.mode.toUpperCase(),
                    }),
                  })}
                </span>
                {result.mode === 'bm25' && result.degradedReason && (
                  <span
                    data-testid="kb-search-degraded-reason"
                    className="rounded-md bg-[var(--warning-bg)] px-2 py-0.5 text-xs text-[var(--warning)]"
                  >
                    {t(`knowledge.search.degradedReasons.${result.degradedReason}`, {
                      defaultValue: result.degradedReason,
                    })}
                  </span>
                )}
              </div>
            </div>
            {compareMode ? (
              <div
                data-testid="kb-search-compare-grid"
                className="grid grid-cols-1 gap-3 lg:grid-cols-3"
              >
                {[
                  {
                    key: 'bm25',
                    title: t('knowledge.search.columnBm25'),
                    items: result.bm25Hits ?? result.hits,
                    emptyText: t('knowledge.search.noHits'),
                  },
                  {
                    key: 'vector',
                    title: t('knowledge.search.columnVector'),
                    items: result.vectorHits ?? [],
                    emptyText: t('knowledge.search.vectorDegradedHint'),
                  },
                  {
                    key: 'hybrid',
                    title: t('knowledge.search.columnHybrid'),
                    items: result.hits,
                    emptyText: t('knowledge.search.noHits'),
                  },
                ].map((col) => (
                  <div
                    key={col.key}
                    data-testid={`kb-search-col-${col.key}`}
                    className="space-y-2 rounded-xl border border-[var(--line)] bg-[var(--paper)] p-3"
                  >
                    <div className="flex items-center justify-between text-xs font-semibold text-[var(--ink-muted)]">
                      <span>{col.title}</span>
                      <span>{col.items.length}</span>
                    </div>
                    {col.items.length === 0 ? (
                      <p className="py-6 text-center text-xs text-[var(--ink-muted)]">
                        {col.emptyText}
                      </p>
                    ) : (
                      <ul className="space-y-2">{col.items.map(renderHit)}</ul>
                    )}
                  </div>
                ))}
              </div>
            ) : result.hits.length === 0 ? (
              <p className="py-10 text-center text-sm text-[var(--ink-muted)]">
                {t('knowledge.search.noHits')}
              </p>
            ) : (
              <ul className="space-y-2">{result.hits.map(renderHit)}</ul>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
