/**
 * 自定义默认设置 inside a provider's settings panel: whether this provider is
 * the global default, its default model (the existing providerPrimaryModels
 * override) and its default reasoning effort. New tasks fall back to these
 * when their project (Agent) has no explicit choice of its own.
 */
import { useTranslation } from 'react-i18next';

import type { Provider } from '@/config/types';
import {
  REASONING_EFFORT_DEFAULT,
  REASONING_EFFORT_DESCRIPTIONS,
  reasoningEffortChoices,
} from '@/../shared/reasoningEffort';

interface ProviderDefaultsSectionProps {
  provider: Provider;
  isDefaultProvider: boolean;
  defaultModel: string | undefined;
  defaultEffort: string | undefined;
  onDefaultProviderChange: (isDefault: boolean) => void;
  onDefaultModelChange: (model: string) => void;
  onDefaultEffortChange: (effort: string) => void;
}

export function ProviderDefaultsSection({
  provider,
  isDefaultProvider,
  defaultModel,
  defaultEffort,
  onDefaultProviderChange,
  onDefaultModelChange,
  onDefaultEffortChange,
}: ProviderDefaultsSectionProps) {
  const { t } = useTranslation('settings');
  const models = provider.models ?? [];
  const effort = defaultEffort ?? REASONING_EFFORT_DEFAULT;
  const effortChoices = [
    REASONING_EFFORT_DEFAULT,
    ...(reasoningEffortChoices(
      'builtin',
      provider.apiProtocol,
      provider.id,
      defaultModel,
      provider.config.baseUrl,
    ) ?? []),
  ];
  const chip = (selected: boolean) =>
    `rounded-full border px-3 py-1 text-xs font-medium transition-colors ${
      selected
        ? 'border-[var(--accent)] bg-[var(--accent)]/10 text-[var(--accent)]'
        : 'border-[var(--line)] text-[var(--ink-muted)] hover:border-[var(--line-strong)] hover:text-[var(--ink)]'
    }`;

  return (
    <section className="rounded-xl border border-[var(--line)] p-4" data-provider-defaults>
      <h4 className="text-sm font-semibold text-[var(--ink)]">{t('providers.defaults.title')}</h4>
      <p className="mt-1 text-xs text-[var(--ink-muted)]">{t('providers.defaults.description')}</p>

      <div className="mt-3 flex items-center justify-between gap-3">
        <span className="text-sm text-[var(--ink)]">{t('providers.defaults.defaultProvider')}</span>
        <button
          type="button"
          role="switch"
          aria-checked={isDefaultProvider}
          aria-label={t('providers.defaults.defaultProvider')}
          onClick={() => onDefaultProviderChange(!isDefaultProvider)}
          className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border-2 border-transparent transition-colors ${
            isDefaultProvider ? 'bg-[var(--accent)]' : 'bg-[var(--line-strong)]'
          }`}
        >
          <span
            className={`pointer-events-none inline-block h-3.5 w-3.5 rounded-full bg-[var(--toggle-thumb)] shadow-sm transition-transform ${
              isDefaultProvider ? 'translate-x-4' : 'translate-x-0.5'
            }`}
          />
        </button>
      </div>

      {models.length > 0 && (
        <div className="mt-3">
          <div className="mb-1.5 text-sm text-[var(--ink)]">{t('providers.defaults.defaultModel')}</div>
          <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label={t('providers.defaults.defaultModel')}>
            {models.map((model) => (
              <button
                key={model.model}
                type="button"
                role="radio"
                aria-checked={model.model === defaultModel}
                onClick={() => onDefaultModelChange(model.model)}
                className={chip(model.model === defaultModel)}
              >
                {model.modelName || model.model}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="mt-3">
        <div className="mb-1.5 text-sm text-[var(--ink)]">{t('providers.defaults.defaultEffort')}</div>
        <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label={t('providers.defaults.defaultEffort')}>
          {effortChoices.map((level) => (
            <button
              key={level}
              type="button"
              role="radio"
              aria-checked={level === effort}
              title={REASONING_EFFORT_DESCRIPTIONS[level]}
              onClick={() => onDefaultEffortChange(level)}
              className={chip(level === effort)}
            >
              {level === REASONING_EFFORT_DEFAULT ? t('providers.defaults.effortFollowProvider') : level}
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}
