// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { PRESET_PROVIDERS } from '@/config/types';
import EmbeddingModelCard from './EmbeddingModelCard';

const mockFetchEmbeddingModels = vi.fn();
const mockTestEmbeddingConnection = vi.fn();

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) =>
      opts?.dimension !== undefined ? `${key}:${opts.dimension}` : key,
  }),
}));

vi.mock('@/components/Toast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  }),
}));

vi.mock('@/config/services/modelDiscoveryService', () => ({
  supportsModelDiscovery: () => true,
  fetchProviderEmbeddingModels: (...args: unknown[]) => mockFetchEmbeddingModels(...args),
}));

vi.mock('@/features/knowledge/knowledgeClient', () => ({
  knowledgeClient: {
    testEmbeddingConnection: (...args: unknown[]) => mockTestEmbeddingConnection(...args),
  },
}));

describe('EmbeddingModelCard (K21-a)', () => {
  const tokenDanceProvider = PRESET_PROVIDERS.find((p) => p.id === 'tokendance')!;
  const openAiProvider = PRESET_PROVIDERS.find((p) => p.id === 'openai-api') ?? PRESET_PROVIDERS[0];

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('defaults to TokenDance and auto-selects recommended Qwen V4 (1024d) when logged in', async () => {
    mockFetchEmbeddingModels.mockResolvedValue([
      { id: 'qwen-text-embedding-v4', displayName: 'Qwen: Text Embedding V4', category: 'embedding' },
      { id: 'qwen3.7-text-embedding', displayName: 'Qwen: Qwen3.7 Text Embedding', category: 'embedding' },
    ]);
    mockTestEmbeddingConnection.mockResolvedValue({
      ok: true,
      dimension: 1024,
      latencyMs: 120,
    });
    const onSave = vi.fn().mockResolvedValue(undefined);

    render(
      <EmbeddingModelCard
        providers={[tokenDanceProvider, openAiProvider]}
        apiKeys={{ tokendance: 'td-key-123' }}
        onSaveEmbeddingModel={onSave}
      />,
    );

    await waitFor(() => {
      expect(mockFetchEmbeddingModels).toHaveBeenCalledTimes(1);
    });
    expect(screen.queryByTestId('embedding-tokendance-logged-out-hint')).toBeNull();

    const testBtn = screen.getByRole('button', { name: 'providers.embedding.testConnection' });
    await waitFor(() => {
      expect((testBtn as HTMLButtonElement).disabled).toBe(false);
    });
    fireEvent.click(testBtn);

    await waitFor(() => {
      expect(mockTestEmbeddingConnection).toHaveBeenCalledWith(
        'tokendance',
        'qwen-text-embedding-v4',
        1024,
      );
    });
    expect(screen.getByTestId('embedding-test-result').textContent).toContain(
      'providers.embedding.testOk:1024',
    );
  });

  it('shows non-error login hint when TokenDance is logged out and handles discovery failure + 402 error', async () => {
    mockFetchEmbeddingModels.mockRejectedValue(new Error('network error'));
    mockTestEmbeddingConnection.mockResolvedValue({
      ok: false,
      latencyMs: 400,
      errorKind: 'insufficient_balance',
      statusCode: 402,
    });
    const onSave = vi.fn().mockResolvedValue(undefined);

    render(
      <EmbeddingModelCard
        providers={[tokenDanceProvider, openAiProvider]}
        apiKeys={{}}
        embeddingModel={{
          enabled: true,
          providerId: 'tokendance',
          model: 'qwen-text-embedding-v4',
          dimensions: 1024,
        }}
        onSaveEmbeddingModel={onSave}
      />,
    );

    expect(screen.getByTestId('embedding-tokendance-logged-out-hint')).toBeTruthy();
    await waitFor(() => {
      expect(screen.getByTestId('embedding-discovery-error')).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: 'providers.embedding.testConnection' }));
    await waitFor(() => {
      expect(screen.getByTestId('embedding-test-result').textContent).toContain(
        'providers.embedding.errors.insufficient_balance',
      );
    });
  });
});
