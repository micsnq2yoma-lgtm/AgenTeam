import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';

import {
  ArrowLeftIcon,
  FolderIcon,
  NotebookIcon,
  SearchIcon,
  SettingsIcon,
  UploadIcon,
} from '@/components/icons';
import { useToast } from '@/components/Toast';

import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import KnowledgeSearchTab from './KnowledgeSearchTab';
import KnowledgeSettingsTab from './KnowledgeSettingsTab';
import KnowledgeSourcesTab from './KnowledgeSourcesTab';
import KnowledgeWikiTab from './KnowledgeWikiTab';
import SourceDetailDrawer from './SourceDetailDrawer';
import type { KnowledgeBaseRecord, KnowledgeChangeEvent, KnowledgeSourceView } from './types';
import { useKnowledgeChangeSubscription } from './useKnowledgeEvents';

export type KnowledgeDetailTab = 'sources' | 'search' | 'wiki' | 'settings';

interface DrawerState {
  sourceId: string;
  initialTab: 'metadata' | 'chunks';
  highlightChunkId?: string;
}

interface KnowledgeDetailProps {
  kbId: string;
  initialBase?: KnowledgeBaseRecord;
  isActive: boolean;
  onBack: () => void;
  onDeleted: () => void;
}

export default function KnowledgeDetail({
  kbId,
  initialBase,
  isActive,
  onBack,
  onDeleted,
}: KnowledgeDetailProps) {
  const { t } = useTranslation('launcher');
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const onBackRef = useRef(onBack);
  useLayoutEffect(() => {
    onBackRef.current = onBack;
  }, [onBack]);

  const [tab, setTab] = useState<KnowledgeDetailTab>('sources');
  const [kb, setKb] = useState<KnowledgeBaseRecord | null>(initialBase ?? null);
  const [sources, setSources] = useState<KnowledgeSourceView[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [importing, setImporting] = useState(false);
  const [drawer, setDrawer] = useState<DrawerState | null>(null);

  const mounted = useRef(false);
  const loadSeq = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const reload = useCallback(async () => {
    const seq = ++loadSeq.current;
    try {
      const [bases, sourceList, categoryList] = await Promise.all([
        knowledgeClient.list(),
        knowledgeClient.listSourceViews(kbId),
        knowledgeClient.listCategories(kbId),
      ]);
      if (!mounted.current || loadSeq.current !== seq) return;
      const found = bases.find((item) => item.id === kbId) ?? null;
      if (!found) {
        onBackRef.current();
        return;
      }
      setKb(found);
      setSources(sourceList);
      setCategories(categoryList);
      setLoadFailed(false);
      setLoading(false);
    } catch {
      if (mounted.current && loadSeq.current === seq) {
        setLoadFailed(true);
        setLoading(false);
      }
    }
  }, [kbId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const handleStoreEvent = useCallback(
    (event: KnowledgeChangeEvent) => {
      if (event.kind === 'kb-removed' && event.kbId === kbId) {
        onBackRef.current();
        return;
      }
      void reload();
    },
    [kbId, reload],
  );
  useKnowledgeChangeSubscription(handleStoreEvent, kbId);

  const handleImportPaths = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0 || importing) return;
      setImporting(true);
      try {
        const outcomes = await knowledgeClient.importFiles(kbId, paths);
        if (!mounted.current) return;
        let accepted = 0;
        let duplicates = 0;
        let rejected = 0;
        for (const item of outcomes) {
          if (item.outcome === 'accepted') {
            if (item.receipt.duplicate) duplicates += 1;
            else accepted += 1;
          } else {
            rejected += 1;
          }
        }
        if (accepted > 0) {
          toastRef.current.success(t('knowledge.detail.importAccepted', { count: accepted }));
        }
        if (duplicates > 0) {
          toastRef.current.info(t('knowledge.detail.importDuplicates', { count: duplicates }));
        }
        if (rejected > 0) {
          toastRef.current.error(t('knowledge.detail.importRejected', { count: rejected }));
        }
        await reload();
      } catch (error) {
        toastRef.current.error(
          t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
            defaultValue: t('knowledge.errors.unknown'),
          }),
        );
      } finally {
        if (mounted.current) setImporting(false);
      }
    },
    [importing, kbId, reload, t],
  );

  const handlePickFiles = useCallback(async () => {
    try {
      const selected = await openFileDialog({
        multiple: true,
        directory: false,
        title: t('knowledge.detail.importDialogTitle'),
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length > 0) {
        await handleImportPaths(paths);
      }
    } catch {
      toastRef.current.error(t('knowledge.errors.unknown'));
    }
  }, [handleImportPaths, t]);

  const drawerSource = useMemo(
    () => (drawer ? sources.find((s) => s.id === drawer.sourceId) ?? null : null),
    [drawer, sources],
  );

  const tabs: Array<{
    id: KnowledgeDetailTab;
    label: string;
    icon: React.ReactNode;
    badge?: number;
  }> = [
    { id: 'sources', label: t('knowledge.detail.tabs.sources'), icon: <FolderIcon className="h-3.5 w-3.5" /> },
    { id: 'search', label: t('knowledge.detail.tabs.search'), icon: <SearchIcon className="h-3.5 w-3.5" /> },
    {
      id: 'wiki',
      label: t('knowledge.detail.tabs.wiki'),
      icon: <NotebookIcon className="h-3.5 w-3.5" />,
      badge: kb?.pendingCategories.length || undefined,
    },
    { id: 'settings', label: t('knowledge.detail.tabs.settings'), icon: <SettingsIcon className="h-3.5 w-3.5" /> },
  ];

  return (
    <div
      className="mx-auto flex h-full w-full max-w-5xl min-h-0 flex-col px-4 py-4 sm:px-6 sm:py-5"
      data-testid="knowledge-detail"
    >
      {/* Top header */}
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-[var(--line)] pb-3">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <button
            type="button"
            onClick={onBack}
            className="inline-flex shrink-0 items-center gap-1 rounded-lg px-2 py-1.5 text-sm text-[var(--ink-muted)] transition-colors hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
          >
            <ArrowLeftIcon className="h-4 w-4" />
            <span>{t('knowledge.detail.back')}</span>
          </button>
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-lg font-semibold text-[var(--ink)]">
              {kb?.name ?? kbId}
            </h1>
            {kb?.description && (
              <p className="truncate text-xs text-[var(--ink-muted)]">{kb.description}</p>
            )}
          </div>
        </div>

        <button
          type="button"
          disabled={importing}
          onClick={() => void handlePickFiles()}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-3.5 py-1.5 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
        >
          <UploadIcon className="h-4 w-4" />
          {importing ? t('knowledge.detail.importing') : t('knowledge.detail.importFiles')}
        </button>
      </div>

      {/* In-page 4 tabs */}
      <div
        role="tablist"
        aria-label={t('knowledge.detail.tabsLabel')}
        className="mt-3 flex shrink-0 flex-wrap items-center gap-1"
      >
        {tabs.map((item) => {
          const selected = item.id === tab;
          return (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={selected}
              onClick={() => setTab(item.id)}
              className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                selected
                  ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
                  : 'text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]'
              }`}
            >
              {item.icon}
              <span>{item.label}</span>
              {item.badge !== undefined && (
                <span className="rounded-full bg-[var(--warning-bg)] px-1.5 text-xs text-[var(--warning)]">
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      <div className="mt-4 flex min-h-0 flex-1 flex-col overflow-hidden">
        {loadFailed ? (
          <div role="alert" className="rounded-xl border border-[var(--line)] p-6 text-center text-sm text-[var(--ink-muted)]">
            <p>{t('knowledge.loadFailed')}</p>
            <button
              type="button"
              onClick={() => void reload()}
              className="mt-3 rounded-lg bg-[var(--button-secondary-bg)] px-3 py-1.5 text-[var(--button-secondary-text)]"
            >
              {t('knowledge.retry')}
            </button>
          </div>
        ) : !kb ? (
          <p className="py-10 text-center text-sm text-[var(--ink-muted)]" role="status">
            {t('knowledge.loading')}
          </p>
        ) : tab === 'sources' ? (
          <KnowledgeSourcesTab
            kbId={kbId}
            isActive={isActive}
            sources={sources}
            approvedCategories={categories}
            loading={loading}
            onImportPaths={handleImportPaths}
            onPickFiles={handlePickFiles}
            onSelectSource={(source) =>
              setDrawer({ sourceId: source.id, initialTab: 'metadata' })
            }
            onMutated={reload}
          />
        ) : tab === 'search' ? (
          <KnowledgeSearchTab
            kbId={kbId}
            approvedCategories={categories}
            onInspectHit={(sourceId, chunkId) =>
              setDrawer({ sourceId, initialTab: 'chunks', highlightChunkId: chunkId })
            }
          />
        ) : tab === 'wiki' ? (
          <KnowledgeWikiTab kb={kb} onMutated={reload} />
        ) : (
          <KnowledgeSettingsTab kb={kb} onMutated={reload} onDeleted={onDeleted} />
        )}
      </div>

      {drawer && drawerSource && (
        <SourceDetailDrawer
          kbId={kbId}
          source={drawerSource}
          approvedCategories={categories}
          initialTab={drawer.initialTab}
          highlightChunkId={drawer.highlightChunkId}
          onClose={() => setDrawer(null)}
          onMutated={reload}
        />
      )}
    </div>
  );
}
