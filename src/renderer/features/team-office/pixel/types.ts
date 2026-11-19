// Types and model-family detection for the pixel office, ported from
// AgenTeam packages/shared (MIT, see THIRD_PARTY_NOTICES.md).

export type OfficeAnim = 'failed' | 'waiting' | 'working' | 'delivering' | 'paused' | 'idle' | 'walk_to_lead';

/** Visual identity follows the selected model, including models behind a CLI/API adapter. */
export const MODEL_FAMILIES = ['claude', 'codex', 'deepseek', 'gemini', 'kimi', 'glm', 'qwen', 'minimax', 'grok', 'other'] as const;
export type ModelFamily = typeof MODEL_FAMILIES[number];

export function modelFamilyFor(adapterType?: string, model?: unknown): ModelFamily {
  const selected = typeof model === 'string' ? model.trim().toLowerCase() : undefined;
  if (selected && selected !== 'default' && selected !== 'auto') {
    if (/deepseek/.test(selected)) return 'deepseek';
    if (/claude|sonnet|opus|haiku/.test(selected)) return 'claude';
    if (/codex|gpt|(?:^|[/:-])o[134](?:$|[-.])/.test(selected)) return 'codex';
    if (/gemini/.test(selected)) return 'gemini';
    if (/kimi|moonshot/.test(selected)) return 'kimi';
    if (/glm|zhipu/.test(selected)) return 'glm';
    if (/qwen|tongyi/.test(selected)) return 'qwen';
    if (/minimax/.test(selected)) return 'minimax';
    if (/grok/.test(selected)) return 'grok';
    return 'other';
  }
  const adapter = adapterType?.toLowerCase() ?? '';
  return MODEL_FAMILIES.find((family) => family !== 'other' && adapter.includes(family)) ?? 'other';
}
