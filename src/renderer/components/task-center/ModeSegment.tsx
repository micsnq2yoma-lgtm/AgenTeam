// ModeSegment — the Launcher's page-level 对话 / 团队 switch.
//   • 对话 → the composer that starts a new employee (conversation).
//   • 团队 → the pixel team office (features/team-office), which replaced the
//            former 记录 (Records) mode on the Launcher.
// A macOS-settings-style icon segmented control: one affordance with two
// mutually exclusive modes.

import { CapabilitiesIcon, TeamIcon } from '@/components/icons';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { retainFocusOnMouseDown } from '@/utils/focusRetention';

export type InputMode = 'chat' | 'team';

interface ModeSegmentProps {
  value: InputMode;
  onChange: (mode: InputMode) => void;
  disabled?: boolean;
  /** Optional slot on the right side (e.g. info tooltip). */
  suffix?: ReactNode;
  /**
   * When true, each button surfaces a `title` tooltip hinting that Tab
   * toggles the segment. Used on the Launcher where BrandSection binds
   * a page-level Tab handler; omit on surfaces without that binding so
   * we don't advertise a shortcut that doesn't work there.
   */
  tabSwitchHint?: boolean;
}

export function ModeSegment({
  value,
  onChange,
  suffix,
  tabSwitchHint = false,
  disabled = false,
}: ModeSegmentProps) {
  const { t } = useTranslation('chat');
  const chatTitle = tabSwitchHint ? t('input.mode.switchToTeam') : undefined;
  const teamTitle = tabSwitchHint ? t('input.mode.switchToChat') : undefined;

  // Segment button — the `active` state gets a raised paper-elevated
  // background (so the whole row reads as a track with a sliding
  // thumb), `shadow-xs` for the subtle macOS-style lift, and full ink
  // for the label. Inactive state stays ink-muted and relies on a soft
  // hover → ink-secondary step for feedback.
  // v0.1.69 polish: sized down (px-3.5 → px-3,（v2.5 字阶合并后文字回到 ui 档 14px）
  // icons 3.5 → 3) so the toggle reads as an affordance above the
  // input rather than a second-tier headline competing with the brand
  // group above it.
  const baseBtn =
    'inline-flex items-center gap-1.5 rounded-[var(--radius-sm)] px-3 py-1 text-sm font-medium transition-all duration-150';
  const activeBtn = 'bg-[var(--paper-elevated)] text-[var(--ink)] shadow-xs';
  const inactiveBtn =
    'text-[var(--ink-muted)] hover:text-[var(--ink-secondary)]';

  return (
    // `retainFocusOnMouseDown` on each segment button prevents the click
    // from transferring focus to the button itself, so any textarea the
    // user was typing in stays focused. This ALSO avoids a macOS WebKit
    // touchpad-tap bug where a subsequent `rAF(() => input.focus())`
    // would race the click-synthesis window and drop the click — see
    // `utils/focusRetention.ts` for the full write-up. Pit-of-success:
    // every future tab/toggle-style button should paste the same
    // `onMouseDown={retainFocusOnMouseDown}` so the reader immediately
    // recognises the intent and we can grep all call sites.
    <div className="inline-flex items-center">
      <div className="inline-flex gap-0.5 rounded-[var(--radius-md)] bg-[var(--paper-inset)] p-[3px]">
        <button
          type="button"
          disabled={disabled}
          onClick={() => onChange('chat')}
          onMouseDown={retainFocusOnMouseDown}
          aria-pressed={value === 'chat'}
          title={chatTitle}
          className={`${baseBtn} disabled:cursor-wait disabled:opacity-50 ${value === 'chat' ? activeBtn : inactiveBtn}`}
        >
          <CapabilitiesIcon className="h-3 w-3" strokeWidth={1.75} />
          {t('input.mode.chat')}
        </button>
        <button
          type="button"
          disabled={disabled}
          onClick={() => onChange('team')}
          onMouseDown={retainFocusOnMouseDown}
          aria-pressed={value === 'team'}
          title={teamTitle}
          className={`${baseBtn} disabled:cursor-wait disabled:opacity-50 ${value === 'team' ? activeBtn : inactiveBtn}`}
        >
          <TeamIcon className="h-3 w-3" strokeWidth={1.75} />
          {t('input.mode.team')}
        </button>
      </div>
      {suffix && <span className="ml-2 flex items-center">{suffix}</span>}
    </div>
  );
}

export default ModeSegment;
