import { memo } from 'react';
import { useTranslation } from 'react-i18next';

import { BookIcon, MessageIcon } from '@/components/icons';

export type LauncherMode = 'chat' | 'knowledge';

/** Segmented switch at the top of the launcher: 对话 ⇄ 知识库. */
export default memo(function LauncherModeSwitch({
  mode,
  onChange,
}: {
  mode: LauncherMode;
  onChange: (mode: LauncherMode) => void;
}) {
  const { t } = useTranslation('launcher');
  const items: { id: LauncherMode; label: string; icon: React.ReactNode }[] = [
    { id: 'chat', label: t('modeSwitch.chat'), icon: <MessageIcon className="h-3.5 w-3.5" /> },
    { id: 'knowledge', label: t('modeSwitch.knowledge'), icon: <BookIcon className="h-3.5 w-3.5" /> },
  ];
  return (
    <div className="flex shrink-0 justify-center px-4 pt-4" data-testid="launcher-mode-switch">
      <div
        role="tablist"
        aria-label={t('modeSwitch.label')}
        className="inline-flex items-center gap-0.5 rounded-xl bg-[var(--paper-inset)] p-1"
      >
        {items.map((item) => {
          const selected = item.id === mode;
          return (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={selected}
              onClick={() => { if (!selected) onChange(item.id); }}
              className={`flex items-center gap-1.5 rounded-lg px-4 py-1.5 text-sm font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] ${
                selected
                  ? 'bg-[var(--paper-elevated)] text-[var(--ink)] shadow-sm'
                  : 'text-[var(--ink-muted)] hover:text-[var(--ink)]'
              }`}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
});
