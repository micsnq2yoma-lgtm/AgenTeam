import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import Markdown from '@/components/Markdown';
import {
  AlertIcon,
  CheckIcon,
  CloseIcon,
  NotebookIcon,
  WarningIcon,
} from '@/components/icons';
import { useToast } from '@/components/Toast';

import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type { KnowledgeBaseRecord, LintReportEntry, WikiPageEntry } from './types';

type WikiSubTab = 'pages' | 'lint';

interface KnowledgeWikiTabProps {
  kb: KnowledgeBaseRecord;
  onMutated: () => Promise<void>;
}

export default function KnowledgeWikiTab({ kb, onMutated }: KnowledgeWikiTabProps) {
  const { t } = useTranslation('launcher');
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const [subTab, setSubTab] = useState<WikiSubTab>('pages');
  const [pages, setPages] = useState<WikiPageEntry[] | null>(null);
  const [selectedPage, setSelectedPage] = useState<string | null>(null);
  const [pageContent, setPageContent] = useState<string | null>(null);
  const [pageLoading, setPageLoading] = useState(false);
  const [reports, setReports] = useState<LintReportEntry[] | null>(null);
  const [busyCategory, setBusyCategory] = useState<string | null>(null);

  const mounted = useRef(false);
  const listSeq = useRef(0);
  const pageSeq = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const loadWikiData = useCallback(async () => {
    const seq = ++listSeq.current;
    try {
      const [pageList, lintList] = await Promise.all([
        knowledgeClient.listWikiPages(kb.id),
        knowledgeClient.listLintReports(kb.id),
      ]);
      if (!mounted.current || listSeq.current !== seq) return;
      setPages(pageList);
      setReports(lintList);
      setSelectedPage((current) => {
        if (current && pageList.some((p) => p.path === current)) return current;
        return pageList[0]?.path ?? null;
      });
    } catch {
      if (mounted.current && listSeq.current === seq) {
        setPages([]);
        setReports([]);
      }
    }
  }, [kb.id]);

  useEffect(() => {
    void loadWikiData();
  }, [loadWikiData]);

  useEffect(() => {
    if (!selectedPage) {
      setPageContent(null);
      return;
    }
    const seq = ++pageSeq.current;
    setPageLoading(true);
    void (async () => {
      try {
        const text = await knowledgeClient.readWikiPage(kb.id, selectedPage);
        if (!mounted.current || pageSeq.current !== seq) return;
        setPageContent(text);
      } catch {
        if (mounted.current && pageSeq.current === seq) setPageContent(null);
      } finally {
        if (mounted.current && pageSeq.current === seq) setPageLoading(false);
      }
    })();
  }, [kb.id, selectedPage]);

  const handleConfirmCategory = async (category: string) => {
    if (busyCategory) return;
    setBusyCategory(category);
    try {
      await knowledgeClient.confirmCategory(kb.id, category);
      if (!mounted.current) return;
      toastRef.current.success(t('knowledge.wiki.categoryConfirmed', { category }));
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusyCategory(null);
    }
  };

  const handleDismissCategory = async (category: string) => {
    if (busyCategory) return;
    setBusyCategory(category);
    try {
      await knowledgeClient.dismissCategory(kb.id, category);
      if (!mounted.current) return;
      toastRef.current.success(t('knowledge.wiki.categoryDismissed', { category }));
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusyCategory(null);
    }
  };

  const openWikiPageFromLint = (pagePath: string) => {
    setSelectedPage(pagePath);
    setSubTab('pages');
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden" data-testid="knowledge-wiki-tab">
      {kb.pendingCategories.length > 0 && (
        <section
          data-testid="kb-pending-categories"
          className="shrink-0 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4"
        >
          <div className="flex items-center gap-2 text-sm font-medium text-[var(--ink)]">
            <WarningIcon className="h-4 w-4 text-[var(--warning)]" />
            <span>
              {t('knowledge.wiki.pendingTitle', { count: kb.pendingCategories.length })}
            </span>
          </div>
          <p className="mt-0.5 text-xs text-[var(--ink-muted)]">
            {t('knowledge.wiki.pendingHint')}
          </p>
          <ul className="mt-3 space-y-2">
            {kb.pendingCategories.map((category) => {
              const reason = kb.pendingCategoryReasons?.[category];
              const isBusy = busyCategory === category;
              return (
                <li
                  key={category}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-[var(--line)] bg-[var(--paper)] px-3 py-2"
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-mono text-sm font-medium text-[var(--ink)]">
                      {category}
                    </div>
                    {reason && (
                      <p className="mt-0.5 text-xs text-[var(--ink-muted)]">{reason}</p>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                    <button
                      type="button"
                      disabled={isBusy}
                      onClick={() => void handleConfirmCategory(category)}
                      className="inline-flex items-center gap-1 rounded-lg bg-[var(--button-primary-bg)] px-2.5 py-1 text-xs font-medium text-[var(--button-primary-text)] hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
                    >
                      <CheckIcon className="h-3.5 w-3.5" />
                      {t('knowledge.wiki.confirmCategory')}
                    </button>
                    <button
                      type="button"
                      disabled={isBusy}
                      onClick={() => void handleDismissCategory(category)}
                      className="inline-flex items-center gap-1 rounded-lg bg-[var(--button-secondary-bg)] px-2.5 py-1 text-xs text-[var(--button-secondary-text)] hover:bg-[var(--button-secondary-bg-hover)] disabled:opacity-50"
                    >
                      <CloseIcon className="h-3.5 w-3.5" />
                      {t('knowledge.wiki.dismissCategory')}
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {!kb.wikiEnabled && (
        <div className="shrink-0 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] px-4 py-3 text-xs text-[var(--ink-muted)]">
          {t('knowledge.wiki.disabledHint')}
        </div>
      )}

      <div className="flex shrink-0 items-center gap-1" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={subTab === 'pages'}
          onClick={() => setSubTab('pages')}
          className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
            subTab === 'pages'
              ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
              : 'text-[var(--ink-muted)] hover:text-[var(--ink)]'
          }`}
        >
          <NotebookIcon className="h-3.5 w-3.5" />
          {t('knowledge.wiki.pagesTab')}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={subTab === 'lint'}
          onClick={() => setSubTab('lint')}
          className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
            subTab === 'lint'
              ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
              : 'text-[var(--ink-muted)] hover:text-[var(--ink)]'
          }`}
        >
          <AlertIcon className="h-3.5 w-3.5" />
          {t('knowledge.wiki.lintTab')}
          {reports && reports.length > 0 && (
            <span className="rounded-full bg-[var(--paper-elevated)] px-1.5 text-xs text-[var(--ink-muted)]">
              {reports.length}
            </span>
          )}
        </button>
      </div>

      {subTab === 'pages' ? (
        pages === null ? (
          <p className="py-10 text-center text-sm text-[var(--ink-muted)]" role="status">
            {t('knowledge.loading')}
          </p>
        ) : pages.length === 0 ? (
          <div className="rounded-xl border border-dashed border-[var(--line-strong)] px-6 py-12 text-center text-sm text-[var(--ink-muted)]">
            {t('knowledge.wiki.emptyPages')}
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden md:flex-row">
            <aside className="shrink-0 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-2 md:w-56 md:overflow-y-auto">
              <ul className="space-y-0.5">
                {pages.map((entry) => (
                  <li key={entry.path}>
                    <button
                      type="button"
                      onClick={() => setSelectedPage(entry.path)}
                      className={`w-full truncate rounded-lg px-2.5 py-1.5 text-left text-sm transition-colors ${
                        selectedPage === entry.path
                          ? 'bg-[var(--paper-inset)] font-medium text-[var(--ink)]'
                          : 'text-[var(--ink-secondary)] hover:bg-[var(--hover-bg)]'
                      }`}
                    >
                      {entry.path}
                    </button>
                  </li>
                ))}
              </ul>
            </aside>

            <div className="min-h-0 min-w-0 flex-1 overflow-y-auto rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
              {!selectedPage ? (
                <p className="text-sm text-[var(--ink-muted)]">{t('knowledge.wiki.selectPage')}</p>
              ) : pageLoading ? (
                <p className="text-sm text-[var(--ink-muted)]" role="status">
                  {t('knowledge.loading')}
                </p>
              ) : pageContent !== null ? (
                <div className="ai-message-content">
                  <Markdown raw>{pageContent}</Markdown>
                </div>
              ) : (
                <p className="text-sm text-[var(--ink-muted)]">{t('knowledge.errors.KB_NOT_FOUND')}</p>
              )}
            </div>
          </div>
        )
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {reports === null ? (
            <p className="py-10 text-center text-sm text-[var(--ink-muted)]" role="status">
              {t('knowledge.loading')}
            </p>
          ) : reports.length === 0 ? (
            <div className="rounded-xl border border-dashed border-[var(--line-strong)] px-6 py-12 text-center text-sm text-[var(--ink-muted)]">
              {t('knowledge.wiki.emptyLint')}
            </div>
          ) : (
            <div className="space-y-3">
              {reports.map(({ id, report }) => (
                <section
                  key={id}
                  data-testid="kb-lint-report"
                  className="rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-[var(--ink-muted)]">
                    <span className="font-mono">{report.generatedAt}</span>
                    <span>
                      {report.items.length === 0
                        ? t('knowledge.wiki.lintClean')
                        : t('knowledge.wiki.lintSummary', { count: report.items.length })}
                    </span>
                  </div>
                  {report.items.length > 0 && (
                    <ul className="mt-3 space-y-2">
                      {report.items.map((item, idx) => (
                        <li
                          key={`${id}-${idx}`}
                          className="rounded-lg border border-[var(--line)] bg-[var(--paper)] p-3"
                        >
                          <div className="flex flex-wrap items-center justify-between gap-2">
                            <div className="flex items-center gap-2">
                              <span className="rounded-md bg-[var(--warning-bg)] px-1.5 py-0.5 text-xs font-medium text-[var(--warning)]">
                                {t(`knowledge.wiki.lintKinds.${item.kind}`)}
                              </span>
                              <span className="font-mono text-xs text-[var(--ink-secondary)]">
                                {item.page}
                              </span>
                            </div>
                            <button
                              type="button"
                              onClick={() => openWikiPageFromLint(item.page)}
                              className="rounded-md bg-[var(--button-secondary-bg)] px-2 py-0.5 text-xs text-[var(--button-secondary-text)] hover:bg-[var(--button-secondary-bg-hover)]"
                            >
                              {t('knowledge.wiki.openPage')}
                            </button>
                          </div>
                          <p className="mt-1.5 text-sm text-[var(--ink)]">{item.message}</p>
                          {item.refs.length > 0 && (
                            <div className="mt-1.5 flex flex-wrap gap-1.5 font-mono text-xs text-[var(--ink-subtle)]">
                              {item.refs.map((ref) => (
                                <span key={ref} className="rounded bg-[var(--paper-inset)] px-1.5 py-0.5">
                                  {ref}
                                </span>
                              ))}
                            </div>
                          )}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
