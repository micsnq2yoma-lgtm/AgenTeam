import routeFixtures from './appRoute.fixtures.json';
import { describe, expect, it } from 'vitest';

import {
  createTaskCommentAppRoute,
  parseAppRouteUrl,
  serializeAppRoute,
} from './appRoute';

describe('AppRoute', () => {
  it('matches the shared native route corpus', () => {
    for (const { url, route } of routeFixtures.accepted) {
      expect(parseAppRouteUrl(url), url).toEqual(route);
      expect(parseAppRouteUrl(serializeAppRoute(route as Parameters<typeof serializeAppRoute>[0]))).toEqual(route);
    }
    for (const url of routeFixtures.rejected) expect(parseAppRouteUrl(url), url).toBeNull();
  });

  it('round-trips supported v1 routes', () => {

    const commentRoute = createTaskCommentAppRoute('task_1', 'comment-2');
    expect(serializeAppRoute(commentRoute)).toBe(
      'agenteam://open/v1/tasks/task_1/comments/comment-2',
    );
    expect(parseAppRouteUrl(serializeAppRoute(commentRoute))).toEqual(commentRoute);
  });

  it.each([
    'agenteam://attachment/session/file.png',
    'agenteam://tool-attachment/session/turn/file.png',
    'myagents-resource://attachment/session/file.png',
    'agenteam://evil/v1/spaces/a/issues/b',
    'agenteam://open/v2/spaces/a/issues/b',
    'agenteam://open/v1/spaces/a/issues/b/extra',
    'agenteam://open/v1/tasks/a/comments/b/extra',
    'agenteam://open/v1/tasks/a/issues/b',
    'agenteam://open/v1/spaces/a/issues/b?prompt=run',
    'agenteam://open/v1/spaces/a/issues/b#fragment',
    'agenteam://open/v1/spaces/a%2Fb/issues/c',
    'agenteam://open/v1/spaces/a/issues/%ZZ',
    'agenteam://user@open/v1/spaces/a/issues/b',
    'agenteam://open:42/v1/spaces/a/issues/b',
  ])('rejects unsupported or ambiguous input: %s', (value) => {
    expect(parseAppRouteUrl(value)).toBeNull();
  });

  it('bounds identifiers before serialization', () => {
    expect(() => createTaskCommentAppRoute('task', '')).toThrow();
  });
});
