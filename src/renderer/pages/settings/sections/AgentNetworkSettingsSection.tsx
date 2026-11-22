import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useToast } from '@/components/Toast';
import { useConfig } from '@/hooks/useConfig';
import AgentNetwork from '@/pages/AgentNetwork';

export function AgentNetworkSettingsSection({ isActive }: { isActive?: boolean }) {
  const { t } = useTranslation('settings');
  const { config, updateConfig } = useConfig();
  const toast = useToast();
  const descriptionId = useId();
  const [saving, setSaving] = useState(false);
  const enabled = config.agentNetworkSelfhostDevGate === true;

  async function toggle() {
    setSaving(true);
    try {
      await updateConfig({ agentNetworkSelfhostDevGate: !enabled });
    } catch {
      toast.error(t('general.saveFailedRetry'));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mx-auto max-w-5xl px-8 py-8 text-[var(--ink)]">
      <h1 className="mb-5 text-xl font-semibold">{t('sidebar.nav.agentNetwork')}</h1>
      <div className="mb-6 flex items-center justify-between gap-4 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
        <div className="min-w-0">
          <h2 className="text-sm font-medium">{t('agentNetwork.enableTitle')}</h2>
          <p id={descriptionId} className="mt-1 text-xs text-[var(--ink-muted)]">
            {t('agentNetwork.enableDescription')}
          </p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-label={t('agentNetwork.enableTitle')}
          aria-describedby={descriptionId}
          disabled={saving}
          onClick={() => void toggle()}
          className={`relative h-6 w-11 shrink-0 rounded-full transition-colors disabled:opacity-60 ${enabled ? 'bg-[var(--accent)]' : 'bg-[var(--line-strong)]'}`}
        >
          <span className={`absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-[var(--toggle-thumb)] shadow transition-transform ${enabled ? 'translate-x-5' : 'translate-x-0'}`} />
        </button>
      </div>
      <AgentNetwork mode="settings" isActive={isActive} />
    </div>
  );
}
