import type { RuntimeSource, RuntimeType } from '../../shared/types/runtime';

import antigravityIcon from '@/assets/runtime-icons/antigravity.png';
import brandMark from '@/assets/brand/agenteam-mark.svg';
import claudeCodeIcon from '@/assets/runtime-icons/claude-code.png';
import codexIcon from '@/assets/runtime-icons/codex.png';
import dshIcon from '@/assets/runtime-icons/deepseek-harness.png';

interface RuntimePresentation {
  name: string;
  /** Compact label for inline surfaces (e.g. the Launcher context header),
   *  where the product prefix is already implied by the brand mark icon. */
  shortName?: string;
  icon: string;
  engineIcon?: string;
}

export const RUNTIME_PRESENTATION: Record<RuntimeType, RuntimePresentation> = {
  builtin: { name: 'AgenTeam (Claude Agent SDK)', shortName: 'Claude Agent SDK', icon: brandMark, engineIcon: claudeCodeIcon },
  dsh: { name: 'AgenTeam (DeepSeek Harness)', shortName: 'DeepSeek Harness', icon: brandMark, engineIcon: dshIcon },
  'claude-code': { name: 'Claude Code CLI', icon: claudeCodeIcon },
  antigravity: { name: 'Antigravity CLI', icon: antigravityIcon },
  codex: { name: 'Codex CLI', icon: codexIcon },
};

/** Managed Codex and the user's Codex CLI share a runtime type, but not an owner. */
export function sessionRuntimePresentation(
  runtime: RuntimeType,
  source?: RuntimeSource | null,
): RuntimePresentation {
  if (runtime === 'codex' && source === 'managed-provider') {
    return { ...RUNTIME_PRESENTATION.codex, name: 'Managed Codex' };
  }
  return RUNTIME_PRESENTATION[runtime];
}
