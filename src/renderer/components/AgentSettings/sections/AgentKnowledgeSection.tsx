import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { BookIcon } from '@/components/icons';
import { useToast } from '@/components/Toast';
import { patchAgentConfig } from '@/config/services/agentConfigService';
import { knowledgeClient } from '@/features/knowledge/knowledgeClient';
import type { KnowledgeBaseRecord } from '@/features/knowledge/types';
import { useKnowledgeChangeSubscription } from '@/features/knowledge/useKnowledgeEvents';

import type { AgentConfig } from '../../../../shared/types/agent';

interface AgentKnowledgeSectionProps {
  agent: AgentConfig;
  onAgentChanged: () => void;
}

/**
 * Agent-level default knowledge base bindings (`AgentConfig.knowledgeBaseIds`).
 * Writes through `patchAgentConfig` (`agent-config-intent.lock`) and does not
 * mutate existing session snapshots.
 */
export default function AgentKnowledgeSection({
  agent,
  onAgentChanged,
}: AgentKnowledgeSectionProps) {
  const { t } = useTranslation('settings');
  const toast = useToast();
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    toastRef.current = toast;
  }, [toast]);

  const [bases, setBases] = useState<KnowledgeBaseRecord[] | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
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
      const list = await knowledgeClient.list();
      if (mounted.current && loadSeq.current === seq) {
        setBases(list);
      }
    } catch {
      if (mounted.current && loadSeq.current === seq) {
        setBases([]);
      }
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useKnowledgeChangeSubscription(() => {
    void reload();
  });

  const enabledIds = useMemo(
    () => agent.knowledgeBaseIds ?? [],
    [agent.knowledgeBaseIds],
  );

  const handleToggle = useCallback(
    async (kbId: string) => {
      if (busyId) return;
      setBusyId(kbId);
      const next = enabledIds.includes(kbId)
        ? enabledIds.filter((id) => id !== kbId)
        : [...enabledIds, kbId];
      try {
        await patchAgentConfig(agent.id, { knowledgeBaseIds: next });
        if (mounted.current) onAgentChanged();
      } catch {
        toastRef.current.error(t('agentSettings.knowledge.saveFailed'));
      } finally {
        if (mounted.current) setBusyId(null);
      }
    },
    [agent.id, busyId, enabledIds, onAgentChanged, t],
  );

  return (
    <section data-testid="agent-knowledge-section">
      <h3 className="flex items-center gap-2 text-base font-medium text-[var(--ink)]">
        <BookIcon className="h-[18px] w-[18px] text-[var(--ink-muted)]" />
        {t('agentSettings.knowledge.title')}
      </h3>
      <p className="mt-0.5 text-xs text-[var(--ink-muted)]">
        {t('agentSettings.knowledge.subtitle')}
      </p>

      <div className="mt-3">
        {bases === null ? null : bases.length === 0 ? (
          <p className="rounded-lg border border-dashed border-[var(--line-strong)] px-3 py-4 text-xs text-[var(--ink-muted)]">
            {t('agentSettings.knowledge.empty')}
          </p>
        ) : (
          <ul className="divide-y divide-[var(--line)] rounded-lg border border-[var(--line)] bg-[var(--paper)]">
            {bases.map((kb) => {
              const checked = enabledIds.includes(kb.id);
              return (
                <li key={kb.id} className="flex items-center justify-between gap-3 px-3.5 py-2.5">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-[var(--ink)]">{kb.name}</div>
                    {kb.description && (
                      <div className="truncate text-xs text-[var(--ink-muted)]">
                        {kb.description}
                      </div>
                    )}
                  </div>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={checked}
                    aria-label={kb.name}
                    disabled={busyId === kb.id}
                    onClick={() => void handleToggle(kb.id)}
                    className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] disabled:opacity-50 ${
                      checked ? 'bg-[var(--accent)]' : 'bg-[var(--line-strong)]'
                    }`}
                  >
                    <span
                      className={`pointer-events-none inline-block h-3.5 w-3.5 rounded-full bg-[var(--toggle-thumb)] shadow-sm transition-transform ${
                        checked ? 'translate-x-4' : 'translate-x-0.5'
                      }`}
                    />
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}
