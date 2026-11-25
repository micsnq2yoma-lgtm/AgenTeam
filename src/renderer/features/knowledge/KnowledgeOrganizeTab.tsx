import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import ConfirmDialog from '@/components/ConfirmDialog';
import {
  ArrowRightIcon,
  CheckIcon,
  FolderIcon,
  PlayIcon,
  RefreshIcon,
  UndoIcon,
  WarningIcon,
} from '@/components/icons';
import { useToast } from '@/components/Toast';

import { knowledgeClient, knowledgeErrorCode } from './knowledgeClient';
import type {
  ItemApprovalDecision,
  KnowledgeBaseRecord,
  KnowledgeLinkedFolder,
  OrganizeItemStatus,
  OrganizePlan,
  OrganizePlanItem,
  OrganizePlanStatus,
  UndoReport,
} from './types';

interface KnowledgeOrganizeTabProps {
  kb: KnowledgeBaseRecord;
  plans: OrganizePlan[];
  loading: boolean;
  onReload: () => Promise<void>;
}

/** Pure helper: items that affect relative Markdown links are NEVER selected by default (AC-K15). */
export function isOrganizeItemDefaultSelected(item: OrganizePlanItem): boolean {
  return item.defaultSelected && item.affectedMarkdownLinks === 0;
}

