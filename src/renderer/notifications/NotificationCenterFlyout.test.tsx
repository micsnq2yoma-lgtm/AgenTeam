import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  copy: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@/utils/clipboard', () => ({ copyPlainText: mocks.copy }));

import { i18n } from '@/i18n';
import NotificationCenterFlyout, { formatNotificationTime } from './NotificationCenterFlyout';
import type { NotificationSnapshot } from './notificationCenter';

const taskComment = {
  id: 'task-comment:comment-9',
  kind: 'task_agent_comment' as const,
  createdAt: '2026-08-16T08:03:00.000Z',
  isRead: false,
  taskId: 'task-3',
  taskName: '升级依赖',
  commentId: 'comment-9',
  agent: { type: 'agent' as const, id: 'session-3', displayName: 'Agent' },
  excerpt: '发现两个高危依赖，建议先升级解析器。',
  target: {
    kind: 'app_route' as const,
    route: { version: 1 as const, name: 'task.comment' as const, params: { taskId: 'task-3', commentId: 'comment-9' } },
  },
};

function snapshot(overrides: Partial<NotificationSnapshot> = {}): NotificationSnapshot {
  return {
    loadState: 'ready',
    items: [taskComment],
    hasUnread: true,
    hasMore: false,
    isLoadingMore: false,
    feedCutoff: { createdAt: taskComment.createdAt, id: taskComment.id },
    errorCode: null,
    ...overrides,
  };
}

function renderFlyout(overrides: Partial<Parameters<typeof NotificationCenterFlyout>[0]> = {}) {
  const props = {
    snapshot: snapshot(),
    onRefresh: vi.fn(),
    onLoadMore: vi.fn(async () => undefined),
    onMarkAllRead: vi.fn(async () => undefined),
    onOpenAppRoute: vi.fn(async () => true),
    onClose: vi.fn(),
    ...overrides,
  };
  return { ...render(<NotificationCenterFlyout {...props} />), props };
}

describe('NotificationCenterFlyout', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage('zh-CN');
  });





  it('formats local task timestamps using the selected locale', () => {
    const now = Date.parse('2026-08-16T08:03:00.000Z');
    expect(formatNotificationTime('invalid', 'en-US', now)).toBe('');
    expect(formatNotificationTime('2026-08-16T08:01:00.000Z', 'en-US', now)).toBe('2 minutes ago');
  });

  it('renders a local Agent task comment and opens its exact typed route', async () => {
    mocks.invoke.mockResolvedValueOnce({ notificationId: taskComment.id, target: taskComment.target });
    const onOpenAppRoute = vi.fn(async () => true);
    renderFlyout({
      snapshot: snapshot({ items: [taskComment], feedCutoff: { createdAt: taskComment.createdAt, id: taskComment.id } }),
      onOpenAppRoute,
    });

    expect(screen.queryByText(String(i18n.t('app:notificationCenter.accountScope')))).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Agent.*升级依赖/ }));
    await waitFor(() => expect(onOpenAppRoute).toHaveBeenCalledWith(taskComment.target.route));
    expect(screen.getByText(taskComment.excerpt)).toBeInTheDocument();
  });

  it('uses text tone alone for read state and clamps comment excerpts to three lines', () => {
    const readTaskComment = { ...taskComment, id: 'task-comment:read', isRead: true };
    renderFlyout({
      snapshot: snapshot({
        items: [taskComment, readTaskComment],
        feedCutoff: { createdAt: readTaskComment.createdAt, id: readTaskComment.id },
      }),
    });

    const unreadItem = screen.getByLabelText(String(i18n.t('app:notificationCenter.unread')));
    const readItem = screen.getByLabelText(String(i18n.t('app:notificationCenter.read')));
    expect(unreadItem.querySelector('[data-notification-headline]')).toHaveClass(
      'font-medium',
      'text-[var(--ink)]',
    );
    expect(readItem.querySelector('[data-notification-headline]')).toHaveClass(
      'font-normal',
      'text-[var(--ink-muted)]',
    );
    const unreadDetail = unreadItem.querySelector('[data-notification-detail]');
    const readDetail = readItem.querySelector('[data-notification-detail]');
    expect(unreadDetail).toHaveClass(
      'line-clamp-3',
      'text-[var(--ink-secondary)]',
    );
    expect(unreadDetail).not.toHaveClass('block');
    expect(readDetail).toHaveClass(
      'line-clamp-3',
      'text-[var(--ink-muted)]/75',
    );
    expect(readDetail).not.toHaveClass('block');
    expect(unreadItem.querySelector('[aria-hidden="true"]')).not.toBeInTheDocument();
    expect(unreadItem.querySelector('svg')).not.toBeInTheDocument();
  });

  it('marks all read against the loaded feed cutoff without activating a row', async () => {
    const onMarkAllRead = vi.fn(async () => undefined);
    renderFlyout({ onMarkAllRead });

    fireEvent.click(screen.getByRole('button', { name: String(i18n.t('app:notificationCenter.markAllRead')) }));
    await waitFor(() => expect(onMarkAllRead).toHaveBeenCalledOnce());
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
