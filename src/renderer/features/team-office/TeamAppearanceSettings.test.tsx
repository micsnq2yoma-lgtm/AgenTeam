import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';

const mocks = vi.hoisted(() => ({
  skin: undefined as string | undefined,
  updateConfig: vi.fn(async () => {}),
  toastError: vi.fn(),
}));

vi.mock('@/hooks/useConfig', () => ({
  useConfig: () => ({ config: { teamOfficeSkin: mocks.skin }, updateConfig: mocks.updateConfig }),
}));
vi.mock('@/components/Toast', () => ({ useToast: () => ({ error: mocks.toastError }) }));
vi.mock('@/theme', () => ({ useResolvedTheme: () => ({ resolvedColorScheme: 'light' }) }));
vi.mock('@/components/FloatingBallPetSettings', () => ({
  default: ({ embedded }: { embedded?: boolean }) => <div data-testid="pet-settings" data-embedded={String(embedded)} />,
}));

import TeamAppearanceSettings from './TeamAppearanceSettings';

const skinName = (id: string) => String(i18n.t(`launcher:teamOffice.skins.${id}.name`));

beforeEach(async () => {
  await i18n.changeLanguage('zh-CN');
  mocks.skin = undefined;
  mocks.updateConfig.mockClear();
  mocks.toastError.mockClear();
});

describe('TeamAppearanceSettings', () => {
  it('lists every skin, marks the current one and saves a new choice through config', async () => {
    render(<TeamAppearanceSettings showDesktopPet={false} />);

    const radios = screen.getAllByRole('radio');
    expect(radios).toHaveLength(3);
    expect(screen.getByRole('radio', { name: new RegExp(skinName('office')) })).toHaveAttribute('aria-checked', 'true');

    fireEvent.click(screen.getByRole('radio', { name: new RegExp(skinName('court')) }));
    await waitFor(() => expect(mocks.updateConfig).toHaveBeenCalledWith({ teamOfficeSkin: 'court' }));

    fireEvent.click(screen.getByRole('radio', { name: new RegExp(skinName('office')) }));
    expect(mocks.updateConfig).toHaveBeenCalledTimes(1); // already selected
  });

  it('reports a failed save', async () => {
    mocks.updateConfig.mockRejectedValueOnce(new Error('locked'));
    render(<TeamAppearanceSettings showDesktopPet={false} />);
    fireEvent.click(screen.getByRole('radio', { name: new RegExp(skinName('starship')) }));
    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith(expect.stringContaining('locked')));
  });

  it('hosts the desktop pet settings below the skins only when the pet is available', () => {
    const { rerender } = render(<TeamAppearanceSettings showDesktopPet={false} />);
    expect(screen.queryByTestId('pet-settings')).not.toBeInTheDocument();

    rerender(<TeamAppearanceSettings showDesktopPet />);
    expect(screen.getByTestId('pet-settings')).toHaveAttribute('data-embedded', 'true');
  });
});
