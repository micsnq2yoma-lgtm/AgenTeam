import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import ConfirmDialog from './ConfirmDialog';

describe('ConfirmDialog i18n', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('en-US');
  });

  it('uses translated default action labels', () => {
    render(
      <ConfirmDialog
        title="Delete item"
        message="This cannot be undone."
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Confirm' })).toBeInTheDocument();
  });

  it('offers an optional secondary action while Enter keeps the primary one', () => {
    const onConfirm = vi.fn();
    const onSecondary = vi.fn();
    render(
      <ConfirmDialog
        title="Switch provider"
        message="Choose how to continue."
        confirmText="Keep context"
        secondaryText="Start blank"
        onSecondary={onSecondary}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Start blank' }));
    expect(onSecondary).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
    fireEvent.keyDown(document, { key: 'Enter' });
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('renders no secondary button unless both text and handler are given', () => {
    render(
      <ConfirmDialog title="t" message="m" secondaryText="Start blank" onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(screen.queryByRole('button', { name: 'Start blank' })).not.toBeInTheDocument();
  });
});
