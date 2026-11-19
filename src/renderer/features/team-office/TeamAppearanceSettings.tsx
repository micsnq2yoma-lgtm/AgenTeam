/**
 * Settings → 团队形象. Picks the skin of the launcher's pixel team office
 * (persisted as `config.teamOfficeSkin`) with a live preview of every skin,
 * and hosts the desktop pet settings underneath when the pet is available.
 */

import { memo, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useToast } from '@/components/Toast';
import FloatingBallPetSettings from '@/components/FloatingBallPetSettings';
import { normalizeTeamOfficeSkin, TEAM_OFFICE_SKIN_IDS, type TeamOfficeSkinId } from '@/config/types';
import { useConfig } from '@/hooks/useConfig';
import { useResolvedTheme } from '@/theme';

import { OfficeRenderer, layoutOffice, type OfficeSnapshot } from './pixel/office';
import { OFFICE_SKINS } from './pixel/skins';

/** One department: the assistant at the head desk and three employees at work. */
const PREVIEW_SNAPSHOT: OfficeSnapshot = {
  at: '',
  departments: [{ id: 'preview', name: '', icon: '' }],
  people: [
    { agentId: 'lead', departmentId: 'preview', name: '', title: '', model: '', eye: 'claude', isLead: true, anim: 'working', pendingCards: 0 },
    { agentId: 'a', departmentId: 'preview', name: '', title: '', model: '', eye: 'codex', isLead: false, anim: 'working', pendingCards: 0 },
    { agentId: 'b', departmentId: 'preview', name: '', title: '', model: '', eye: 'deepseek', isLead: false, anim: 'thinking', pendingCards: 0 },
    { agentId: 'c', departmentId: 'preview', name: '', title: '', model: '', eye: 'gemini', isLead: false, anim: 'reading', pendingCards: 0 },
  ],
};
const PREVIEW_LAYOUT = layoutOffice(PREVIEW_SNAPSHOT);

const SkinPreview = memo(function SkinPreview({ skin, dark }: { skin: TeamOfficeSkinId; dark: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<OfficeRenderer | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || typeof canvas.getContext !== 'function') return;
    let renderer: OfficeRenderer;
    try {
      renderer = new OfficeRenderer(canvas);
    } catch {
      return; // No 2D context (test DOM).
    }
    renderer.setSnapshot(PREVIEW_SNAPSHOT);
    rendererRef.current = renderer;
    renderer.start();
    return () => {
      renderer.stop();
      rendererRef.current = null;
    };
  }, []);

  useEffect(() => {
    rendererRef.current?.setSkin(OFFICE_SKINS[skin], dark);
  }, [dark, skin]);

  return (
    <canvas
      ref={canvasRef}
      width={PREVIEW_LAYOUT.width}
      height={PREVIEW_LAYOUT.height}
      aria-hidden="true"
      className="block h-auto w-full"
      style={{ imageRendering: 'pixelated', aspectRatio: `${PREVIEW_LAYOUT.width} / ${PREVIEW_LAYOUT.height}` }}
    />
  );
});

export default function TeamAppearanceSettings({ showDesktopPet }: { showDesktopPet: boolean }) {
  const { t } = useTranslation('settings');
  const { t: tLauncher } = useTranslation('launcher');
  const { config, updateConfig } = useConfig();
  const toast = useToast();
  const dark = useResolvedTheme().resolvedColorScheme === 'dark';
  const current = normalizeTeamOfficeSkin(config.teamOfficeSkin);
  const [saving, setSaving] = useState<TeamOfficeSkinId | null>(null);

  const choose = async (skin: TeamOfficeSkinId) => {
    if (skin === current || saving) return;
    setSaving(skin);
    try {
      await updateConfig({ teamOfficeSkin: skin });
    } catch (error) {
      toast.error(t('teamAppearance.saveFailed', { message: error instanceof Error ? error.message : String(error) }));
    } finally {
      setSaving(null);
    }
  };

  return (
    <div className="mx-auto max-w-4xl px-4 py-8 sm:px-8" data-team-appearance>
      <div className="mb-8">
        <h2 className="text-xl font-semibold text-[var(--ink)]">{t('teamAppearance.title')}</h2>
        <p className="mt-2 max-w-2xl text-sm leading-6 text-[var(--ink-muted)]">{t('teamAppearance.description')}</p>
      </div>

      <section className="rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-4 sm:p-5">
        <h3 className="text-base font-medium text-[var(--ink)]">{t('teamAppearance.skinTitle')}</h3>
        <p className="mt-1 text-sm text-[var(--ink-muted)]">{t('teamAppearance.skinDescription')}</p>
        <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-3" role="radiogroup" aria-label={t('teamAppearance.skinTitle')}>
          {TEAM_OFFICE_SKIN_IDS.map((skin) => {
            const selected = skin === current;
            return (
              <button
                key={skin}
                type="button"
                role="radio"
                aria-checked={selected}
                disabled={saving !== null}
                onClick={() => void choose(skin)}
                className={`overflow-hidden rounded-lg border text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] ${
                  selected
                    ? 'border-[var(--accent)] ring-1 ring-[var(--accent)]'
                    : 'border-[var(--line)] hover:border-[var(--line-strong)]'
                }`}
                data-team-skin={skin}
              >
                <SkinPreview skin={skin} dark={dark} />
                <span className="block border-t border-[var(--line)] px-3 py-2">
                  <span className="block text-sm font-medium text-[var(--ink)]">{tLauncher(`teamOffice.skins.${skin}.name`)}</span>
                  <span className="mt-0.5 block text-xs leading-relaxed text-[var(--ink-muted)]">
                    {tLauncher(`teamOffice.skins.${skin}.description`)}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </section>

      {showDesktopPet && (
        <div className="mt-10" data-team-appearance-pet>
          <FloatingBallPetSettings embedded />
        </div>
      )}
    </div>
  );
}
