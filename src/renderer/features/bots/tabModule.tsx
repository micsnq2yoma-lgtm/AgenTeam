import { lazy, memo, Suspense } from 'react';

import type { TabModuleDefinition } from '@/tab-workspace/contracts';
import { PAGE_FALLBACK } from '@/tab-workspace/PageFallback';

import type { BotsTab } from './tabContract';

const BotsPage = lazy(() => import('./BotsPage'));

const Renderer = memo(function BotsTabRenderer({ isDeferred }: {
  tab: BotsTab;
  isActive: boolean;
  isDeferred: boolean;
  binding: null;
}) {
  if (isDeferred) return PAGE_FALLBACK;
  return (
    <Suspense fallback={PAGE_FALLBACK}>
      <BotsPage />
    </Suspense>
  );
});

export const botsTabModule = {
  kind: 'bots',
  render: Renderer,
  chrome: (_tab, { t }) => ({ title: t('tabs.bots'), subtitle: t('tabs.bots') }),
  identity: () => 'bots',
  open: {
    findExisting: (tabs) => tabs[0],
    create: (intent, { id }) => ({ id, view: 'bots', title: intent.title }),
  },
  initialMount: () => 'deferred-content',
} satisfies TabModuleDefinition<BotsTab, { title: string }, null>;
