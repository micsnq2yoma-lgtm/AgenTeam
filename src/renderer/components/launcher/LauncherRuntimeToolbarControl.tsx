// LauncherRuntimeToolbarControl — the Launcher's runtime picker in the
// composer toolbar, next to the Tools menu:
//
//   +  完全自主 ^  工具  [icon] Claude Agent SDK          Model ^  ➤
//
// The runtime decides which tools, permissions and models apply to the
// message, so it sits with those controls instead of in the context header
// (which now only names the destination project). The runtime icon and name
// identify the control on their own, so no visible label is shown; the group
// keeps an accessible name (环境 / Environment) for assistive technology.

import { memo } from 'react';
import { useTranslation } from 'react-i18next';

import RuntimeSelector from '@/components/RuntimeSelector';
import type { RuntimeDetections, RuntimeType } from '../../../shared/types/runtime';

interface LauncherRuntimeToolbarControlProps {
  runtime: RuntimeType;
  runtimeDetections: RuntimeDetections;
  runtimeDetecting?: boolean;
  runtimeDetectionFailed?: boolean;
  onRuntimeDetectAgain?: () => void;
  onRuntimeChange: (runtime: RuntimeType) => void;
}

export default memo(function LauncherRuntimeToolbarControl({
  runtime,
  runtimeDetections,
  runtimeDetecting,
  runtimeDetectionFailed,
  onRuntimeDetectAgain,
  onRuntimeChange,
}: LauncherRuntimeToolbarControlProps) {
  const { t } = useTranslation('launcher');
  const label = t('runtimeSelector.label');

  return (
    <div
      role="group"
      aria-label={label}
      data-testid="launcher-runtime-toolbar"
      className="flex min-w-0 items-center"
    >
      <RuntimeSelector
        value={runtime}
        detections={runtimeDetections}
        detecting={runtimeDetecting}
        detectionFailed={runtimeDetectionFailed}
        onDetectAgain={onRuntimeDetectAgain}
        onChange={onRuntimeChange}
        variant="launcher"
      />
    </div>
  );
});
