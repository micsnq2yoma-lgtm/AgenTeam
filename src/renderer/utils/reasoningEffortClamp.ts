import { reasoningEffortChoices } from '../../shared/reasoningEffort';

/**
 * Should the Chat page reset a selected 推理强度 to 'default' because the active provider's protocol
 * cannot carry it? (An OpenAI-only level left selected after switching to an Anthropic-protocol
 * provider would show as active while the wire silently sends the default.)
 *
 * Managed Codex is NOT protocol-bound: its choices come from the Codex model catalog (`ultra` and
 * other future tiers exist there), and the composer keeps builtin chrome, so it reaches this check
 * with `inputUsesExternalRuntimeControls === false`. Judging it against the OpenAI/SDK vocabulary
 * silently reset every catalog-only level (e.g. `ultra`) straight back to 'default'. Its model-change
 * path already clamps against the catalog (`reasoningEffortAfterModelChange`).
 */
export function shouldResetEffortForProtocol(args: {
  reasoningEffort: string;
  inputUsesExternalRuntimeControls: boolean;
  managedProviderRuntimeActive: boolean;
  runtime: string;
  apiProtocol: 'anthropic' | 'openai' | undefined;
  providerId: string | undefined;
  model: string | undefined;
  baseUrl: string | undefined;
}): boolean {
  if (args.reasoningEffort === 'default') return false;
  if (args.inputUsesExternalRuntimeControls || args.managedProviderRuntimeActive) return false;
  const choices = reasoningEffortChoices(
    args.runtime === 'dsh' ? 'dsh' : 'builtin',
    args.apiProtocol,
    args.providerId,
    args.model,
    args.baseUrl,
  );
  return (!!choices && !choices.includes(args.reasoningEffort))
    || (args.runtime === 'dsh' && choices === null);
}