function normalizePathKey(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

function isGitRoot(folderRoot: string, linkedFolders?: KnowledgeLinkedFolder[]): boolean {
  if (!linkedFolders?.length) return false;
  const key = normalizePathKey(folderRoot);
  return linkedFolders.some((f) => f.isGitWorkTree && normalizePathKey(f.path) === key);
}

function planStatusTone(status: OrganizePlanStatus): string {
  switch (status) {
    case 'pending_review':
      return 'bg-[var(--warning-bg)] text-[var(--warning)]';
    case 'approved':
    case 'executing':
      return 'bg-[var(--info-bg,var(--paper-inset))] text-[var(--ink)]';
    case 'completed':
      return 'bg-[var(--success-bg,var(--paper-inset))] text-[var(--success,var(--ink))]';
    case 'partially_completed':
    case 'interrupted':
      return 'bg-[var(--error-subtle,var(--warning-bg))] text-[var(--error)]';
    case 'rejected':
    case 'stale':
      return 'bg-[var(--paper-inset)] text-[var(--ink-muted)]';
  }
}

function itemStatusTone(status: OrganizeItemStatus): string {
  switch (status) {
    case 'succeeded':
      return 'bg-[var(--success-bg,var(--paper-inset))] text-[var(--success,var(--ink))]';
    case 'conflict':
    case 'undo_skipped_modified':
    case 'undo_conflict':
      return 'bg-[var(--warning-bg)] text-[var(--warning)]';
    case 'failed':
    case 'undo_failed':
      return 'bg-[var(--error-subtle,var(--warning-bg))] text-[var(--error)]';
    case 'undone':
    case 'skipped':
    case 'pending':
      return 'bg-[var(--paper-inset)] text-[var(--ink-muted)]';
  }
}

function buildInitialDraft(plan: OrganizePlan | null): {
  selected: Record<string, boolean>;
  overrides: Record<string, string>;
} {
  if (!plan) return { selected: {}, overrides: {} };
  const selected: Record<string, boolean> = {};
  const overrides: Record<string, string> = {};
  for (const item of plan.items) {
    selected[item.id] =
      plan.status === 'pending_review' ? isOrganizeItemDefaultSelected(item) : item.approved;
    if (item.overrideTargetRel) {
      overrides[item.id] = item.overrideTargetRel;
    }
  }
  return { selected, overrides };
}

export default function KnowledgeOrganizeTab({
  kb,
  plans,
  loading,
  onReload,
}: KnowledgeOrganizeTabProps) {
  const { t } = useTranslation('launcher');
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const [selectedPlanId, setSelectedPlanId] = useState<string | null>(plans[0]?.id ?? null);
  const activePlan = useMemo(
    () => plans.find((p) => p.id === selectedPlanId) ?? plans[0] ?? null,
    [plans, selectedPlanId],
  );

  // Sync item selection & target overrides whenever the active plan identity or revision changes.
  const activePlanKey = activePlan ? `${activePlan.id}:${activePlan.updatedAt}:${activePlan.status}` : '';
  const [syncedPlanKey, setSyncedPlanKey] = useState(activePlanKey);
  const [selectedItems, setSelectedItems] = useState<Record<string, boolean>>(
    () => buildInitialDraft(activePlan).selected,
  );
  const [targetOverrides, setTargetOverrides] = useState<Record<string, string>>(
    () => buildInitialDraft(activePlan).overrides,
  );
  if (syncedPlanKey !== activePlanKey) {
    setSyncedPlanKey(activePlanKey);
    const next = buildInitialDraft(activePlan);
    setSelectedItems(next.selected);
    setTargetOverrides(next.overrides);
  }

  const [busyAction, setBusyAction] = useState<'execute' | 'reject' | 'undo' | null>(null);
  const [confirmReject, setConfirmReject] = useState(false);
  const [confirmUndo, setConfirmUndo] = useState(false);
  const [lastUndoReport, setLastUndoReport] = useState<UndoReport | null>(null);

  const isReviewable = activePlan?.status === 'pending_review';

  const selectedCount = useMemo(() => {
    if (!activePlan) return 0;
    return activePlan.items.filter((item) => Boolean(selectedItems[item.id])).length;
  }, [activePlan, selectedItems]);

  const executionStats = useMemo(() => {
    if (!activePlan) return null;
    const stats = {
      succeeded: 0,
      conflict: 0,
      failed: 0,
      skipped: 0,
      undone: 0,
      undoSkippedModified: 0,
    };
    for (const item of activePlan.items) {
      if (item.status === 'succeeded') stats.succeeded += 1;
      else if (item.status === 'conflict') stats.conflict += 1;
      else if (item.status === 'failed') stats.failed += 1;
      else if (item.status === 'skipped') stats.skipped += 1;
      else if (item.status === 'undone') stats.undone += 1;
      else if (item.status === 'undo_skipped_modified') stats.undoSkippedModified += 1;
    }
    return stats;
  }, [activePlan]);

  const canUndo = Boolean(
    activePlan &&
      (activePlan.status === 'completed' ||
        activePlan.status === 'partially_completed' ||
        activePlan.status === 'interrupted') &&
      activePlan.items.some((item) => item.status === 'succeeded'),
  );

  // Group effective operations by folderRoot for the before/after directory tree diff preview.
  const diffGroups = useMemo(() => {
    if (!activePlan) return [];
    const groups = new Map<
      string,
      Array<{ id: string; kind: OrganizePlanItem['kind']; before: string | null; after: string; active: boolean }>
    >();
    for (const item of activePlan.items) {
      const active = isReviewable ? Boolean(selectedItems[item.id]) : item.approved;
      const override = targetOverrides[item.id]?.trim();
      const effectiveTarget = override || item.overrideTargetRel || item.targetRel;
      const list = groups.get(item.folderRoot) ?? [];
      list.push({
        id: item.id,
        kind: item.kind,
        before: item.sourceRel ?? null,
        after: effectiveTarget,
        active,
      });
      groups.set(item.folderRoot, list);
    }
    return Array.from(groups.entries());
  }, [activePlan, isReviewable, selectedItems, targetOverrides]);

  const handleToggleItem = useCallback((itemId: string) => {
    setSelectedItems((prev) => ({ ...prev, [itemId]: !prev[itemId] }));
  }, []);

  const handleSelectAll = useCallback(
    (mode: 'safe' | 'all' | 'none') => {
      if (!activePlan) return;
      const next: Record<string, boolean> = {};
      for (const item of activePlan.items) {
        if (mode === 'all') next[item.id] = true;
        else if (mode === 'none') next[item.id] = false;
        else next[item.id] = isOrganizeItemDefaultSelected(item);
      }
      setSelectedItems(next);
    },
    [activePlan],
  );

  const handleOverrideChange = useCallback((itemId: string, value: string) => {
    setTargetOverrides((prev) => ({ ...prev, [itemId]: value }));
  }, []);

  const handleApproveAndExecute = useCallback(async () => {
    if (!activePlan || busyAction) return;
    const items: ItemApprovalDecision[] = activePlan.items.map((item) => {
      const approved = Boolean(selectedItems[item.id]);
      const rawOverride = targetOverrides[item.id]?.trim();
      const overrideTargetRel =
        rawOverride && rawOverride !== item.targetRel ? rawOverride : undefined;
      return {
        itemId: item.id,
        approved,
        ...(overrideTargetRel ? { overrideTargetRel } : {}),
      };
    });
    if (!items.some((entry) => entry.approved)) return;

    setBusyAction('execute');
    setLastUndoReport(null);
    try {
      await knowledgeClient.approveOrganizePlan(kb.id, activePlan.id, {
        reject: false,
        items,
      });
      const executed = await knowledgeClient.executeOrganizePlan(kb.id, activePlan.id);
      if (!mounted.current) return;
      if (executed.status === 'completed') {
        toastRef.current.success(t('knowledge.organize.executeCompleted'));
      } else if (executed.status === 'partially_completed') {
        toastRef.current.info(t('knowledge.organize.executePartiallyCompleted'));
      } else {
        toastRef.current.error(t('knowledge.organize.executeInterrupted'));
      }
      await onReload();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusyAction(null);
    }
  }, [activePlan, busyAction, kb.id, onReload, selectedItems, t, targetOverrides]);

  const handleExecuteApproved = useCallback(async () => {
    if (!activePlan || busyAction) return;
    setBusyAction('execute');
    setLastUndoReport(null);
    try {
      const executed = await knowledgeClient.executeOrganizePlan(kb.id, activePlan.id);
      if (!mounted.current) return;
      if (executed.status === 'completed') {
        toastRef.current.success(t('knowledge.organize.executeCompleted'));
      } else {
        toastRef.current.info(t('knowledge.organize.executePartiallyCompleted'));
      }
      await onReload();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusyAction(null);
    }
  }, [activePlan, busyAction, kb.id, onReload, t]);

  const handleConfirmReject = useCallback(async () => {
    if (!activePlan || busyAction) return;
    setBusyAction('reject');
    try {
      await knowledgeClient.approveOrganizePlan(kb.id, activePlan.id, { reject: true });
      if (!mounted.current) return;
      setConfirmReject(false);
      toastRef.current.info(t('knowledge.organize.planRejected'));
      await onReload();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusyAction(null);
    }
  }, [activePlan, busyAction, kb.id, onReload, t]);

  const handleConfirmUndo = useCallback(async () => {
    if (!activePlan || busyAction) return;
    setBusyAction('undo');
    try {
      const report = await knowledgeClient.undoOrganizePlan(kb.id, activePlan.id);
      if (!mounted.current) return;
      setConfirmUndo(false);
      setLastUndoReport(report);
      toastRef.current.success(
        t('knowledge.organize.undoDone', {
          undone: report.undone.length,
          skipped: report.skippedModified.length,
        }),
      );
      await onReload();
    } catch (error) {
      toastRef.current.error(
        t(`knowledge.errors.${knowledgeErrorCode(error)}`, {
          defaultValue: t('knowledge.errors.unknown'),
        }),
      );
    } finally {
      if (mounted.current) setBusyAction(null);
    }
  }, [activePlan, busyAction, kb.id, onReload, t]);

  if (loading && plans.length === 0) {
    return (
      <p className="py-10 text-center text-sm text-[var(--ink-muted)]" role="status">
        {t('knowledge.loading')}
      </p>
    );
  }

  if (plans.length === 0) {
    return (
      <div
        className="flex flex-col items-center justify-center rounded-xl border border-dashed border-[var(--line)] bg-[var(--paper-elevated)] px-6 py-12 text-center"
        data-testid="knowledge-organize-empty"
      >
        <FolderIcon className="h-8 w-8 text-[var(--ink-subtle)]" />
        <p className="mt-3 text-sm font-medium text-[var(--ink)]">
          {t('knowledge.organize.emptyTitle')}
        </p>
        <p className="mt-1 max-w-md text-xs text-[var(--ink-muted)]">
          {t('knowledge.organize.emptyHint')}
        </p>
      </div>
    );
  }

  return (
    <div
      className="grid min-h-0 flex-1 grid-cols-1 gap-4 overflow-hidden lg:grid-cols-[240px_1fr]"
      data-testid="knowledge-organize-tab"
    >
      {/* Left rail: history & pending plans */}
      <aside className="flex min-h-0 flex-col rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-3">
        <div className="mb-2 flex items-center justify-between gap-2">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-[var(--ink-muted)]">
            {t('knowledge.organize.historyTitle')}
          </h2>
          <button
            type="button"
            onClick={() => void onReload()}
            aria-label={t('knowledge.organize.refresh')}
            className="rounded-md p-1 text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
          >
            <RefreshIcon className="h-3.5 w-3.5" />
          </button>
        </div>

        <ul className="flex-1 space-y-1.5 overflow-y-auto">
          {plans.map((plan) => {
            const selected = activePlan?.id === plan.id;
            return (
              <li key={plan.id}>
                <button
                  type="button"
                  onClick={() => {
                    setSelectedPlanId(plan.id);
                    setLastUndoReport(null);
                  }}
                  aria-current={selected ? 'true' : undefined}
                  className={`w-full rounded-lg border px-2.5 py-2 text-left transition-colors ${
                    selected
                      ? 'border-[var(--accent)] bg-[var(--paper-inset)]'
                      : 'border-transparent hover:bg-[var(--hover-bg)]'
                  }`}
                >
                  <div className="flex items-center justify-between gap-1.5">
                    <span className="truncate font-mono text-xs text-[var(--ink)]">{plan.id}</span>
                    <span
                      className={`shrink-0 rounded-full px-1.5 py-0.5 text-xs font-medium ${planStatusTone(plan.status)}`}
                    >
                      {t(`knowledge.organize.status.${plan.status}`)}
                    </span>
                  </div>
                  <p className="mt-1 text-xs text-[var(--ink-muted)]">
                    {t('knowledge.organize.itemCount', { count: plan.items.length })}
                  </p>
                </button>
              </li>
            );
          })}
        </ul>
      </aside>

      {/* Main review pane */}
      {activePlan && (
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
          {/* Plan header & action bar */}
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4">
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="font-mono text-sm font-semibold text-[var(--ink)]">
                  {activePlan.id}
                </h3>
                <span
                  className={`rounded-full px-2 py-0.5 text-xs font-medium ${planStatusTone(activePlan.status)}`}
                  data-testid="organize-plan-status"
                >
                  {t(`knowledge.organize.status.${activePlan.status}`)}
                </span>
              </div>
              <p className="mt-1 text-xs text-[var(--ink-muted)]">
                {t('knowledge.organize.planMeta', {
                  count: activePlan.items.length,
                  expiresAt: activePlan.expiresAt.slice(0, 10),
                })}
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              {isReviewable && (
                <>
                  <button
                    type="button"
                    disabled={busyAction !== null}
                    onClick={() => setConfirmReject(true)}
                    className="rounded-lg border border-[var(--line)] bg-[var(--button-secondary-bg)] px-3 py-1.5 text-xs font-medium text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--hover-bg)] disabled:opacity-50"
                  >
                    {t('knowledge.organize.rejectPlan')}
                  </button>
                  <button
                    type="button"
                    disabled={busyAction !== null || selectedCount === 0}
                    onClick={() => void handleApproveAndExecute()}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-3.5 py-1.5 text-xs font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
                  >
                    <PlayIcon className="h-3.5 w-3.5" />
                    {busyAction === 'execute'
                      ? t('knowledge.organize.executing')
                      : t('knowledge.organize.approveAndExecute', { count: selectedCount })}
                  </button>
                </>
              )}

              {activePlan.status === 'approved' && (
                <button
                  type="button"
                  disabled={busyAction !== null}
                  onClick={() => void handleExecuteApproved()}
                  className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-3.5 py-1.5 text-xs font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
                >
                  <PlayIcon className="h-3.5 w-3.5" />
                  {busyAction === 'execute'
                    ? t('knowledge.organize.executing')
                    : t('knowledge.organize.executeNow')}
                </button>
              )}

              {canUndo && (
                <button
                  type="button"
                  disabled={busyAction !== null}
                  onClick={() => setConfirmUndo(true)}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--line)] bg-[var(--button-secondary-bg)] px-3 py-1.5 text-xs font-medium text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--hover-bg)] disabled:opacity-50"
                >
                  <UndoIcon className="h-3.5 w-3.5" />
                  {busyAction === 'undo'
                    ? t('knowledge.organize.undoing')
                    : t('knowledge.organize.undoPlan')}
                </button>
              )}
            </div>
          </div>

          {/* Execution progress / result summary banner */}
          {(busyAction === 'execute' || activePlan.status === 'executing') && (
            <div
              role="progressbar"
              aria-label={t('knowledge.organize.executing')}
              className="rounded-lg border border-[var(--line)] bg-[var(--paper-inset)] px-4 py-2.5 text-xs text-[var(--ink)]"
            >
              {t('knowledge.organize.executingBanner')}
            </div>
          )}

          {executionStats &&
            (activePlan.status === 'completed' ||
              activePlan.status === 'partially_completed' ||
              activePlan.status === 'interrupted') && (
              <div
                role="status"
                data-testid="organize-execution-summary"
                className={`rounded-lg border px-4 py-2.5 text-xs ${
                  activePlan.status === 'completed'
                    ? 'border-[var(--line)] bg-[var(--paper-elevated)] text-[var(--ink)]'
                    : 'border-[var(--error-subtle,var(--line))] bg-[var(--warning-bg)] text-[var(--ink)]'
                }`}
              >
                {t('knowledge.organize.executionSummary', {
                  succeeded: executionStats.succeeded,
                  conflict: executionStats.conflict,
                  failed: executionStats.failed,
                  skipped: executionStats.skipped,
                  undone: executionStats.undone,
                })}
              </div>
            )}

          {lastUndoReport && lastUndoReport.planId === activePlan.id && (
            <div
              role="status"
              data-testid="organize-undo-report"
              className="rounded-lg border border-[var(--line)] bg-[var(--paper-elevated)] px-4 py-2.5 text-xs text-[var(--ink)]"
            >
              {t('knowledge.organize.undoSummary', {
                undone: lastUndoReport.undone.length,
                skippedModified: lastUndoReport.skippedModified.length,
                conflicts: lastUndoReport.conflicts.length,
                failed: lastUndoReport.failed.length,
              })}
            </div>
          )}

          {/* Directory tree before/after diff preview */}
          <section
            className="rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4"
            aria-label={t('knowledge.organize.diffTitle')}
          >
            <h4 className="text-xs font-semibold uppercase tracking-wide text-[var(--ink-muted)]">
              {t('knowledge.organize.diffTitle')}
            </h4>
            <div className="mt-2.5 space-y-3">
              {diffGroups.map(([folderRoot, entries]) => (
                <div
                  key={folderRoot}
                  className="rounded-lg border border-[var(--line)] bg-[var(--paper)] p-3"
                >
                  <div className="mb-2 flex items-center gap-1.5 text-xs font-medium text-[var(--ink)]">
                    <FolderIcon className="h-3.5 w-3.5 shrink-0 text-[var(--ink-muted)]" />
                    <span className="truncate font-mono">{folderRoot}</span>
                  </div>
                  <div className="grid grid-cols-1 gap-1.5 text-xs font-mono">
                    {entries.map((entry) => (
                      <div
                        key={entry.id}
                        className={`flex flex-wrap items-center gap-1.5 rounded px-2 py-1 ${
                          entry.active
                            ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
                            : 'text-[var(--ink-subtle)] line-through opacity-60'
                        }`}
                      >
                        <span className="rounded bg-[var(--paper-elevated)] px-1.5 py-0.5 text-xs font-sans">
                          {t(`knowledge.organize.kind.${entry.kind}`)}
                        </span>
                        {entry.before ? (
                          <>
                            <span className="truncate text-[var(--ink-muted)]">{entry.before}</span>
                            <ArrowRightIcon className="h-3 w-3 shrink-0 text-[var(--ink-subtle)]" />
                            <span className="truncate font-medium text-[var(--ink)]">
                              {entry.after}
                            </span>
                          </>
                        ) : (
                          <span className="truncate font-medium text-[var(--ink)]">
                            + {entry.after}/
                          </span>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </section>

          {/* Item-by-item review table */}
          <section className="rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <h4 className="text-xs font-semibold uppercase tracking-wide text-[var(--ink-muted)]">
                {t('knowledge.organize.itemsTitle')}
              </h4>
              {isReviewable && (
                <div className="flex flex-wrap items-center gap-1.5">
                  <button
                    type="button"
                    onClick={() => handleSelectAll('safe')}
                    className="rounded-md border border-[var(--line)] px-2 py-1 text-xs text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
                  >
                    {t('knowledge.organize.selectSafe')}
                  </button>
                  <button
                    type="button"
                    onClick={() => handleSelectAll('all')}
                    className="rounded-md border border-[var(--line)] px-2 py-1 text-xs text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
                  >
                    {t('knowledge.organize.acceptAll')}
                  </button>
                  <button
                    type="button"
                    onClick={() => handleSelectAll('none')}
                    className="rounded-md border border-[var(--line)] px-2 py-1 text-xs text-[var(--ink-muted)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
                  >
                    {t('knowledge.organize.selectNone')}
                  </button>
                </div>
              )}
            </div>

            <ul className="space-y-2.5">
              {activePlan.items.map((item) => {
                const checked = Boolean(selectedItems[item.id]);
                const gitRisk = isGitRoot(item.folderRoot, kb.linkedFolders);
                const linkRisk = item.affectedMarkdownLinks > 0;
                const effectiveTarget = targetOverrides[item.id] ?? item.overrideTargetRel ?? item.targetRel;
                return (
                  <li
                    key={item.id}
                    data-testid={`organize-item-${item.id}`}
                    className="rounded-lg border border-[var(--line)] bg-[var(--paper)] p-3"
                  >
                    <div className="flex items-start gap-2.5">
                      {isReviewable ? (
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={() => handleToggleItem(item.id)}
                          aria-label={t('knowledge.organize.selectItemAria', {
                            target: item.targetRel,
                          })}
                          className="mt-1 h-4 w-4 shrink-0 rounded border-[var(--line)]"
                        />
                      ) : (
                        <span className="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center">
                          {item.approved && <CheckIcon className="h-3.5 w-3.5 text-[var(--ink-muted)]" />}
                        </span>
                      )}

                      <div className="min-w-0 flex-1 space-y-1.5">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="rounded bg-[var(--paper-inset)] px-1.5 py-0.5 text-xs font-medium text-[var(--ink)]">
                            {t(`knowledge.organize.kind.${item.kind}`)}
                          </span>
                          <span
                            className={`rounded-full px-2 py-0.5 text-xs font-medium ${itemStatusTone(item.status)}`}
                          >
                            {t(`knowledge.organize.itemStatus.${item.status}`)}
                          </span>
                          {linkRisk && (
                            <span
                              data-risk="markdown-links"
                              className="inline-flex items-center gap-1 rounded-full bg-[var(--warning-bg)] px-2 py-0.5 text-xs font-medium text-[var(--warning)]"
                            >
                              <WarningIcon className="h-3 w-3" />
                              {t('knowledge.organize.riskLinks', {
                                count: item.affectedMarkdownLinks,
                              })}
                            </span>
                          )}
                          {gitRisk && (
                            <span
                              data-risk="git"
                              className="inline-flex items-center gap-1 rounded-full bg-[var(--warning-bg)] px-2 py-0.5 text-xs font-medium text-[var(--warning)]"
                            >
                              <WarningIcon className="h-3 w-3" />
                              {t('knowledge.organize.riskGit')}
                            </span>
                          )}
                        </div>

                        <div className="flex flex-wrap items-center gap-1.5 font-mono text-xs text-[var(--ink)]">
                          {item.sourceRel && (
                            <>
                              <span className="break-all text-[var(--ink-muted)]">
                                {item.sourceRel}
                              </span>
                              <ArrowRightIcon className="h-3.5 w-3.5 shrink-0 text-[var(--ink-subtle)]" />
                            </>
                          )}
                          {isReviewable ? (
                            <input
                              type="text"
                              value={effectiveTarget}
                              onChange={(event) => handleOverrideChange(item.id, event.target.value)}
                              aria-label={t('knowledge.organize.editTargetAria', {
                                id: item.id,
                              })}
                              className="min-w-[200px] flex-1 rounded border border-[var(--line)] bg-[var(--paper-elevated)] px-2 py-0.5 font-mono text-xs text-[var(--ink)] focus:border-[var(--accent)] focus:outline-none"
                            />
                          ) : (
                            <span className="break-all font-medium">{effectiveTarget}</span>
                          )}
                        </div>

                        <p className="text-xs text-[var(--ink-muted)]">{item.reason}</p>

                        {item.error && (
                          <p
                            role="alert"
                            className="rounded bg-[var(--error-subtle,var(--warning-bg))] px-2 py-1 font-mono text-xs text-[var(--error)]"
                          >
                            {item.error}
                          </p>
                        )}
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          </section>
        </div>
      )}

      {confirmReject && activePlan && (
        <ConfirmDialog
          title={t('knowledge.organize.rejectTitle')}
          message={t('knowledge.organize.rejectMessage')}
          confirmText={t('knowledge.organize.rejectPlan')}
          cancelText={t('knowledge.cancel')}
          confirmVariant="danger"
          loading={busyAction === 'reject'}
          onConfirm={() => void handleConfirmReject()}
          onCancel={() => {
            if (!busyAction) setConfirmReject(false);
          }}
        />
      )}

      {confirmUndo && activePlan && (
        <ConfirmDialog
          title={t('knowledge.organize.undoTitle')}
          message={t('knowledge.organize.undoMessage')}
          confirmText={t('knowledge.organize.undoPlan')}
          cancelText={t('knowledge.cancel')}
          confirmVariant="danger"
          loading={busyAction === 'undo'}
          onConfirm={() => void handleConfirmUndo()}
          onCancel={() => {
            if (!busyAction) setConfirmUndo(false);
          }}
        />
      )}
    </div>
  );
}
