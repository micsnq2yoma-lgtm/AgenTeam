import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import ConfirmDialog from '@/components/ConfirmDialog';
import { BookIcon, PlusIcon, TrashIcon } from '@/components/icons';
import { useToast } from '@/components/Toast';

import KnowledgeDetail from './KnowledgeDetail';
import { formatStorageBytes, knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type { KnowledgeBaseRecord, KnowledgeBaseStats } from './types';
import { useKnowledgeChangeSubscription } from './useKnowledgeEvents';

interface Row {
  base: KnowledgeBaseRecord;
  /** undefined while counting; null when the count could not be read. */
  sources: number | null | undefined;
  stats?: KnowledgeBaseStats | null;
}

interface KnowledgeHomeProps {
  isActive?: boolean;
}

/**
 * Library list & in-page detail host (K9). The Rust Store is the only authority:
 * every mutation or `knowledge:changed` event refetches instead of patching a local copy.
 */
export default function KnowledgeHome({ isActive = true }: KnowledgeHomeProps) {
  const { t, i18n } = useTranslation('launcher');
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const [selectedKbId, setSelectedKbId] = useState<string | null>(null);
  const [rows, setRows] = useState<Row[] | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [busy, setBusy] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<KnowledgeBaseRecord | null>(null);

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
      const bases = await knowledgeClient.list();
      if (!mounted.current || loadSeq.current !== seq) return;
      setLoadFailed(false);
      setRows(bases.map((base) => ({ base, sources: undefined, stats: undefined })));
      await Promise.all(
        bases.map(async (base) => {
          let stats: KnowledgeBaseStats | null = null;
          let count: number | null = null;
          try {
            stats = await knowledgeClient.stats(base.id);
            count = stats.sourceCount;
          } catch {
            try {
              count = await knowledgeClient.sourceCount(base.id);
            } catch {
              /* shown as unknown */
            }
          }
          if (!mounted.current || loadSeq.current !== seq) return;
          setRows(
            (current) =>
              current?.map((row) =>
                row.base.id === base.id ? { ...row, sources: count, stats } : row,
              ) ?? current,
          );
        }),
      );
    } catch {
      if (mounted.current && loadSeq.current === seq) setLoadFailed(true);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useKnowledgeChangeSubscription(() => {
    void reload();
  });

  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed || busy) return;
    setBusy(true);
    try {
      await knowledgeClient.create({
        name: trimmed,
        description: description.trim() || undefined,
        locale: i18n.language,
      });
      if (!mounted.current) return;
      setCreating(false);
      setName('');
      setDescription('');
      await reload();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  const confirmDelete = async () => {
    const target = pendingDelete;
    if (!target || busy) return;
    setBusy(true);
    try {
      await knowledgeClient.remove(target.id);
      if (!mounted.current) return;
      setPendingDelete(null);
      await reload();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  if (selectedKbId) {
    const initialBase = rows?.find((r) => r.base.id === selectedKbId)?.base;
    return (
      <KnowledgeDetail
        kbId={selectedKbId}
        initialBase={initialBase}
        isActive={isActive}
        onBack={() => {
          setSelectedKbId(null);
          void reload();
        }}
        onDeleted={() => {
          setSelectedKbId(null);
          void reload();
        }}
      />
    );
  }

  const modeLabel = (mode: KnowledgeBaseRecord['curationMode']) => t(`knowledge.curation.${mode}`);

  return (
    <div
      className="mx-auto flex h-full w-full max-w-3xl min-h-0 flex-col px-4 py-5 sm:px-6 sm:py-6"
      data-testid="knowledge-home"
    >
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          <h1 className="text-lg font-semibold text-[var(--ink)]">{t('knowledge.title')}</h1>
          <p className="mt-0.5 text-xs text-[var(--ink-muted)]">{t('knowledge.subtitle')}</p>
        </div>
        {!creating && (
          <button
            type="button"
            onClick={() => setCreating(true)}
            className="flex shrink-0 items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-3 py-1.5 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)]"
          >
            <PlusIcon className="h-4 w-4" />
            {t('knowledge.create')}
          </button>
        )}
      </div>

      {creating && (
        <form
          className="mt-4 shrink-0 space-y-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4"
          onSubmit={(event) => {
            event.preventDefault();
            void create();
          }}
        >
          <input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t('knowledge.namePlaceholder')}
            aria-label={t('knowledge.nameLabel')}
            maxLength={80}
            className="h-10 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
          />
          <textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder={t('knowledge.descriptionPlaceholder')}
            aria-label={t('knowledge.descriptionLabel')}
            rows={2}
            className="w-full resize-none rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-3 py-2 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => {
                setCreating(false);
                setName('');
                setDescription('');
              }}
              className="rounded-lg px-3 py-1.5 text-sm text-[var(--ink-muted)] hover:bg-[var(--hover-bg)]"
            >
              {t('knowledge.cancel')}
            </button>
            <button
              type="submit"
              disabled={!name.trim() || busy}
              className="rounded-lg bg-[var(--button-primary-bg)] px-3 py-1.5 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
            >
              {t('knowledge.createConfirm')}
            </button>
          </div>
        </form>
      )}

      <div className="mt-4 min-h-0 flex-1 overflow-y-auto">
        {loadFailed ? (
          <div
            role="alert"
            className="rounded-xl border border-[var(--line)] p-6 text-center text-sm text-[var(--ink-muted)]"
          >
            <p>{t('knowledge.loadFailed')}</p>
            <button
              type="button"
              onClick={() => void reload()}
              className="mt-3 rounded-lg bg-[var(--button-secondary-bg)] px-3 py-1.5 text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--button-secondary-bg-hover)]"
            >
              {t('knowledge.retry')}
            </button>
          </div>
        ) : rows === null ? (
          <p className="py-10 text-center text-sm text-[var(--ink-muted)]" role="status">
            {t('knowledge.loading')}
          </p>
        ) : rows.length === 0 ? (
          <div
            className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-[var(--line-strong)] px-6 py-14 text-center"
            data-testid="knowledge-empty"
          >
            <BookIcon className="h-8 w-8 text-[var(--ink-muted)]" />
            <p className="text-sm font-medium text-[var(--ink)]">{t('knowledge.emptyTitle')}</p>
            <p className="max-w-sm text-xs text-[var(--ink-muted)]">{t('knowledge.emptyHint')}</p>
          </div>
        ) : (
          <ul className="space-y-2">
            {rows.map(({ base, sources, stats }) => (
              <li
                key={base.id}
                data-testid="knowledge-row"
                className="group flex items-start gap-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] px-4 py-3 transition-colors hover:border-[var(--line-strong)]"
              >
                <BookIcon className="mt-0.5 h-4 w-4 shrink-0 text-[var(--accent)]" />
                <button
                  type="button"
                  aria-label={t('knowledge.openLibrary', { name: base.name })}
                  onClick={() => setSelectedKbId(base.id)}
                  className="min-w-0 flex-1 text-left"
                >
                  <div className="truncate text-sm font-medium text-[var(--ink)] group-hover:text-[var(--accent)]">
                    {base.name}
                  </div>
                  {base.description && (
                    <div className="mt-0.5 line-clamp-2 text-xs text-[var(--ink-muted)]">
                      {base.description}
                    </div>
                  )}
                  <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-[var(--ink-muted)]">
                    <span>
                      {sources === undefined
                        ? t('knowledge.counting')
                        : sources === null
                          ? t('knowledge.countUnknown')
                          : t('knowledge.sourceCount', { count: sources })}
                    </span>
                    {stats && (
                      <>
                        <span>
                          {t('knowledge.searchModeLabel', {
                            mode: t(`knowledge.searchModes.${stats.searchMode}`, {
                              defaultValue: stats.searchMode.toUpperCase(),
                            }),
                          })}
                        </span>
                        <span>
                          {t('knowledge.storageLabel', {
                            size: formatStorageBytes(stats.storageBytes),
                          })}
                        </span>
                      </>
                    )}
                    <span>{t('knowledge.curationLabel', { mode: modeLabel(base.curationMode) })}</span>
                    {stats && stats.pendingCurationCount > 0 && (
                      <span>{t('knowledge.pendingCuration', { count: stats.pendingCurationCount })}</span>
                    )}
                    {base.pendingCategories.length > 0 && (
                      <span className="text-[var(--warning)]">
                        {t('knowledge.pendingCategories', { count: base.pendingCategories.length })}
                      </span>
                    )}
                  </div>
                </button>
                <button
                  type="button"
                  aria-label={t('knowledge.deleteAria', { name: base.name })}
                  title={t('knowledge.delete')}
                  onClick={() => setPendingDelete(base)}
                  className="shrink-0 rounded-md p-1.5 text-[var(--ink-muted)] opacity-60 transition hover:bg-[var(--paper-inset)] hover:text-[var(--error)] focus-visible:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
                >
                  <TrashIcon className="h-4 w-4" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {pendingDelete && (
        <ConfirmDialog
          title={t('knowledge.deleteTitle', { name: pendingDelete.name })}
          message={t('knowledge.deleteMessage')}
          confirmText={t('knowledge.delete')}
          cancelText={t('knowledge.cancel')}
          confirmVariant="danger"
          loading={busy}
          onConfirm={() => void confirmDelete()}
          onCancel={() => {
            if (!busy) setPendingDelete(null);
          }}
        />
      )}
    </div>
  );
}
