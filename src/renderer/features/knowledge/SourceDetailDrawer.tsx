import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  CloseIcon,
  LayersIcon,
  LockIcon,
  LockOpenIcon,
  SaveIcon,
  TagIcon,
} from '@/components/icons';
import { useToast } from '@/components/Toast';

import { sourceDisplayTitle } from './folderTree';
import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type { KnowledgeChunk, KnowledgeMetadataKey, KnowledgeSourceView } from './types';

type DrawerTab = 'metadata' | 'chunks';

interface MetadataFormValues {
  category: string;
  summary: string;
  tags: string;
  docType: string;
  date: string;
  language: string;
  entities: string;
  duplicateOf: string;
}

function readStringValue(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function readListValue(value: unknown): string {
  if (!Array.isArray(value)) return '';
  return value.filter((item): item is string => typeof item === 'string').join(', ');
}

function formValuesFromSource(source: KnowledgeSourceView): MetadataFormValues {
  return {
    category: readStringValue(source.metadata.category?.value),
    summary: readStringValue(source.metadata.summary?.value),
    tags: readListValue(source.metadata.tags?.value),
    docType: readStringValue(source.metadata.docType?.value),
    date: readStringValue(source.metadata.date?.value),
    language: readStringValue(source.metadata.language?.value),
    entities: readListValue(source.metadata.entities?.value),
    duplicateOf: readStringValue(source.metadata.duplicateOf?.value),
  };
}

function parseListInput(raw: string): string[] {
  return raw
    .split(/[,，]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

interface SourceDetailDrawerProps {
  kbId: string;
  source: KnowledgeSourceView;
  approvedCategories: string[];
  initialTab?: DrawerTab;
  highlightChunkId?: string;
  onClose: () => void;
  onMutated: () => Promise<void>;
}

export default function SourceDetailDrawer({
  kbId,
  source,
  approvedCategories,
  initialTab = 'metadata',
  highlightChunkId,
  onClose,
  onMutated,
}: SourceDetailDrawerProps) {
  const { t } = useTranslation('launcher');
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const [tab, setTab] = useState<DrawerTab>(initialTab);
  const [form, setForm] = useState<MetadataFormValues>(() => formValuesFromSource(source));
  const [saving, setSaving] = useState(false);
  const [unlockingKey, setUnlockingKey] = useState<KnowledgeMetadataKey | null>(null);
  const [chunks, setChunks] = useState<KnowledgeChunk[] | null>(null);
  const [chunksError, setChunksError] = useState(false);

  const mounted = useRef(false);
  const chunkSeq = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // Re-sync draft form whenever the inspected source or its Store metadata changes.
  const initialValues = useMemo(() => formValuesFromSource(source), [source]);
  const sourceKey = `${source.id}:${JSON.stringify(source.metadata)}`;
  const [syncedKey, setSyncedKey] = useState(sourceKey);
  if (syncedKey !== sourceKey) {
    setSyncedKey(sourceKey);
    setForm(initialValues);
  }

  const loadChunks = useCallback(async () => {
    const seq = ++chunkSeq.current;
    setChunksError(false);
    try {
      const list = await knowledgeClient.listChunks(kbId, source.id);
      if (!mounted.current || chunkSeq.current !== seq) return;
      setChunks(list);
    } catch {
      if (mounted.current && chunkSeq.current === seq) {
        setChunksError(true);
        setChunks([]);
      }
    }
  }, [kbId, source.id]);

  useEffect(() => {
    if (tab === 'chunks') {
      void loadChunks();
    }
  }, [tab, loadChunks]);

  const dirtyKeys = useMemo(() => {
    const keys: KnowledgeMetadataKey[] = [];
    (Object.keys(initialValues) as KnowledgeMetadataKey[]).forEach((key) => {
      if (form[key] !== initialValues[key]) keys.push(key);
    });
    return keys;
  }, [form, initialValues]);

  const handleSave = async () => {
    if (saving || dirtyKeys.length === 0) return;
    setSaving(true);
    try {
      const patch: Partial<Record<KnowledgeMetadataKey, unknown>> = {};
      for (const key of dirtyKeys) {
        if (key === 'tags' || key === 'entities') {
          patch[key] = parseListInput(form[key]);
        } else {
          patch[key] = form[key].trim();
        }
      }
      await knowledgeClient.setMetadata(kbId, source.id, patch);
      if (!mounted.current) return;
      toastRef.current.success(t('knowledge.drawer.savedToast'));
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setSaving(false);
    }
  };

  const handleUnlock = async (key: KnowledgeMetadataKey) => {
    if (unlockingKey) return;
    setUnlockingKey(key);
    try {
      await knowledgeClient.unlockMetadata(kbId, source.id, key);
      if (!mounted.current) return;
      toastRef.current.success(t('knowledge.drawer.unlockedToast'));
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setUnlockingKey(null);
    }
  };

  const renderFieldLabel = (key: KnowledgeMetadataKey, htmlFor: string) => {
    const isLocked = source.metadata[key]?.origin === 'user';
    return (
      <div className="mb-1 flex items-center justify-between gap-2">
        <label htmlFor={htmlFor} className="text-xs font-medium text-[var(--ink-secondary)]">
          {t(`knowledge.drawer.fields.${key}`)}
        </label>
        {isLocked && (
          <span className="inline-flex items-center gap-1 text-xs text-[var(--accent)]" data-testid={`field-lock-${key}`}>
            <LockIcon className="h-3 w-3" />
            <span title={t('knowledge.drawer.lockedByUser')}>{t('knowledge.drawer.lockedByUser')}</span>
            <button
              type="button"
              disabled={unlockingKey === key}
              onClick={() => void handleUnlock(key)}
              aria-label={`${t('knowledge.drawer.unlock')} ${t(`knowledge.drawer.fields.${key}`)}`}
              className="ml-1 inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-xs text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)] disabled:opacity-50"
            >
              <LockOpenIcon className="h-3 w-3" />
              {t('knowledge.drawer.unlock')}
            </button>
          </span>
        )}
      </div>
    );
  };

  const title = sourceDisplayTitle(source);

  return (
    <div
      className="fixed inset-0 z-40 flex justify-end bg-black/30"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      data-testid="source-detail-drawer"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="flex h-full w-full max-w-xl flex-col border-l border-[var(--line)] bg-[var(--paper)] shadow-xl">
        <div className="flex shrink-0 items-center justify-between gap-3 border-b border-[var(--line)] px-5 py-4">
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold text-[var(--ink)]">{title}</h2>
            <p className="mt-0.5 truncate font-mono text-xs text-[var(--ink-muted)]">
              kb://{kbId}/{source.id}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('knowledge.drawer.close')}
            className="shrink-0 rounded-lg p-1.5 text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
          >
            <CloseIcon className="h-4 w-4" />
          </button>
        </div>

        <div className="flex shrink-0 items-center gap-1 border-b border-[var(--line)] px-5 py-2" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'metadata'}
            onClick={() => setTab('metadata')}
            className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
              tab === 'metadata'
                ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
                : 'text-[var(--ink-muted)] hover:text-[var(--ink)]'
            }`}
          >
            <TagIcon className="h-3.5 w-3.5" />
            {t('knowledge.drawer.metadataTab')}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'chunks'}
            onClick={() => setTab('chunks')}
            className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
              tab === 'chunks'
                ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
                : 'text-[var(--ink-muted)] hover:text-[var(--ink)]'
            }`}
          >
            <LayersIcon className="h-3.5 w-3.5" />
            {t('knowledge.drawer.chunksTab')}
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {tab === 'metadata' ? (
            <form
              className="space-y-4"
              onSubmit={(event) => {
                event.preventDefault();
                void handleSave();
              }}
            >
              <div>
                {renderFieldLabel('category', 'kb-meta-category')}
                <input
                  id="kb-meta-category"
                  list="kb-approved-categories"
                  value={form.category}
                  onChange={(e) => setForm((prev) => ({ ...prev, category: e.target.value }))}
                  placeholder={t('knowledge.drawer.categoryPlaceholder')}
                  className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                />
                <datalist id="kb-approved-categories">
                  {approvedCategories.map((cat) => (
                    <option key={cat} value={cat} />
                  ))}
                </datalist>
              </div>

              <div>
                {renderFieldLabel('summary', 'kb-meta-summary')}
                <textarea
                  id="kb-meta-summary"
                  rows={3}
                  value={form.summary}
                  onChange={(e) => setForm((prev) => ({ ...prev, summary: e.target.value }))}
                  placeholder={t('knowledge.drawer.summaryPlaceholder')}
                  className="w-full resize-y rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 py-2 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                />
              </div>

              <div>
                {renderFieldLabel('tags', 'kb-meta-tags')}
                <input
                  id="kb-meta-tags"
                  value={form.tags}
                  onChange={(e) => setForm((prev) => ({ ...prev, tags: e.target.value }))}
                  placeholder={t('knowledge.drawer.tagsPlaceholder')}
                  className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                />
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  {renderFieldLabel('docType', 'kb-meta-docType')}
                  <input
                    id="kb-meta-docType"
                    value={form.docType}
                    onChange={(e) => setForm((prev) => ({ ...prev, docType: e.target.value }))}
                    placeholder={t('knowledge.drawer.docTypePlaceholder')}
                    className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                  />
                </div>
                <div>
                  {renderFieldLabel('date', 'kb-meta-date')}
                  <input
                    id="kb-meta-date"
                    value={form.date}
                    onChange={(e) => setForm((prev) => ({ ...prev, date: e.target.value }))}
                    placeholder={t('knowledge.drawer.datePlaceholder')}
                    className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                  />
                </div>
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  {renderFieldLabel('language', 'kb-meta-language')}
                  <input
                    id="kb-meta-language"
                    value={form.language}
                    onChange={(e) => setForm((prev) => ({ ...prev, language: e.target.value }))}
                    placeholder={t('knowledge.drawer.languagePlaceholder')}
                    className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                  />
                </div>
                <div>
                  {renderFieldLabel('duplicateOf', 'kb-meta-duplicateOf')}
                  <input
                    id="kb-meta-duplicateOf"
                    value={form.duplicateOf}
                    onChange={(e) => setForm((prev) => ({ ...prev, duplicateOf: e.target.value }))}
                    placeholder={t('knowledge.drawer.duplicateOfPlaceholder')}
                    className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                  />
                </div>
              </div>

              <div>
                {renderFieldLabel('entities', 'kb-meta-entities')}
                <input
                  id="kb-meta-entities"
                  value={form.entities}
                  onChange={(e) => setForm((prev) => ({ ...prev, entities: e.target.value }))}
                  placeholder={t('knowledge.drawer.entitiesPlaceholder')}
                  className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper-elevated)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
                />
              </div>

              <div className="flex justify-end border-t border-[var(--line)] pt-4">
                <button
                  type="submit"
                  disabled={saving || dirtyKeys.length === 0}
                  className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-4 py-1.5 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
                >
                  <SaveIcon className="h-4 w-4" />
                  {saving ? t('knowledge.drawer.saving') : t('knowledge.drawer.save')}
                </button>
              </div>
            </form>
          ) : (
            <div className="space-y-3" data-testid="source-chunks-list">
              {chunks === null ? (
                <p className="py-8 text-center text-sm text-[var(--ink-muted)]" role="status">
                  {t('knowledge.drawer.chunksLoading')}
                </p>
              ) : chunksError ? (
                <div role="alert" className="rounded-xl border border-[var(--line)] p-4 text-center text-sm text-[var(--ink-muted)]">
                  <p>{t('knowledge.errors.unknown')}</p>
                  <button
                    type="button"
                    onClick={() => void loadChunks()}
                    className="mt-2 rounded-lg bg-[var(--button-secondary-bg)] px-3 py-1 text-xs text-[var(--button-secondary-text)]"
                  >
                    {t('knowledge.retry')}
                  </button>
                </div>
              ) : chunks.length === 0 ? (
                <p className="py-8 text-center text-sm text-[var(--ink-muted)]">
                  {t('knowledge.drawer.chunksEmpty')}
                </p>
              ) : (
                <>
                  <p className="text-xs text-[var(--ink-muted)]">
                    {t('knowledge.drawer.chunkCount', { count: chunks.length })}
                  </p>
                  {chunks.map((chunk) => {
                    const isHighlighted = highlightChunkId === chunk.chunkId;
                    return (
                      <article
                        key={chunk.chunkId}
                        data-testid="source-chunk-card"
                        className={`rounded-xl border p-3.5 ${
                          isHighlighted
                            ? 'border-[var(--accent)] bg-[var(--accent-warm-subtle)]'
                            : 'border-[var(--line)] bg-[var(--paper-elevated)]'
                        }`}
                      >
                        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-[var(--ink-muted)]">
                          <span className="font-medium text-[var(--ink-secondary)]">
                            {t('knowledge.drawer.chunkOrdinal', { ordinal: chunk.ordinal + 1 })}
                          </span>
                          <div className="flex items-center gap-2">
                            {chunk.page !== undefined && (
                              <span className="rounded bg-[var(--paper-inset)] px-1.5 py-0.5 text-[var(--ink-secondary)]">
                                {t('knowledge.drawer.chunkPage', { page: chunk.page })}
                              </span>
                            )}
                            <span className="font-mono text-[var(--ink-subtle)]">#{chunk.chunkId}</span>
                          </div>
                        </div>
                        {chunk.prefix && (
                          <p className="mt-1 text-xs text-[var(--accent)]">{chunk.prefix}</p>
                        )}
                        <pre className="mt-2 whitespace-pre-wrap break-words font-sans text-sm text-[var(--ink)]">
                          {chunk.text}
                        </pre>
                      </article>
                    );
                  })}
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
