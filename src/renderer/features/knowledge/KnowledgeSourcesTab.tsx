import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import ConfirmDialog from '@/components/ConfirmDialog';
import {
  AlertIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CloseIcon,
  FileCheckIcon,
  FileWarningIcon,
  FolderIcon,
  FolderOpenIcon,
  LoaderIcon,
  LockIcon,
  RefreshIcon,
  TrashIcon,
  UploadIcon,
} from '@/components/icons';
import { useToast } from '@/components/Toast';
import { useTauriFileDrop } from '@/hooks/useTauriFileDrop';

import {
  buildVirtualFolderTree,
  filterSourcesByFolder,
  sourceCategory,
  sourceDisplayTitle,
  sourceTags,
  type FolderSelection,
  type VirtualFolderNode,
} from './folderTree';
import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type { KnowledgeSourceStatus, KnowledgeSourceView } from './types';

const SOURCE_DRAG_MIME = 'application/x-agenteam-kb-source-id';

function isInProgressStatus(status: KnowledgeSourceStatus): boolean {
  return status === 'queued' || status === 'converting' || status === 'chunking';
}

interface KnowledgeSourcesTabProps {
  kbId: string;
  isActive: boolean;
  sources: KnowledgeSourceView[];
  approvedCategories: string[];
  loading: boolean;
  onImportPaths: (paths: string[]) => Promise<void>;
  onPickFiles: () => Promise<void>;
  onSelectSource: (source: KnowledgeSourceView) => void;
  onMutated: () => Promise<void>;
}

