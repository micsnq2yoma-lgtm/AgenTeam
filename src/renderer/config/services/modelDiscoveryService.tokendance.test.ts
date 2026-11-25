import { describe, expect, it, vi } from 'vitest';
import { mergePresetCustomModels, PRESET_PROVIDERS } from '../types';
import { resolveProviderForModel } from '../../../shared/provider-model-routing';
import {
  classifyDiscoveredModelCategory,
  fetchProviderEmbeddingModels,
  fetchProviderModels,
  isTokenDanceConversationModel,
  parseModelsResponse,
  toModelEntity,
} from './modelDiscoveryService';
import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const provider = PRESET_PROVIDERS.find((p) => p.id === 'tokendance')!;
const go = PRESET_PROVIDERS.find((p) => p.id === 'opencode-go')!;
describe('Token Dance public model discovery', () => {
  it('preserves protocol metadata through discovery, user editing and disk JSON reload', () => {
    const discovered = parseModelsResponse({
      data: [
        {
          id: 'future-model',
          name: 'Future Model',
          context_length: 123456,
          supported_protocols: ['openai:chat-completions', 'openai:responses'],
        },
      ],
    });
    const model = {
      ...toModelEntity(discovered[0], provider),
      modelName: 'My model',
      source: 'manual' as const,
    };
    const config = JSON.parse(
      JSON.stringify({ presetCustomModels: { tokendance: [model] } }),
    );
    const restored = mergePresetCustomModels(
      [provider],
      config.presetCustomModels,
    )[0];
    expect(
      restored.models.find((m) => m.model === model.model)?.modelName,
    ).toBe('My model');
    expect(resolveProviderForModel(restored, model.model).upstreamFormat).toBe(
      'responses',
    );
  });

  it('fetches the public catalog without sending an API key', async () => {
    vi.mocked(invoke).mockResolvedValue({ data: [] });
    await fetchProviderModels(provider, 'should-not-be-sent');
    expect(invoke).toHaveBeenCalledWith(
      'cmd_fetch_provider_models',
      expect.objectContaining({
        url: 'https://tokendance.space/gateway/v1/models',
        authHeaderName: null,
        authHeaderValue: null,
      }),
    );
  });

  it('discovers embedding models separately without altering chat model discovery', async () => {
    vi.mocked(invoke).mockResolvedValue({
      data: [
        {
          id: 'deepseek-v4-pro-0813',
          name: 'DeepSeek V4 Pro',
          context_length: 1000000,
          supported_protocols: ['openai:chat-completions', 'openai:responses'],
        },
        {
          id: 'qwen-text-embedding-v4',
          name: 'Qwen: Text Embedding V4',
          context_length: 8000,
          supported_protocols: ['openai:embeddings'],
        },
        {
          id: 'qwen3.7-text-embedding',
          name: 'Qwen: Qwen3.7 Text Embedding',
          context_length: 128000,
          supported_protocols: ['openai:embeddings'],
        },
        {
          id: 'cosyvoice-tts-v2',
          name: 'TTS',
          supported_protocols: ['openai:audio'],
        },
      ],
    });

    const all = await fetchProviderModels(provider, undefined);
    const chatModels = all.filter(isTokenDanceConversationModel);
    expect(chatModels.map((m) => m.id)).toEqual(['deepseek-v4-pro-0813']);
    expect(classifyDiscoveredModelCategory(all[0])).toBe('chat');
    expect(classifyDiscoveredModelCategory(all[1])).toBe('embedding');
    expect(classifyDiscoveredModelCategory(all[3])).toBeUndefined();

    const embeddingModels = await fetchProviderEmbeddingModels(provider, undefined);
    expect(embeddingModels.map((m) => ({ id: m.id, category: m.category }))).toEqual([
      { id: 'qwen-text-embedding-v4', category: 'embedding' },
      { id: 'qwen3.7-text-embedding', category: 'embedding' },
    ]);
  });
});

describe('OpenCode Go public model discovery', () => {
  it('parses the observed ID-only directory without inventing a protocol or sending a Key', async () => {
    const observed = { data: [{ id: 'future-go', object: 'model', created: 1790658758, owned_by: 'opencode' }] };
    vi.mocked(invoke).mockResolvedValue(observed);
    const models = await fetchProviderModels(go, undefined);
    expect(invoke).toHaveBeenCalledWith('cmd_fetch_provider_models', expect.objectContaining({
      url: 'https://opencode.ai/zen/go/v1/models',
      authHeaderName: null,
      authHeaderValue: null,
    }));
    expect(models).toEqual([{ id: 'future-go', displayName: undefined, ownedBy: 'opencode', contextLength: undefined,
      maxOutputTokens: undefined, supportsImage: undefined, supportsVideo: undefined,
      supportsReasoning: undefined, status: undefined, supportedProtocols: undefined }]);
    expect(toModelEntity(models[0], go).executionProtocol).toBeUndefined();
  });
});
