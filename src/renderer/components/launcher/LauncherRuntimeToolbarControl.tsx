// LauncherRuntimeToolbarControl — the Launcher's runtime picker in the
// composer toolbar, next to the Tools menu:
//
//   +  完全自主 ^  工具  环境 [icon] Claude Agent SDK          Model ^  ➤
//
// The runtime decides which tools, permissions and models apply to the
// message, so it sits with those controls instead of in the context header
// (which now only names the destination project). The label uses the
// toolbar's `toolbar-label` class, so it collapses with the other toolbar
// labels when the composer is narrow and the icon + name remain.

import { memo } from 'react';
import { useTranslation } from 'react-i18next';

import RuntimeSelector from '@/components/RuntimeSelector';
import type { RuntimeDetections, RuntimeType } from '../../../shared/types/runtime';

interface LauncherRuntimeToolbarControlProps {
  runtime: RuntimeType;
  runtimeDetections: RuntimeDetections;
  onRuntimeChange: (runtime: RuntimeType) => void;
}

export default memo(function LauncherRuntimeToolbarControl({
  runtime,
  runtimeDetections,
  onRuntimeChange,
}: LauncherRuntimeToolbarControlProps) {
  const { t } = useTranslation('launcher');
  const label = t('runtimeSelector.label');

  return (
    <div
      role="group"
      aria-label={label}
      data-testid="launcher-runtime-toolbar"
      className="flex min-w-0 items-center pl-1"
    >
      <span className="toolbar-label shrink-0 px-1 text-sm font-medium text-[var(--ink-muted)]">
        {label}
      </span>
      <RuntimeSelector
        value={runtime}
        detections={runtimeDetections}
        onChange={onRuntimeChange}
        variant="launcher"
      />
    </div>
  );
});
