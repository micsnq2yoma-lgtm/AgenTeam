import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import CustomSelect from '@/components/CustomSelect';
import { useToast } from '@/components/Toast';
import type { AppConfig } from '@/config/types';
import { useAvailableProviders } from '@/hooks/useAvailableProviders';
import { isProviderModelCompatibleWithRuntime } from '@/utils/runtimeProviderProjection';
import type { RuntimeType } from '../../../../shared/types/runtime';

interface DefaultModelSettingProps {
  selection: AppConfig['defaultModelSelection'];
  runtime: RuntimeType;
  updateConfig: (patch: Partial<AppConfig>) => Promise<void>;
}

export function DefaultModelSetting({ selection, runtime, updateConfig }: DefaultModelSettingProps) {
  const { t } = useTranslation('settings');
  const toast = useToast();
  const providers = useAvailableProviders();
  const [saving, setSaving] = useState(false);
  const title = t('general.defaultModelTitle');
  const choices = providers.flatMap(provider => provider.models
    .filter(model => isProviderModelCompatibleWithRuntime(runtime, provider, model.model))
    .map(model => ({
      value: JSON.stringify([provider.id, model.model]),
      label: `${provider.name} · ${model.modelName || model.model}`,
      selection: { providerId: provider.id, model: model.model },
    })));
  const value = selection ? JSON.stringify([selection.providerId, selection.model]) : '';
  const options = [{ value: '', label: t('general.defaultModelAutomatic') }, ...choices];

  async function save(next: string) {
    if (next === value) return;
    setSaving(true);
    try {
      await updateConfig({ defaultModelSelection: choices.find(choice => choice.value === next)?.selection });
    } catch {
      toast.error(t('general.saveFailedRetry'));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mt-4 flex items-center justify-between gap-4 border-t border-[var(--line)] pt-4">
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-[var(--ink)]">{title}</p>
        <p className="text-xs text-[var(--ink-muted)]">
          {t(choices.length ? 'general.defaultModelDescription' : 'general.defaultModelNoProviders')}
        </p>
      </div>
      <CustomSelect
        ariaLabel={title}
        value={value}
        options={options}
        placeholder={t('general.defaultModelUnavailable')}
        disabled={saving || (!selection && choices.length === 0)}
        size="md"
        className="w-72 max-w-[45%] shrink-0"
        popoverMinWidth={220}
        onChange={next => void save(next)}
      />
    </div>
  );
}
