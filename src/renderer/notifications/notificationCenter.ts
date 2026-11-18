import { invoke } from '@tauri-apps/api/core';

import type { AppRoute } from '../../shared/appRoute';

export type NotificationTarget =
  | { kind: 'app_route'; route: AppRoute };

export interface NotificationSortPoint {
  createdAt: string;
  id: string;
}

export interface TaskAgentCommentNotificationItem {
  id: string;
  kind: 'task_agent_comment';
  createdAt: string;
  isRead: boolean;
  taskId: string;
  taskName: string;
  commentId: string;
  agent: {
    type: 'agent';
    id: string;
    displayName: string;
  };
  excerpt: string;
  target: NotificationTarget;
}

export type NotificationItem = TaskAgentCommentNotificationItem;

export type NotificationLoadState = 'idle' | 'loading' | 'ready' | 'error' | 'unavailable';

export interface NotificationSnapshot {
  loadState: NotificationLoadState;
  items: NotificationItem[];
  hasUnread: boolean;
  hasMore: boolean;
  isLoadingMore: boolean;
  feedCutoff: NotificationSortPoint | null;
  errorCode: string | null;
}

export interface NotificationActivation {
  notificationId: string;
  target: NotificationTarget;
}

export const EMPTY_NOTIFICATION_SNAPSHOT: NotificationSnapshot = {
  loadState: 'idle',
  items: [],
  hasUnread: false,
  hasMore: false,
  isLoadingMore: false,
  feedCutoff: null,
  errorCode: null,
};

export function getNotificationSnapshot(): Promise<NotificationSnapshot> {
  return invoke<NotificationSnapshot>('cmd_notification_get_snapshot');
}

export function requestNotificationRefresh(): Promise<NotificationSnapshot> {
  return invoke<NotificationSnapshot>('cmd_notification_refresh');
}

export function loadMoreNotifications(): Promise<NotificationSnapshot> {
  return invoke<NotificationSnapshot>('cmd_notification_load_more');
}

export function markNotificationRead(notificationId: string): Promise<NotificationActivation> {
  return invoke<NotificationActivation>('cmd_notification_mark_read', { notificationId });
}

export function markAllNotificationsRead(): Promise<NotificationSnapshot> {
  return invoke<NotificationSnapshot>('cmd_notification_mark_all_read');
}
