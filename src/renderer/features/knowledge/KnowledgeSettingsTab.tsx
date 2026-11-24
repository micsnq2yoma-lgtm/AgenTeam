import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import ConfirmDialog from '@/components/ConfirmDialog';
import CustomSelect, { type SelectOption } from '@/components/CustomSelect';
import { PlayIcon, SaveIcon, TrashIcon } from '@/components/icons';
import MarkdownEditor, { type MarkdownEditorHandle } from '@/components/markdown-editor/MarkdownEditor';
import { useToast } from '@/components/Toast';
import { useConfig } from '@/hooks/useConfig';

import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type {
  KnowledgeAgentWriteAccess,
  KnowledgeBaseRecord,
  KnowledgeCurationMode,
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
  const { config } = useConfig();
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

  const agents = useMemo(() => config.agents ?? [], [config.agents]);
  const agentOptions = useMemo<SelectOption[]>(
    () => [
      { value: '', label: t('knowledge.settings.curatorAgentNone') },
      ...agents.map((agent) => ({ value: agent.id, label: agent.name })),
    ],
    [agents, t],
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