export default function KnowledgeSourcesTab({
  kbId,
  isActive,
  sources,
  approvedCategories,
  loading,
  onImportPaths,
  onPickFiles,
  onSelectSource,
  onMutated,
}: KnowledgeSourcesTabProps) {
  const { t } = useTranslation('launcher');
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const onImportPathsRef = useRef(onImportPaths);
  useLayoutEffect(() => {
    onImportPathsRef.current = onImportPaths;
  }, [onImportPaths]);

  const [selection, setSelection] = useState<FolderSelection>({ kind: 'all' });
  const [collapsedPaths, setCollapsedPaths] = useState<Record<string, boolean>>({});
  const [dragTargetFolder, setDragTargetFolder] = useState<string | null>(null);
  const [busySourceId, setBusySourceId] = useState<string | null>(null);
  const [pendingRemove, setPendingRemove] = useState<KnowledgeSourceView | null>(null);

  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // Native OS file drag-and-drop via Tauri (passes paths to Rust, never reads bytes in Renderer).
  const dropZoneRef = useRef<HTMLDivElement>(null);
  const { isDragging, activeZoneId, registerZone, unregisterZone } = useTauriFileDrop({
    enabled: isActive,
  });

  useEffect(() => {
    const el = dropZoneRef.current;
    if (!el) return;
    registerZone('kb-sources-drop', el, (paths) => {
      if (paths.length > 0) void onImportPathsRef.current(paths);
    });
    return () => unregisterZone('kb-sources-drop');
  }, [registerZone, unregisterZone]);

  // Show non-wiki sources in the Sources tab; wiki pages live in the Wiki tab.
  const librarySources = useMemo(
    () => sources.filter((source) => source.kind !== 'wiki'),
    [sources],
  );
  const activeIngestSources = useMemo(
    () => librarySources.filter((source) => isInProgressStatus(source.status)),
    [librarySources],
  );
  const tree = useMemo(
    () => buildVirtualFolderTree(approvedCategories, librarySources),
    [approvedCategories, librarySources],
  );
  const visibleSources = useMemo(
    () => filterSourcesByFolder(librarySources, selection),
    [librarySources, selection],
  );

  // Poll active ingest jobs lightly as a fallback alongside `knowledge:changed` events.
  const activeJobIdsKey = activeIngestSources
    .map((s) => s.jobId)
    .filter((id): id is string => Boolean(id))
    .join(',');
  useEffect(() => {
    if (!activeJobIdsKey) return;
    const jobIds = activeJobIdsKey.split(',');
    const timer = setInterval(() => {
      void (async () => {
        let changed = false;
        for (const jobId of jobIds) {
          try {
            const job = await knowledgeClient.jobStatus(jobId);
            if (!isInProgressStatus(job.status)) changed = true;
          } catch {
            changed = true;
          }
        }
        if (changed && mounted.current) {
          void onMutated();
        }
      })();
    }, 1500);
    return () => clearInterval(timer);
  }, [activeJobIdsKey, onMutated]);

  const handleAssignCategory = useCallback(async (sourceId: string, category: string) => {
    setBusySourceId(sourceId);
    try {
      await knowledgeClient.setMetadata(kbId, sourceId, { category });
      if (!mounted.current) return;
      toastRef.current.success(
        category
          ? t('knowledge.sources.categoryUpdated', { category })
          : t('knowledge.sources.categoryCleared'),
      );
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusySourceId(null);
    }
  }, [kbId, onMutated, t]);

  const handleCancelJob = async (source: KnowledgeSourceView) => {
    if (!source.jobId || busySourceId) return;
    setBusySourceId(source.id);
    try {
      await knowledgeClient.cancelJob(source.jobId);
      if (mounted.current) await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusySourceId(null);
    }
  };

  const handleRetrySource = async (source: KnowledgeSourceView) => {
    if (busySourceId) return;
    setBusySourceId(source.id);
    try {
      await knowledgeClient.retrySource(kbId, source.id);
      if (mounted.current) await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusySourceId(null);
    }
  };

  const handleConfirmRemove = async () => {
    const target = pendingRemove;
    if (!target || busySourceId) return;
    setBusySourceId(target.id);
    try {
      await knowledgeClient.removeSource(kbId, target.id);
      if (!mounted.current) return;
      setPendingRemove(null);
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusySourceId(null);
    }
  };

  const renderFolderNode = (node: VirtualFolderNode, depth: number) => {
    const hasChildren = node.children.length > 0;
    const isCollapsed = collapsedPaths[node.path] === true;
    const isSelected = selection.kind === 'category' && selection.path === node.path;
    const isDropTarget = dragTargetFolder === node.path;
    return (
      <li key={node.path}>
        <div
          data-testid={`folder-node-${node.path}`}
          onDragOver={(event) => {
            if (event.dataTransfer.types.includes(SOURCE_DRAG_MIME)) {
              event.preventDefault();
              event.dataTransfer.dropEffect = 'move';
              if (dragTargetFolder !== node.path) setDragTargetFolder(node.path);
            }
          }}
          onDragLeave={() => {
            if (dragTargetFolder === node.path) setDragTargetFolder(null);
          }}
          onDrop={(event) => {
            const draggedId = event.dataTransfer.getData(SOURCE_DRAG_MIME);
            setDragTargetFolder(null);
            if (!draggedId) return;
            event.preventDefault();
            void handleAssignCategory(draggedId, node.path);
          }}
          style={{ paddingLeft: `${depth * 14 + 8}px` }}
          className={`flex items-center gap-1 rounded-lg py-1.5 pr-2 text-sm transition-colors ${
            isDropTarget
              ? 'bg-[var(--accent-warm-muted)] text-[var(--ink)] ring-1 ring-[var(--accent)]'
              : isSelected
                ? 'bg-[var(--paper-inset)] font-medium text-[var(--ink)]'
                : 'text-[var(--ink-secondary)] hover:bg-[var(--hover-bg)]'
          }`}
        >
          {hasChildren ? (
            <button
              type="button"
              aria-label={node.name}
              onClick={() =>
                setCollapsedPaths((prev) => ({ ...prev, [node.path]: !prev[node.path] }))
              }
              className="rounded p-0.5 text-[var(--ink-muted)] hover:text-[var(--ink)]"
            >
              {isCollapsed ? (
                <ChevronRightIcon className="h-3.5 w-3.5" />
              ) : (
                <ChevronDownIcon className="h-3.5 w-3.5" />
              )}
            </button>
          ) : (
            <span className="w-4 shrink-0" />
          )}
          <button
            type="button"
            onClick={() => setSelection({ kind: 'category', path: node.path })}
            className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
          >
            {isSelected ? (
              <FolderOpenIcon className="h-3.5 w-3.5 shrink-0 text-[var(--accent)]" />
            ) : (
              <FolderIcon className="h-3.5 w-3.5 shrink-0 text-[var(--ink-muted)]" />
            )}
            <span className="truncate">{node.name}</span>
          </button>
          <span className="shrink-0 text-xs text-[var(--ink-muted)]">{node.count}</span>
        </div>
        {hasChildren && !isCollapsed && (
          <ul className="space-y-0.5">{node.children.map((child) => renderFolderNode(child, depth + 1))}</ul>
        )}
      </li>
    );
  };

  const showDropOverlay = isDragging && activeZoneId === 'kb-sources-drop';

  return (
    <div
      ref={dropZoneRef}
      className="relative flex min-h-0 flex-1 flex-col gap-4 overflow-hidden md:flex-row"
      data-testid="knowledge-sources-tab"
    >
      {showDropOverlay && (
        <div
          className="pointer-events-none absolute inset-0 z-20 flex flex-col items-center justify-center rounded-xl border-2 border-dashed border-[var(--accent)] bg-[var(--paper)]/90 text-center"
          data-testid="kb-drop-overlay"
        >
          <UploadIcon className="h-8 w-8 text-[var(--accent)]" />
          <p className="mt-2 text-sm font-medium text-[var(--ink)]">
            {t('knowledge.detail.dropHint')}
          </p>
          <p className="mt-1 text-xs text-[var(--ink-muted)]">
            {t('knowledge.detail.dropSubhint')}
          </p>
        </div>
      )}

      {/* Left: Virtual folder tree */}
      <aside className="flex shrink-0 flex-col rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-3 md:w-60 md:overflow-y-auto">
        <div className="mb-2 flex items-center justify-between px-2">
          <span className="text-xs font-medium text-[var(--ink-muted)]">
            {t('knowledge.sources.foldersTitle')}
          </span>
        </div>
        <ul className="space-y-0.5">
          <li>
            <button
              type="button"
              onClick={() => setSelection({ kind: 'all' })}
              className={`flex w-full items-center justify-between rounded-lg px-2 py-1.5 text-sm transition-colors ${
                selection.kind === 'all'
                  ? 'bg-[var(--paper-inset)] font-medium text-[var(--ink)]'
                  : 'text-[var(--ink-secondary)] hover:bg-[var(--hover-bg)]'
              }`}
            >
              <span>{t('knowledge.sources.allSources')}</span>
              <span className="text-xs text-[var(--ink-muted)]">{tree.totalCount}</span>
            </button>
          </li>
          <li>
            <button
              type="button"
              data-testid="folder-node-uncategorized"
              onDragOver={(event) => {
                if (event.dataTransfer.types.includes(SOURCE_DRAG_MIME)) {
                  event.preventDefault();
                  event.dataTransfer.dropEffect = 'move';
                  if (dragTargetFolder !== '') setDragTargetFolder('');
                }
              }}
              onDragLeave={() => {
                if (dragTargetFolder === '') setDragTargetFolder(null);
              }}
              onDrop={(event) => {
                const draggedId = event.dataTransfer.getData(SOURCE_DRAG_MIME);
                setDragTargetFolder(null);
                if (!draggedId) return;
                event.preventDefault();
                void handleAssignCategory(draggedId, '');
              }}
              onClick={() => setSelection({ kind: 'uncategorized' })}
              className={`flex w-full items-center justify-between rounded-lg px-2 py-1.5 text-sm transition-colors ${
                dragTargetFolder === ''
                  ? 'bg-[var(--accent-warm-muted)] text-[var(--ink)] ring-1 ring-[var(--accent)]'
                  : selection.kind === 'uncategorized'
                    ? 'bg-[var(--paper-inset)] font-medium text-[var(--ink)]'
                    : 'text-[var(--ink-secondary)] hover:bg-[var(--hover-bg)]'
              }`}
            >
              <span>{t('knowledge.sources.uncategorized')}</span>
              <span className="text-xs text-[var(--ink-muted)]">{tree.uncategorizedCount}</span>
            </button>
          </li>
          {tree.roots.map((node) => renderFolderNode(node, 0))}
        </ul>
        <p className="mt-3 border-t border-[var(--line)] px-2 pt-2 text-xs text-[var(--ink-subtle)]">
          {t('knowledge.sources.dragToCategoryHint')}
        </p>
      </aside>

      {/* Right: Source list */}
      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        {activeIngestSources.length > 0 && (
          <div
            role="status"
            data-testid="kb-ingest-progress"
            className="mb-3 flex shrink-0 items-center gap-2 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] px-3.5 py-2.5 text-xs text-[var(--ink-secondary)]"
          >
            <LoaderIcon className="h-4 w-4 shrink-0 animate-spin text-[var(--accent)]" />
            <span>
              {t('knowledge.sources.inProgressBanner', { count: activeIngestSources.length })}
            </span>
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto">
          {loading ? (
            <p className="py-10 text-center text-sm text-[var(--ink-muted)]" role="status">
              {t('knowledge.loading')}
            </p>
          ) : librarySources.length === 0 ? (
            <div
              className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-[var(--line-strong)] px-6 py-14 text-center"
              data-testid="knowledge-sources-empty"
            >
              <UploadIcon className="h-8 w-8 text-[var(--ink-muted)]" />
              <p className="text-sm font-medium text-[var(--ink)]">
                {t('knowledge.sources.emptyTitle')}
              </p>
              <p className="max-w-md text-xs text-[var(--ink-muted)]">
                {t('knowledge.sources.emptyHint')}
              </p>
              <button
                type="button"
                onClick={() => void onPickFiles()}
                className="mt-1 inline-flex items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-3.5 py-1.5 text-sm font-medium text-[var(--button-primary-text)] hover:bg-[var(--button-primary-bg-hover)]"
              >
                <UploadIcon className="h-4 w-4" />
                {t('knowledge.detail.importFiles')}
              </button>
            </div>
          ) : visibleSources.length === 0 ? (
            <p className="py-10 text-center text-sm text-[var(--ink-muted)]">
              {t('knowledge.sources.emptyFolder')}
            </p>
          ) : (
            <ul className="space-y-2">
              {visibleSources.map((source) => {
                const title = sourceDisplayTitle(source);
                const cat = sourceCategory(source);
                const tags = sourceTags(source);
                const duplicateOf = typeof source.metadata.duplicateOf?.value === 'string'
                  ? source.metadata.duplicateOf.value.trim()
                  : '';
                const inProgress = isInProgressStatus(source.status);
                const canRetry =
                  source.kind === 'file' &&
                  (source.status === 'failed' ||
                    source.status === 'cancelled' ||
                    source.status === 'interrupted');
                const categoryLocked = source.metadata.category?.origin === 'user';
                const isBusy = busySourceId === source.id;

                return (
                  <li
                    key={source.id}
                    draggable
                    onDragStart={(event) => {
                      event.dataTransfer.setData(SOURCE_DRAG_MIME, source.id);
                      event.dataTransfer.effectAllowed = 'move';
                    }}
                    data-testid="knowledge-source-row"
                    className="group flex items-start gap-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] px-4 py-3 transition-colors hover:border-[var(--line-strong)]"
                  >
                    <div className="mt-0.5 shrink-0">
                      {inProgress ? (
                        <LoaderIcon className="h-4 w-4 animate-spin text-[var(--accent)]" />
                      ) : source.status === 'failed' || source.status === 'interrupted' ? (
                        <FileWarningIcon className="h-4 w-4 text-[var(--error)]" />
                      ) : (
                        <FileCheckIcon className="h-4 w-4 text-[var(--success)]" />
                      )}
                    </div>

                    <div className="min-w-0 flex-1">
                      <button
                        type="button"
                        onClick={() => onSelectSource(source)}
                        className="block max-w-full truncate text-left text-sm font-medium text-[var(--ink)] hover:text-[var(--accent)]"
                      >
                        {title}
                      </button>

                      <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs">
                        <span
                          className={`rounded-md px-1.5 py-0.5 ${
                            source.status === 'failed'
                              ? 'bg-[var(--error-bg)] text-[var(--error)]'
                              : inProgress
                                ? 'bg-[var(--accent-warm-subtle)] text-[var(--accent)]'
                                : 'bg-[var(--paper-inset)] text-[var(--ink-secondary)]'
                          }`}
                        >
                          {t(`knowledge.sources.status.${source.status}`)}
                        </span>

                        {source.status === 'ready' && (
                          <span className="rounded-md bg-[var(--paper-inset)] px-1.5 py-0.5 text-[var(--ink-muted)]">
                            {t(`knowledge.sources.searchStatus.${source.searchStatus}`)}
                          </span>
                        )}

                        {source.curationPending && source.status === 'ready' && (
                          <span className="rounded-md bg-[var(--warning-bg)] px-1.5 py-0.5 text-[var(--warning)]">
                            {t('knowledge.sources.curationPending')}
                          </span>
                        )}

                        {cat && (
                          <span className="inline-flex items-center gap-1 rounded-md bg-[var(--paper-inset)] px-1.5 py-0.5 text-[var(--ink-secondary)]">
                            {categoryLocked && (
                              <LockIcon
                                className="h-3 w-3 text-[var(--accent)]"
                                aria-label={t('knowledge.drawer.lockedByUser')}
                              />
                            )}
                            {cat}
                          </span>
                        )}

                        {duplicateOf && (
                          <span
                            className="inline-flex items-center gap-1 rounded-md bg-[var(--warning-bg)] px-1.5 py-0.5 text-[var(--warning)]"
                            title={duplicateOf}
                          >
                            <AlertIcon className="h-3 w-3" />
                            {t('knowledge.sources.duplicateBadge')}
                          </span>
                        )}

                        {tags.slice(0, 5).map((tag) => (
                          <span
                            key={tag}
                            className="rounded-md border border-[var(--line)] px-1.5 py-0.5 text-[var(--ink-muted)]"
                          >
                            #{tag}
                          </span>
                        ))}
                      </div>

                      {source.ingestError && (
                        <p className="mt-1.5 text-xs text-[var(--error)]" role="alert">
                          {t(`knowledge.errors.${source.ingestError}`, {
                            defaultValue: source.ingestError,
                          })}
                          {source.ingestSuggestion && (
                            <span className="ml-1.5 text-[var(--ink-muted)]">
                              ·{' '}
                              {t(`knowledge.sources.suggestions.${source.ingestSuggestion}`, {
                                defaultValue: source.ingestSuggestion,
                              })}
                            </span>
                          )}
                        </p>
                      )}
                    </div>

                    <div className="flex shrink-0 items-center gap-1">
                      {inProgress && source.jobId && (
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => void handleCancelJob(source)}
                          className="inline-flex items-center gap-1 rounded-md bg-[var(--button-secondary-bg)] px-2 py-1 text-xs text-[var(--button-secondary-text)] hover:bg-[var(--button-secondary-bg-hover)] disabled:opacity-50"
                        >
                          <CloseIcon className="h-3 w-3" />
                          {t('knowledge.sources.cancelJob')}
                        </button>
                      )}
                      {canRetry && (
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => void handleRetrySource(source)}
                          className="inline-flex items-center gap-1 rounded-md bg-[var(--button-secondary-bg)] px-2 py-1 text-xs text-[var(--button-secondary-text)] hover:bg-[var(--button-secondary-bg-hover)] disabled:opacity-50"
                        >
                          <RefreshIcon className="h-3 w-3" />
                          {t('knowledge.sources.retryJob')}
                        </button>
                      )}
                      <button
                        type="button"
                        disabled={isBusy}
                        aria-label={t('knowledge.sources.remove')}
                        title={t('knowledge.sources.remove')}
                        onClick={() => setPendingRemove(source)}
                        className="rounded-md p-1.5 text-[var(--ink-muted)] opacity-60 transition hover:bg-[var(--paper-inset)] hover:text-[var(--error)] focus-visible:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
                      >
                        <TrashIcon className="h-4 w-4" />
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </section>

      {pendingRemove && (
        <ConfirmDialog
          title={t('knowledge.sources.removeTitle', { name: sourceDisplayTitle(pendingRemove) })}
          message={t('knowledge.sources.removeMessage')}
          confirmText={t('knowledge.delete')}
          cancelText={t('knowledge.cancel')}
          confirmVariant="danger"
          loading={busySourceId === pendingRemove.id}
          onConfirm={() => void handleConfirmRemove()}
          onCancel={() => {
            if (!busySourceId) setPendingRemove(null);
          }}
        />
      )}
    </div>
  );
}
