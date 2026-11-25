import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import CustomSelect, { type SelectOption } from '@/components/CustomSelect';
import { CheckIcon, LoaderIcon, WarningIcon } from '@/components/icons';
import { useToast } from '@/components/Toast';
import {
  fetchProviderEmbeddingModels,
  supportsModelDiscovery,
  type DiscoveredEmbeddingModel,
} from '@/config/services/modelDiscoveryService';
import type { EmbeddingModelConfig, Provider } from '@/config/types';
import { knowledgeClient } from '@/features/knowledge/knowledgeClient';
import type { EmbeddingConnectionTestResult } from '@/features/knowledge/types';
import { TOKENDANCE_PROVIDER_ID } from '../../../../shared/tokendance';

const RECOMMENDED_TOKENDANCE_EMBEDDING_MODEL = 'qwen-text-embedding-v4';
const DEFAULT_TOKENDANCE_DIMENSIONS = 1024;
const MANUAL_MODEL_SENTINEL = '__manual__';

interface EmbeddingModelCardProps {
  providers: Provider[];
  apiKeys?: Record<string, string>;
  embeddingModel?: EmbeddingModelConfig;
  onSaveEmbeddingModel: (next: EmbeddingModelConfig) => Promise<void>;
}

export default function EmbeddingModelCard({
  providers,
  apiKeys,
  embeddingModel,
  onSaveEmbeddingModel,
}: EmbeddingModelCardProps) {
  const { t } = useTranslation('settings');
  const toast = useToast();

  const eligibleProviders = useMemo(
    () => providers.filter((p) => p.type !== 'subscription'),
    [providers],
  );
  const hasTokenDanceKey = Boolean(apiKeys?.[TOKENDANCE_PROVIDER_ID]?.trim());
  const hasTokenDanceProvider = useMemo(
    () => eligibleProviders.some((p) => p.id === TOKENDANCE_PROVIDER_ID),
    [eligibleProviders],
  );

  const initialProviderId =
    embeddingModel?.providerId ||
    (hasTokenDanceKey && hasTokenDanceProvider ? TOKENDANCE_PROVIDER_ID : '');
  const initialDimensions = embeddingModel?.dimensions
    ? String(embeddingModel.dimensions)
    : initialProviderId === TOKENDANCE_PROVIDER_ID && !embeddingModel?.providerId
      ? String(DEFAULT_TOKENDANCE_DIMENSIONS)
      : '';

  const [enabled, setEnabled] = useState(Boolean(embeddingModel?.enabled));
  const [providerId, setProviderId] = useState(initialProviderId);
  const [model, setModel] = useState(embeddingModel?.model ?? '');
  const [dimensions, setDimensions] = useState<string>(initialDimensions);
  const [manualModelMode, setManualModelMode] = useState(false);
  const [discoveredModels, setDiscoveredModels] = useState<DiscoveredEmbeddingModel[]>([]);
  const [discoveryState, setDiscoveryState] = useState<'idle' | 'loading' | 'ready' | 'error'>('idle');
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<EmbeddingConnectionTestResult | null>(null);

  const syncedKey = `${String(Boolean(embeddingModel?.enabled))}:${embeddingModel?.providerId ?? ''}:${embeddingModel?.model ?? ''}:${embeddingModel?.dimensions ?? ''}`;
  const [lastSyncedKey, setLastSyncedKey] = useState(syncedKey);
  if (lastSyncedKey !== syncedKey) {
    setLastSyncedKey(syncedKey);
    const nextProviderId =
      embeddingModel?.providerId ||
      (hasTokenDanceKey && hasTokenDanceProvider ? TOKENDANCE_PROVIDER_ID : '');
    setEnabled(Boolean(embeddingModel?.enabled));
    setProviderId(nextProviderId);
    setModel(embeddingModel?.model ?? '');
    setDimensions(
      embeddingModel?.dimensions
        ? String(embeddingModel.dimensions)
        : nextProviderId === TOKENDANCE_PROVIDER_ID && !embeddingModel?.providerId
          ? String(DEFAULT_TOKENDANCE_DIMENSIONS)
          : '',
    );
  }

  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const selectedProvider = useMemo(
    () => eligibleProviders.find((p) => p.id === providerId),
    [eligibleProviders, providerId],
  );
  const selectedApiKey = providerId ? apiKeys?.[providerId] : undefined;

  useEffect(() => {
    let cancelled = false;
    if (!selectedProvider || !supportsModelDiscovery(selectedProvider)) {
      setDiscoveredModels([]);
      setDiscoveryState('idle');
      return;
    }
    const isTokenDance = selectedProvider.id === TOKENDANCE_PROVIDER_ID;
    if (!isTokenDance && !selectedApiKey?.trim()) {
      setDiscoveredModels([]);
      setDiscoveryState('idle');
      return;
    }

    setDiscoveryState('loading');
    void fetchProviderEmbeddingModels(selectedProvider, selectedApiKey)
      .then((models) => {
        if (cancelled) return;
        setDiscoveredModels(models);
        setDiscoveryState('ready');
        if (isTokenDance && models.length > 0) {
          setModel((current) => {
            if (current.trim()) return current;
            const preferred =
              models.find((m) => m.id === RECOMMENDED_TOKENDANCE_EMBEDDING_MODEL) ?? models[0];
            return preferred.id;
          });
          setDimensions((current) =>
            current.trim() ? current : String(DEFAULT_TOKENDANCE_DIMENSIONS),
          );
        }
      })
      .catch(() => {
        if (cancelled) return;
        setDiscoveredModels([]);
        setDiscoveryState('error');
      });

    return () => {
      cancelled = true;
    };
  }, [selectedApiKey, selectedProvider]);

  const providerOptions = useMemo<SelectOption[]>(
    () => [
      { value: '', label: t('providers.embedding.selectProvider') },
      ...eligibleProviders.map((p) => ({ value: p.id, label: p.name })),
    ],
    [eligibleProviders, t],
  );

  const modelOptions = useMemo<SelectOption[]>(() => {
    const items: SelectOption[] = discoveredModels.map((m) => {
      const baseLabel = m.displayName ? `${m.displayName} (${m.id})` : m.id;
      const isRecommended =
        providerId === TOKENDANCE_PROVIDER_ID && m.id === RECOMMENDED_TOKENDANCE_EMBEDDING_MODEL;
      return {
        value: m.id,
        label: isRecommended ? `${baseLabel} · ${t('providers.embedding.recommendedBadge')}` : baseLabel,
      };
    });
    items.push({ value: MANUAL_MODEL_SENTINEL, label: t('providers.embedding.manualModelOption') });
    return items;
  }, [discoveredModels, providerId, t]);

  const showDiscoveredSelect =
    discoveredModels.length > 0 &&
    !manualModelMode &&
    (!model.trim() || discoveredModels.some((m) => m.id === model));

  const parsedDimensions = useMemo(() => {
    const trimmed = dimensions.trim();
    if (!trimmed) return undefined;
    const n = Number(trimmed);
    return Number.isInteger(n) && n > 0 && n <= 8192 ? n : undefined;
  }, [dimensions]);

  const handleProviderChange = useCallback((nextId: string) => {
    setProviderId(nextId);
    setManualModelMode(false);
    setTestResult(null);
    if (nextId === TOKENDANCE_PROVIDER_ID) {
      setModel('');
      setDimensions(String(DEFAULT_TOKENDANCE_DIMENSIONS));
    }
  }, []);

  const handleSave = useCallback(async () => {
    if (saving) return;
    setSaving(true);
    try {
      await onSaveEmbeddingModel({
        enabled,
        providerId: providerId.trim(),
        model: model.trim(),
        ...(parsedDimensions ? { dimensions: parsedDimensions } : {}),
      });
      if (mounted.current) {
        toast.success(t('providers.embedding.saved'));
      }
    } catch {
      if (mounted.current) {
        toast.error(t('providers.embedding.saveFailed'));
      }
    } finally {
      if (mounted.current) setSaving(false);
    }
  }, [enabled, model, onSaveEmbeddingModel, parsedDimensions, providerId, saving, t, toast]);

  const handleTest = useCallback(async () => {
    if (testing || !providerId.trim() || !model.trim()) return;
    setTesting(true);
    setTestResult(null);
    try {
      const res = await knowledgeClient.testEmbeddingConnection(
        providerId.trim(),
        model.trim(),
        parsedDimensions,
      );
      if (!mounted.current) return;
      setTestResult(res);
      if (res.ok && res.dimension) {
        setDimensions(String(res.dimension));
        await onSaveEmbeddingModel({
          enabled,
          providerId: providerId.trim(),
          model: model.trim(),
          dimensions: res.dimension,
        });
      }
    } catch {
      if (mounted.current) {
        setTestResult({ ok: false, latencyMs: 0, errorKind: 'network_unreachable' });
      }
    } finally {
      if (mounted.current) setTesting(false);
    }
  }, [enabled, model, onSaveEmbeddingModel, parsedDimensions, providerId, testing]);

  const showTokenDanceLoggedOutHint =
    !hasTokenDanceKey && (providerId === TOKENDANCE_PROVIDER_ID || !providerId);

  return (
    <section
      data-testid="embedding-model-card"
      className="mb-6 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-base font-semibold text-[var(--ink)]">
            {t('providers.embedding.title')}
          </h3>
          <p className="mt-1 text-xs text-[var(--ink-muted)]">
            {t('providers.embedding.privacyNotice')}
          </p>
        </div>

        <label className="inline-flex shrink-0 items-center gap-2 text-sm text-[var(--ink)]">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
            className="h-4 w-4 rounded border-[var(--line)]"
          />
          <span>{t('providers.embedding.enableToggle')}</span>
        </label>
      </div>

      {showTokenDanceLoggedOutHint && (
        <p
          data-testid="embedding-tokendance-logged-out-hint"
          className="mt-3 rounded-lg bg-[var(--paper-inset)] px-3 py-2 text-xs text-[var(--ink-muted)]"
        >
          {t('providers.embedding.tokenDanceLoggedOutHint')}
        </p>
      )}

      <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-3">
        <div className="space-y-1">
          <span className="block text-xs text-[var(--ink-muted)]">
            {t('providers.embedding.providerLabel')}
          </span>
          <CustomSelect
            value={providerId}
            options={providerOptions}
            onChange={handleProviderChange}
            ariaLabel={t('providers.embedding.providerLabel')}
            size="sm"
            className="w-full"
          />
        </div>

        <div className="space-y-1">
          <span className="block text-xs text-[var(--ink-muted)]">
            {t('providers.embedding.modelLabel')}
          </span>
          {showDiscoveredSelect ? (
            <CustomSelect
              value={model}
              options={modelOptions}
              onChange={(nextVal) => {
                if (nextVal === MANUAL_MODEL_SENTINEL) {
                  setManualModelMode(true);
                } else {
                  setModel(nextVal);
                }
              }}
              ariaLabel={t('providers.embedding.modelLabel')}
              size="sm"
              className="w-full"
            />
          ) : (
            <input
              type="text"
              value={model}
              onChange={(event) => setModel(event.target.value)}
              placeholder={t('providers.embedding.modelPlaceholder')}
              className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-2.5 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
            />
          )}
          {discoveryState === 'loading' && (
            <p className="text-xs text-[var(--ink-muted)]">
              {t('providers.embedding.discoveryLoading')}
            </p>
          )}
          {discoveryState === 'error' && (
            <p
              data-testid="embedding-discovery-error"
              className="text-xs text-[var(--ink-muted)]"
            >
              {t('providers.embedding.discoveryFailed')}
            </p>
          )}
        </div>

        <label className="space-y-1">
          <span className="block text-xs text-[var(--ink-muted)]">
            {t('providers.embedding.dimensionsLabel')}
          </span>
          <input
            type="text"
            inputMode="numeric"
            value={dimensions}
            onChange={(event) => setDimensions(event.target.value)}
            placeholder={t('providers.embedding.dimensionsPlaceholder')}
            className="h-9 w-full rounded-lg border border-[var(--line-strong)] bg-[var(--paper)] px-2.5 text-sm text-[var(--ink)] outline-none focus:border-[var(--accent)]"
          />
        </label>
      </div>

      <div className="mt-4 flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0 flex-1 text-xs">
          {testResult && (
            <div
              role="status"
              data-testid="embedding-test-result"
              className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 ${
                testResult.ok
                  ? 'bg-[var(--paper-inset)] text-[var(--ink)]'
                  : 'bg-[var(--error-bg,var(--warning-bg))] text-[var(--error)]'
              }`}
            >
              {testResult.ok ? (
                <>
                  <CheckIcon className="h-3.5 w-3.5 shrink-0" />
                  <span>
                    {t('providers.embedding.testOk', {
                      dimension: testResult.dimension ?? 0,
                      ms: testResult.latencyMs,
                    })}
                  </span>
                </>
              ) : (
                <>
                  <WarningIcon className="h-3.5 w-3.5 shrink-0" />
                  <span>
                    {t(`providers.embedding.errors.${testResult.errorKind ?? 'network_unreachable'}`, {
                      defaultValue: t('providers.embedding.errors.network_unreachable'),
                    })}
                  </span>
                </>
              )}
            </div>
          )}
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            disabled={testing || !providerId.trim() || !model.trim()}
            onClick={() => void handleTest()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--line)] bg-[var(--button-secondary-bg)] px-3 py-1.5 text-xs font-medium text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--hover-bg)] disabled:opacity-50"
          >
            {testing && <LoaderIcon className="h-3.5 w-3.5 animate-spin" />}
            {testing ? t('providers.embedding.testing') : t('providers.embedding.testConnection')}
          </button>
          <button
            type="button"
            disabled={saving}
            onClick={() => void handleSave()}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--button-primary-bg)] px-3.5 py-1.5 text-xs font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
          >
            {saving ? t('providers.embedding.saving') : t('providers.embedding.save')}
          </button>
        </div>
      </div>
    </section>
  );
}
