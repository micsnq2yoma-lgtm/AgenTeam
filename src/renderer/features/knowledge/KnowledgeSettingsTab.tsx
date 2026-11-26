import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';

import ConfirmDialog from '@/components/ConfirmDialog';
import CustomSelect, { type SelectOption } from '@/components/CustomSelect';
import {
  FolderPlusIcon,
  PlayIcon,
  RefreshIcon,
  SaveIcon,
  TrashIcon,
  WarningIcon,
} from '@/components/icons';
import MarkdownEditor, { type MarkdownEditorHandle } from '@/components/markdown-editor/MarkdownEditor';
import { useToast } from '@/components/Toast';
import { useConfig } from '@/hooks/useConfig';

import { curatorAgentOptions } from './curatorAgentOptions';
import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type {
  KnowledgeAgentWriteAccess,
  KnowledgeBaseRecord,
  KnowledgeCurationMode,
  KnowledgeLinkedFolder,
  LibraryEmbeddingSummary,
  RuleDocKind,
} from './types';

const INTERVAL_PRESETS = [1440, 4320, 10080, 20160] as const;

interface KnowledgeSettingsTabProps {
  kb: KnowledgeBaseRecord;
  onMutated: () => Promise<void>;
  onDeleted: () => void;
}

export default function KnowledgeSettingsTab({
  kb,
  onMutated,
  onDeleted,
}: KnowledgeSettingsTabProps) {
  const { t } = useTranslation('launcher');
  const { config, projects } = useConfig();
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const [name, setName] = useState(kb.name);
  const [description, setDescription] = useState(kb.description);
  const [curatorAgentId, setCuratorAgentId] = useState(kb.curatorAgentId ?? '');
  const [curationMode, setCurationMode] = useState<KnowledgeCurationMode>(kb.curationMode);
  const [intervalMinutes, setIntervalMinutes] = useState(kb.curationIntervalMinutes);
  const [agentWriteAccess, setAgentWriteAccess] = useState<KnowledgeAgentWriteAccess>(
    kb.agentWriteAccess,
  );
  const [wikiEnabled, setWikiEnabled] = useState(kb.wikiEnabled);
  const [savingSettings, setSavingSettings] = useState(false);
  const [curating, setCurating] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [folderBusyPath, setFolderBusyPath] = useState<string | null>(null);
  const [addingFolder, setAddingFolder] = useState(false);
  const [confirmRemoveFolder, setConfirmRemoveFolder] = useState<string | null>(null);
  const [embeddingSummary, setEmbeddingSummary] = useState<LibraryEmbeddingSummary | null>(null);
  const [reembedding, setReembedding] = useState(false);

  // Sync form state when Store pushes updated KB settings.
  const kbVersionKey = `${kb.id}:${kb.name}:${kb.description}:${kb.curatorAgentId ?? ''}:${kb.curationMode}:${kb.curationIntervalMinutes}:${kb.agentWriteAccess}:${String(kb.wikiEnabled)}`;
  const [syncedKey, setSyncedKey] = useState(kbVersionKey);
  if (syncedKey !== kbVersionKey) {
    setSyncedKey(kbVersionKey);
    setName(kb.name);
    setDescription(kb.description);
    setCuratorAgentId(kb.curatorAgentId ?? '');
    setCurationMode(kb.curationMode);
    setIntervalMinutes(kb.curationIntervalMinutes);
    setAgentWriteAccess(kb.agentWriteAccess);
    setWikiEnabled(kb.wikiEnabled);
  }

  // Rule doc editor state (reusing existing MarkdownEditor).
  const [activeDoc, setActiveDoc] = useState<RuleDocKind>('organize-rules');
  const [docSource, setDocSource] = useState<string | null>(null);
  const [docRevision, setDocRevision] = useState(0);
  const [docDirty, setDocDirty] = useState(false);
  const [savingDoc, setSavingDoc] = useState(false);
  const editorRef = useRef<MarkdownEditorHandle>(null);

  const mounted = useRef(false);
  const docSeq = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const loadRuleDoc = useCallback(async (doc: RuleDocKind) => {
    const seq = ++docSeq.current;
    setDocDirty(false);
    try {
      const content = await knowledgeClient.readRuleDoc(kb.id, doc);
      if (!mounted.current || docSeq.current !== seq) return;
      setDocSource(content ?? '');
      setDocRevision((rev) => rev + 1);
    } catch {
      if (mounted.current && docSeq.current === seq) {
        setDocSource('');
        setDocRevision((rev) => rev + 1);
      }
    }
  }, [kb.id]);

  useEffect(() => {
    void loadRuleDoc(activeDoc);
  }, [activeDoc, loadRuleDoc]);

  const loadEmbeddingSummary = useCallback(async () => {
    try {
      const summary = await knowledgeClient.embeddingSummary(kb.id);
      if (mounted.current) {
        setEmbeddingSummary(summary);
      }
    } catch {
      // Non-fatal in mock environments.
    }
  }, [kb.id]);

  useEffect(() => {
    void loadEmbeddingSummary();
  }, [loadEmbeddingSummary]);

  const handleRetryLibraryEmbedding = async () => {
    if (reembedding) return;
    setReembedding(true);
    try {
      await knowledgeClient.retryEmbedding(kb.id);
      await loadEmbeddingSummary();
    } finally {
      if (mounted.current) setReembedding(false);
    }
  };

  const agentOptions = useMemo<SelectOption[]>(
    () => [
      { value: '', label: t('knowledge.settings.curatorAgentNone') },
      ...curatorAgentOptions(projects, config.agents ?? [], curatorAgentId).map((agent) => ({
        value: agent.id,
        label: agent.removed ? t('knowledge.settings.curatorAgentRemoved', { name: agent.name }) : agent.name,
      })),
    ],
    [projects, config.agents, curatorAgentId, t],
  );
  const curationModeOptions = useMemo<SelectOption[]>(
    () => [
      ...(curatorAgentId ? [{ value: 'auto', label: t('knowledge.curation.auto') }] : []),
      { value: 'manual', label: t('knowledge.curation.manual') },
      { value: 'off', label: t('knowledge.curation.off') },
    ],
    [curatorAgentId, t],
  );
  const intervalOptions = useMemo<SelectOption[]>(() => {
    const base = INTERVAL_PRESETS.map((minutes) => ({
      value: String(minutes),
      label: t(`knowledge.settings.intervals.${minutes}`),
    }));
    if (!INTERVAL_PRESETS.includes(intervalMinutes as (typeof INTERVAL_PRESETS)[number])) {
      base.push({ value: String(intervalMinutes), label: `${intervalMinutes} min` });
    }
    return base;
  }, [intervalMinutes, t]);
  const writeAccessOptions = useMemo<SelectOption[]>(
    () => [
      { value: 'curator-only', label: t('knowledge.settings.writeAccess.curator-only') },
      { value: 'bound-sessions', label: t('knowledge.settings.writeAccess.bound-sessions') },
    ],
    [t],
  );

  const handleCuratorChange = (nextAgentId: string) => {
    setCuratorAgentId(nextAgentId);
    if (!nextAgentId && curationMode === 'auto') {
      setCurationMode('manual');
    }
  };

  const handleSaveSettings = async () => {
    const trimmedName = name.trim();
    if (!trimmedName || savingSettings) return;
    setSavingSettings(true);
    try {
      await knowledgeClient.update(kb.id, {
        name: trimmedName,
        description: description.trim(),
        curatorAgentId: curatorAgentId ? curatorAgentId : null,
        curationMode: !curatorAgentId && curationMode === 'auto' ? 'manual' : curationMode,
        curationIntervalMinutes: intervalMinutes,
        agentWriteAccess,
        wikiEnabled,
      });
      if (!mounted.current) return;
      toastRef.current.success(t('knowledge.settings.saved'));
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setSavingSettings(false);
    }
  };

  const handleCurateNow = async () => {
    if (curating || !kb.curatorAgentId) return;
    setCurating(true);
    try {
      await knowledgeClient.curateNow(kb.id);
      if (!mounted.current) return;
      toastRef.current.success(t('knowledge.settings.curateTriggered'));
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setCurating(false);
    }
  };

  const handleAddLinkedFolder = async () => {
    if (addingFolder) return;
    try {
      const selected = await openFileDialog({
        directory: true,
        multiple: false,
        title: t('knowledge.settings.linkedFoldersAddDialog'),
      });
      if (!selected || Array.isArray(selected)) return;
      setAddingFolder(true);
      const result = await knowledgeClient.addLinkedFolder(kb.id, selected);
      if (!mounted.current) return;
      toastRef.current.success(
        t('knowledge.settings.linkedFolderAdded', {
          added: result.summary.added,
          discovered: result.summary.discovered,
        }),
      );
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setAddingFolder(false);
    }
  };

  const handleUpdateLinkedFolder = async (
    folder: KnowledgeLinkedFolder,
    patch: { organizeAllowed?: boolean; gitConfirmed?: boolean },
  ) => {
    if (folderBusyPath) return;
    setFolderBusyPath(folder.path);
    try {
      await knowledgeClient.updateLinkedFolder(kb.id, folder.path, patch);
      if (!mounted.current) return;
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setFolderBusyPath(null);
    }
  };

  const handleScanLinkedFolder = async (folderPath: string) => {
    if (folderBusyPath) return;
    setFolderBusyPath(folderPath);
    try {
      const summary = await knowledgeClient.scanLinkedFolder(kb.id, folderPath);
      if (!mounted.current) return;
      toastRef.current.success(
        t('knowledge.settings.linkedFolderScanned', {
          added: summary.added,
          moved: summary.moved,
          updated: summary.updated,
        }),
      );
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setFolderBusyPath(null);
    }
  };

  const handleConfirmRemoveLinkedFolder = async () => {
    if (!confirmRemoveFolder || folderBusyPath) return;
    const targetPath = confirmRemoveFolder;
    setFolderBusyPath(targetPath);
    try {
      await knowledgeClient.removeLinkedFolder(kb.id, targetPath);
      if (!mounted.current) return;
      setConfirmRemoveFolder(null);
      toastRef.current.info(t('knowledge.settings.linkedFolderRemoved'));
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setFolderBusyPath(null);
    }
  };

  const handleSaveDoc = useCallback(async () => {
    if (savingDoc) return;
    const currentText = editorRef.current?.getSource() ?? docSource ?? '';
    setSavingDoc(true);
    try {
      await knowledgeClient.writeRuleDoc(kb.id, activeDoc, currentText);
      if (!mounted.current) return;
      setDocSource(currentText);
      setDocDirty(false);
      toastRef.current.success(
        t('knowledge.settings.docSaved', {
          doc:
            activeDoc === 'organize-rules'
              ? t('knowledge.settings.docOrganizeRules')
              : t('knowledge.settings.docWikiSchema'),
        }),
      );
      await onMutated();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setSavingDoc(false);
    }
  }, [activeDoc, docSource, kb.id, onMutated, savingDoc, t]);

  const handleConfirmDelete = async () => {
    if (deleting) return;
    setDeleting(true);
    try {
      await knowledgeClient.remove(kb.id);
      if (!mounted.current) return;
      setConfirmDelete(false);
      onDeleted();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setDeleting(false);
    }
  };

  const docFilename =
    activeDoc === 'organize-rules' ? 'ORGANIZE_RULES.md' : 'WIKI_SCHEMA.md';

  return (
    <div
      className="min-h-0 flex-1 space-y-5 overflow-y-auto pb-8"
      data-testid="knowledge-settings-tab"
    >
      {/* General & curation settings */}
      <form
        className="space-y-4 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5"
        onSubmit={(event) => {
          event.preventDefault();
          void handleSaveSettings();
        }}
      >
        <h2 className="text-base font-medium text-[var(--ink)]">
          {t('knowledge.settings.generalTitle')}
        </h2>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <label className="block text-xs text-[var(--ink-secondary)]">
            <span className="mb-1 block font-medium">{t('knowledge.nameLabel')}</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={80}
              className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-3 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
            />
          </label>

          <div className="block text-xs text-[var(--ink-secondary)]">
            <span className="mb-1 block font-medium">
              {t('knowledge.settings.curatorAgentLabel')}
            </span>
            <CustomSelect
              value={curatorAgentId}
              options={agentOptions}
              onChange={handleCuratorChange}
              ariaLabel={t('knowledge.settings.curatorAgentLabel')}
              size="sm"
              className="w-full"
            />
          </div>
        </div>

        <label className="block text-xs text-[var(--ink-secondary)]">
          <span className="mb-1 block font-medium">{t('knowledge.descriptionLabel')}</span>
          <textarea
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            className="w-full resize-none rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-3 py-2 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
          />
        </label>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <div className="block text-xs text-[var(--ink-secondary)]">
            <span className="mb-1 block font-medium">
              {t('knowledge.settings.curationModeLabel')}
            </span>
            <CustomSelect
              value={curationMode}
              options={curationModeOptions}
              onChange={(next) => setCurationMode(next as KnowledgeCurationMode)}
              ariaLabel={t('knowledge.settings.curationModeLabel')}
              size="sm"
              className="w-full"
            />
            {!curatorAgentId && (
              <span className="mt-1 block text-xs text-[var(--ink-muted)]">
                {t('knowledge.settings.curationAutoNeedsAgent')}
              </span>
            )}
          </div>

          <div className="block text-xs text-[var(--ink-secondary)]">
            <span className="mb-1 block font-medium">
              {t('knowledge.settings.curationIntervalLabel')}
            </span>
            <CustomSelect
              value={String(intervalMinutes)}
              options={intervalOptions}
              onChange={(next) => setIntervalMinutes(Number(next))}
              ariaLabel={t('knowledge.settings.curationIntervalLabel')}
              size="sm"
              className="w-full"
            />
          </div>

          <div className="block text-xs text-[var(--ink-secondary)]">
            <span className="mb-1 block font-medium">
              {t('knowledge.settings.writeAccessLabel')}
            </span>
            <CustomSelect
              value={agentWriteAccess}
              options={writeAccessOptions}
              onChange={(next) => setAgentWriteAccess(next as KnowledgeAgentWriteAccess)}
              ariaLabel={t('knowledge.settings.writeAccessLabel')}
              size="sm"
              className="w-full"
            />
          </div>
        </div>

        <label className="flex items-start gap-2.5 pt-1 text-sm text-[var(--ink)]">
          <input
            type="checkbox"
            checked={wikiEnabled}
            onChange={(e) => setWikiEnabled(e.target.checked)}
            className="mt-1 h-4 w-4 accent-[var(--accent)]"
          />
          <span>
            <span className="block font-medium">{t('knowledge.settings.wikiEnabledLabel')}</span>
            <span className="block text-xs text-[var(--ink-muted)]">
              {t('knowledge.settings.wikiEnabledHint')}
            </span>
          </span>
        </label>

        <div className="flex justify-end border-t border-[var(--line)] pt-3">
          <button
            type="submit"
            disabled={!name.trim() || savingSettings}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-4 py-1.5 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
          >
            <SaveIcon className="h-4 w-4" />
            {t('knowledge.settings.save')}
          </button>
        </div>
      </form>

      {/* Curate now */}
      <section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-medium text-[var(--ink)]">
            {t('knowledge.settings.curateNowTitle')}
          </h3>
          <p className="mt-0.5 text-xs text-[var(--ink-muted)]">
            {kb.curatorAgentId
              ? t('knowledge.settings.curateNowHint')
              : t('knowledge.settings.curateNeedsAgent')}
          </p>
        </div>
        <button
          type="button"
          disabled={!kb.curatorAgentId || curating}
          onClick={() => void handleCurateNow()}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-[var(--button-secondary-bg)] px-3.5 py-1.5 text-sm font-medium text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--button-secondary-bg-hover)] disabled:opacity-50"
        >
          <PlayIcon className="h-4 w-4" />
          {t('knowledge.settings.curateNow')}
        </button>
      </section>

      {/* Online embedding progress & token estimate (K21-b) */}
      {embeddingSummary && (
        <section
          data-testid="kb-embedding-summary"
          className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5"
        >
          <div className="min-w-0 flex-1 space-y-1">
            <h3 className="text-sm font-medium text-[var(--ink)]">
              {t('knowledge.embedding.libraryTitle')}
            </h3>
            <p className="text-xs text-[var(--ink-muted)]">
              {embeddingSummary.enabled
                ? t('knowledge.embedding.libraryProgress', {
                    embedded: embeddingSummary.embeddedChunks,
                    total: embeddingSummary.totalChunks,
                    tokens: embeddingSummary.estimatedTokens.toLocaleString(),
                  })
                : t('knowledge.embedding.disabledHint')}
            </p>
            {embeddingSummary.activeModelRevision && (
              <p className="font-mono text-xs text-[var(--ink-muted)]">
                {embeddingSummary.activeModelRevision}
              </p>
            )}
            {embeddingSummary.failedSources > 0 && (
              <p className="text-xs text-[var(--error)]">
                {t('knowledge.embedding.failedSourcesHint', {
                  count: embeddingSummary.failedSources,
                  reason: embeddingSummary.lastErrorKind ?? 'unknown',
                })}
              </p>
            )}
          </div>
          {embeddingSummary.enabled && (
            <button
              type="button"
              disabled={reembedding}
              onClick={() => void handleRetryLibraryEmbedding()}
              className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-[var(--button-secondary-bg)] px-3.5 py-1.5 text-sm font-medium text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--button-secondary-bg-hover)] disabled:opacity-50"
            >
              <RefreshIcon className="h-4 w-4" />
              {t('knowledge.embedding.reembedLibrary')}
            </button>
          )}
        </section>
      )}

      {/* Rule documents editor (reusing existing MarkdownEditor) */}
      <section className="space-y-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h3 className="text-sm font-medium text-[var(--ink)]">
              {t('knowledge.settings.rulesTitle')}
            </h3>
            <p className="mt-0.5 text-xs text-[var(--ink-muted)]">
              {t('knowledge.settings.rulesHint')}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {docDirty && (
              <span className="rounded bg-[var(--warning-bg)] px-2 py-0.5 text-xs text-[var(--warning)]">
                {t('knowledge.settings.unsavedBadge')}
              </span>
            )}
            <button
              type="button"
              disabled={savingDoc || docSource === null}
              onClick={() => void handleSaveDoc()}
              className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-3 py-1.5 text-xs font-medium text-[var(--button-primary-text)] hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
            >
              <SaveIcon className="h-3.5 w-3.5" />
              {t('knowledge.settings.saveDoc')}
            </button>
          </div>
        </div>

        <div className="flex items-center gap-1" role="tablist">
          {(['organize-rules', 'wiki-schema'] as const).map((doc) => (
            <button
              key={doc}
              type="button"
              role="tab"
              aria-selected={activeDoc === doc}
              onClick={() => setActiveDoc(doc)}
              className={`rounded-lg px-3 py-1 text-xs font-medium transition-colors ${
                activeDoc === doc
                  ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
                  : 'text-[var(--ink-muted)] hover:text-[var(--ink)]'
              }`}
            >
              {doc === 'organize-rules'
                ? t('knowledge.settings.docOrganizeRules')
                : t('knowledge.settings.docWikiSchema')}
            </button>
          ))}
        </div>

        <div className="h-72 overflow-hidden rounded-lg border border-[var(--line)] bg-[var(--paper)]">
          {docSource === null ? (
            <p className="py-10 text-center text-sm text-[var(--ink-muted)]" role="status">
              {t('knowledge.loading')}
            </p>
          ) : (
            <MarkdownEditor
              key={`${kb.id}:${activeDoc}:${docRevision}`}
              ref={editorRef}
              initialSource={docSource}
              sourceMode
              path={docFilename}
              allowImages={false}
              onChange={() => setDocDirty(true)}
              onSave={() => void handleSaveDoc()}
            />
          )}
        </div>
      </section>

      {/* Linked folders (K12 + K15 organize authorization) */}
      <section className="space-y-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 className="text-sm font-medium text-[var(--ink)]">
              {t('knowledge.settings.linkedFoldersTitle')}
            </h3>
            <p className="mt-0.5 text-xs text-[var(--ink-muted)]">
              {t('knowledge.settings.linkedFoldersHint')}
            </p>
          </div>
          <button
            type="button"
            disabled={addingFolder}
            onClick={() => void handleAddLinkedFolder()}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-[var(--button-secondary-bg)] px-3 py-1.5 text-xs font-medium text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--hover-bg)] disabled:opacity-50"
          >
            <FolderPlusIcon className="h-3.5 w-3.5" />
            {addingFolder
              ? t('knowledge.settings.linkedFoldersAdding')
              : t('knowledge.settings.linkedFoldersAdd')}
          </button>
        </div>

        {(kb.linkedFolders ?? []).length === 0 ? (
          <p className="rounded-lg border border-dashed border-[var(--line)] px-4 py-4 text-center text-xs text-[var(--ink-muted)]">
            {t('knowledge.settings.linkedFoldersEmpty')}
          </p>
        ) : (
          <ul className="space-y-2">
            {(kb.linkedFolders ?? []).map((folder) => {
              const busy = folderBusyPath === folder.path;
              return (
                <li
                  key={folder.path}
                  className="space-y-2 rounded-lg border border-[var(--line)] bg-[var(--paper)] p-3"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex min-w-0 flex-1 items-center gap-2">
                      <span className="truncate font-mono text-xs text-[var(--ink)]">
                        {folder.path}
                      </span>
                      {folder.isGitWorkTree && (
                        <span className="inline-flex shrink-0 items-center gap-1 rounded-full bg-[var(--warning-bg)] px-2 py-0.5 text-xs font-medium text-[var(--warning)]">
                          <WarningIcon className="h-3 w-3" />
                          {t('knowledge.settings.linkedFolderGitBadge')}
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => void handleScanLinkedFolder(folder.path)}
                        className="inline-flex items-center gap-1 rounded-md border border-[var(--line)] px-2 py-1 text-xs text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)] disabled:opacity-50"
                      >
                        <RefreshIcon className="h-3 w-3" />
                        {t('knowledge.settings.linkedFolderScan')}
                      </button>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => setConfirmRemoveFolder(folder.path)}
                        className="rounded-md border border-[var(--line)] px-2 py-1 text-xs text-[var(--error)] hover:bg-[var(--hover-bg)] disabled:opacity-50"
                      >
                        {t('knowledge.settings.linkedFolderRemove')}
                      </button>
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-4 text-xs text-[var(--ink-muted)]">
                    <label className="inline-flex items-center gap-1.5">
                      <input
                        type="checkbox"
                        checked={folder.organizeAllowed}
                        disabled={busy}
                        onChange={(event) =>
                          void handleUpdateLinkedFolder(folder, {
                            organizeAllowed: event.target.checked,
                          })
                        }
                        className="h-3.5 w-3.5 rounded border-[var(--line)]"
                      />
                      <span>{t('knowledge.settings.linkedFolderOrganizeAllowed')}</span>
                    </label>

                    {folder.isGitWorkTree && folder.organizeAllowed && (
                      <label className="inline-flex items-center gap-1.5 text-[var(--warning)]">
                        <input
                          type="checkbox"
                          checked={folder.gitConfirmed}
                          disabled={busy}
                          onChange={(event) =>
                            void handleUpdateLinkedFolder(folder, {
                              gitConfirmed: event.target.checked,
                            })
                          }
                          className="h-3.5 w-3.5 rounded border-[var(--line)]"
                        />
                        <span>{t('knowledge.settings.linkedFolderGitConfirm')}</span>
                      </label>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {/* Danger zone */}
      <section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--error-subtle)] bg-[var(--paper-elevated)] p-5">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-medium text-[var(--error)]">
            {t('knowledge.settings.dangerTitle')}
          </h3>
          <p className="mt-0.5 text-xs text-[var(--ink-muted)]">
            {t('knowledge.settings.dangerHint')}
          </p>
        </div>
        <button
          type="button"
          onClick={() => setConfirmDelete(true)}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-[var(--error)] px-3.5 py-1.5 text-sm font-medium text-[var(--on-error)] transition-colors hover:bg-[var(--error-hover)]"
        >
          <TrashIcon className="h-4 w-4" />
          {t('knowledge.delete')}
        </button>
      </section>

      {confirmRemoveFolder && (
        <ConfirmDialog
          title={t('knowledge.settings.linkedFolderRemoveTitle')}
          message={t('knowledge.settings.linkedFolderRemoveMessage', {
            path: confirmRemoveFolder,
          })}
          confirmText={t('knowledge.settings.linkedFolderRemove')}
          cancelText={t('knowledge.cancel')}
          confirmVariant="danger"
          loading={folderBusyPath === confirmRemoveFolder}
          onConfirm={() => void handleConfirmRemoveLinkedFolder()}
          onCancel={() => {
            if (!folderBusyPath) setConfirmRemoveFolder(null);
          }}
        />
      )}

      {confirmDelete && (
        <ConfirmDialog
          title={t('knowledge.deleteTitle', { name: kb.name })}
          message={t('knowledge.deleteMessage')}
          confirmText={t('knowledge.delete')}
          cancelText={t('knowledge.cancel')}
          confirmVariant="danger"
          loading={deleting}
          onConfirm={() => void handleConfirmDelete()}
          onCancel={() => {
            if (!deleting) setConfirmDelete(false);
          }}
        />
      )}
    </div>
  );
}
