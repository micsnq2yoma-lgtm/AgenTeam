import { createContext, useCallback, useContext, useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { listenWithCleanup } from '@/utils/tauriListen';

interface RestrictionStatus {
  enabled: boolean;
  expectedIp: string;
  observedIp: string | null;
  state: 'disabled' | 'checking' | 'matched' | 'mismatch' | 'unavailable';
}

interface RestrictionControls {
  status: RestrictionStatus;
  save: (enabled: boolean, expectedIp: string) => Promise<void>;
  recheck: () => Promise<void>;
}

const RestrictionContext = createContext<RestrictionControls | null>(null);
const actionClass = 'rounded-lg border border-[var(--line)] px-3 py-1.5 text-xs font-medium text-[var(--ink)] hover:bg-[var(--hover-bg)] disabled:opacity-50';

export function useEgressRestrictionEnabled() {
  return useContext(RestrictionContext)?.status.enabled === true;
}

export function RestrictedModelHint() {
  const enabled = useEgressRestrictionEnabled();
  const { t } = useTranslation('settings');
  return enabled && <p className="px-3 py-2 text-xs text-[var(--ink-muted)]">{t('proxy.restriction.modelHint')}</p>;
}

export function EgressRestrictionSettings() {
  const { t } = useTranslation('settings');
  const controls = useContext(RestrictionContext);
  if (controls) return <RestrictionForm key={controls.status.expectedIp} {...controls} />;
  return (
    <div className="rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
      <h3 className="text-sm font-medium text-[var(--ink)]">{t('proxy.restriction.title')}</h3>
      <p className="mt-1 text-xs text-[var(--ink-muted)]">{t('proxy.restriction.desktopOnly')}</p>
    </div>
  );
}

function RestrictionForm({ status, save, recheck }: RestrictionControls) {
  const { t } = useTranslation('settings');
  const inputId = useId();
  const explanationId = useId();
  const [draft, setDraft] = useState(status.expectedIp);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<'invalidIp' | 'saveError' | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const submitting = useRef(false);

  async function submit(action: 'enable' | 'disable' | 'recheck') {
    if (submitting.current) return;
    const ip = draft.trim();
    if (action === 'enable' && !ip) {
      setError('invalidIp');
      input.current?.focus();
      return;
    }
    submitting.current = true;
    setPending(true);
    setError(null);
    try {
      if (action === 'recheck') await recheck();
      else await save(action === 'enable', action === 'disable' ? status.expectedIp : ip);
    } catch (cause) {
      if (String(cause).includes('invalid_expected_ip')) {
        setError('invalidIp');
        input.current?.focus();
      } else {
        setError('saveError');
      }
    } finally {
      submitting.current = false;
      setPending(false);
    }
  }

  const targetChanged = draft.trim() !== status.expectedIp;
  return (
    <form onSubmit={event => {
      event.preventDefault();
      void submit(status.enabled && !targetChanged ? 'recheck' : 'enable');
    }} className="space-y-4 rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h3 className="text-sm font-medium text-[var(--ink)]">{t('proxy.restriction.title')}</h3>
          <p id={explanationId} className="mt-1 text-xs text-[var(--ink-muted)]">{t('proxy.restriction.description')}</p>
          <p className="mt-1 text-xs text-[var(--ink-muted)]">{t('proxy.restriction.modelHint')}</p>
        </div>
        <button type="button" role="switch" aria-checked={status.enabled} aria-label={t('proxy.restriction.title')}
          aria-describedby={explanationId} disabled={pending}
          onClick={() => void submit(status.enabled ? 'disable' : 'enable')}
          className={`relative h-6 w-11 shrink-0 rounded-full transition-colors disabled:opacity-50 ${status.enabled ? 'bg-[var(--accent)]' : 'bg-[var(--line-strong)]'}`}>
          <span className={`absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-[var(--toggle-thumb)] shadow transition-transform ${status.enabled ? 'translate-x-5' : 'translate-x-0'}`} />
        </button>
      </div>
      <div className="border-t border-[var(--line)] pt-4">
        <label htmlFor={inputId} className="block text-xs font-medium text-[var(--ink)]">{t('proxy.restriction.target')}</label>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <input id={inputId} ref={input} value={draft} type="text" dir="ltr" spellCheck={false} autoComplete="off"
            aria-invalid={error === 'invalidIp'} aria-describedby={error ? inputId + '-error' : undefined}
            onChange={event => { setDraft(event.target.value); setError(null); }}
            placeholder={t('proxy.restriction.placeholder')}
            className="min-w-32 flex-1 rounded-lg border border-[var(--line)] bg-[var(--paper)] px-3 py-1.5 text-sm text-[var(--ink)] placeholder:text-[var(--ink-faint)] focus:border-[var(--focus-border)] focus:outline-none" />
          {(status.enabled || draft.trim()) && (
            <button type="submit" className={actionClass} disabled={pending}>
              {status.enabled
                ? targetChanged ? t('proxy.restriction.save') : t('proxy.restriction.retry')
                : t('proxy.restriction.enable')}
            </button>
          )}
        </div>
        {error && <p role="alert" id={inputId + '-error'} className="mt-2 text-xs text-[var(--error)]">{t(`proxy.restriction.${error}`)}</p>}
        <p className="mt-2 text-xs text-[var(--ink-muted)]">{t('proxy.restriction.restartHint')}</p>
      </div>
      {status.enabled && (
        <p role="status" className="break-words text-xs text-[var(--ink-muted)]">
          {status.state === 'matched' ? t('proxy.restriction.matched', { ip: status.observedIp }) :
            status.state === 'mismatch' ? t('proxy.restriction.mismatch', { ip: status.observedIp, expected: status.expectedIp }) :
            status.state === 'checking' ? t('proxy.restriction.checking') : t('proxy.restriction.unavailable')}
        </p>
      )}
    </form>
  );
}

/** Mount business components only after the native gate admits this window. */
export function EgressRestrictionGate({ children, auxiliary = false, recoveryOnly = false }: {
  children: ReactNode;
  auxiliary?: boolean;
  recoveryOnly?: boolean;
}) {
  const { t } = useTranslation('settings');
  const [status, setStatus] = useState<RestrictionStatus | null>(null);
  const [readFailed, setReadFailed] = useState(false);
  const readSequence = useRef(0);
  const refresh = useCallback(async () => {
    const sequence = ++readSequence.current;
    try {
      const current = await invoke<RestrictionStatus>('cmd_get_egress_restriction');
      if (sequence === readSequence.current) {
        setStatus(current);
        setReadFailed(false);
      }
    } catch {
      if (sequence === readSequence.current) {
        setStatus(null);
        setReadFailed(true);
      }
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    // Subscribe first so a transition during the initial read cannot be lost.
    void listenWithCleanup('egress-restriction-changed', refresh, controller.signal)
      .then(() => { if (!controller.signal.aborted) void refresh(); });
    const timer = window.setInterval(refresh, 15_000);
    window.addEventListener('focus', refresh);
    return () => {
      controller.abort();
      window.clearInterval(timer);
      window.removeEventListener('focus', refresh);
    };
  }, [refresh]);

  const save = async (enabled: boolean, expectedIp: string) => {
    await invoke('cmd_set_egress_restriction', { enabled, expectedIp });
    await refresh();
  };
  const recheck = async () => {
    await invoke('cmd_recheck_egress_restriction');
    await refresh();
  };
  const admitted = status && ((!status.enabled && status.state === 'disabled') || (status.enabled && status.state === 'matched'));
  if (admitted && !recoveryOnly) {
    return <RestrictionContext.Provider value={{ status, save, recheck }}>{children}</RestrictionContext.Provider>;
  }
  if (auxiliary) return null;
  return (
    <main className="flex h-screen flex-col overflow-auto bg-[var(--paper)] text-[var(--ink)]">
      <div data-tauri-drag-region className="h-11 shrink-0" />
      <section className="m-auto w-full max-w-lg space-y-4 px-5 pb-11">
        <h1 className="text-lg font-semibold">
          {admitted ? t('proxy.restriction.reopening') : status || readFailed ? t('proxy.restriction.locked') : t('proxy.restriction.loading')}
        </h1>
        {status && !admitted && <RestrictionForm key={status.expectedIp} status={status} save={save} recheck={recheck} />}
        {readFailed && <>
          <p role="alert" className="text-sm text-[var(--ink-muted)]">{t('proxy.restriction.readError')}</p>
          <button type="button" className={actionClass} onClick={() => void refresh()}>{t('proxy.restriction.retry')}</button>
        </>}
      </section>
    </main>
  );
}
