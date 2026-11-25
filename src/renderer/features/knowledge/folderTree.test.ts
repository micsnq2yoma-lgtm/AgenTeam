import { describe, expect, it } from 'vitest';

import {
  buildVirtualFolderTree,
  filterSourcesByFolder,
  sourceCategory,
  sourceDisplayTitle,
  sourceTags,
} from './folderTree';
import type { KnowledgeSourceView } from './types';

const makeSource = (
  id: string,
  over: Partial<KnowledgeSourceView> = {},
): KnowledgeSourceView => ({
  id,
  displayName: `${id}.md`,
  kind: 'file',
  status: 'ready',
  metadata: {},
  curationPending: false,
  searchStatus: 'indexed',
  ...over,
});

describe('folderTree', () => {
  it('builds nested category folders, rolls up counts, and tracks uncategorized items', () => {
    const sources = [
      makeSource('src_1', {
        metadata: { category: { value: 'Projects/Q4', origin: 'user', updatedAt: '2026-11-24T00:00:00Z' } },
      }),
      makeSource('src_2', {
        metadata: { category: { value: 'Projects/Q4/Specs', origin: 'agent', updatedAt: '2026-11-24T00:00:00Z' } },
      }),
      makeSource('src_3', {
        metadata: { category: { value: 'Reference', origin: 'agent', updatedAt: '2026-11-24T00:00:00Z' } },
      }),
      makeSource('src_4'),
    ];

    const tree = buildVirtualFolderTree(['Projects', 'Reference', 'Meetings'], sources);
    expect(tree.totalCount).toBe(4);
    expect(tree.uncategorizedCount).toBe(1);
    expect(tree.roots.map((n) => `${n.name}:${n.count}`)).toEqual([
      'Meetings:0',
      'Projects:2',
      'Reference:1',
    ]);
    const projects = tree.roots.find((n) => n.name === 'Projects')!;
    expect(projects.children[0].path).toBe('Projects/Q4');
    expect(projects.children[0].count).toBe(2);
    expect(projects.children[0].children[0].path).toBe('Projects/Q4/Specs');
    expect(projects.children[0].children[0].count).toBe(1);
  });

  it('filters sources by all, uncategorized, and nested category prefix', () => {
    const sources = [
      makeSource('src_1', {
        metadata: { category: { value: 'Projects/Q4', origin: 'user', updatedAt: '2026-11-24T00:00:00Z' } },
      }),
      makeSource('src_2', {
        metadata: { category: { value: 'Projects2', origin: 'agent', updatedAt: '2026-11-24T00:00:00Z' } },
      }),
      makeSource('src_3'),
    ];
    expect(filterSourcesByFolder(sources, { kind: 'all' })).toHaveLength(3);
    expect(filterSourcesByFolder(sources, { kind: 'uncategorized' }).map((s) => s.id)).toEqual(['src_3']);
    expect(filterSourcesByFolder(sources, { kind: 'category', path: 'Projects' }).map((s) => s.id)).toEqual(['src_1']);
  });

  it('extracts category, tags, and display title fallbacks safely', () => {
    const s = makeSource('src_a', {
      displayName: undefined,
      currentPath: 'C:\\docs\\report.pdf',
      metadata: {
        category: { value: '  Reports ', origin: 'agent', updatedAt: '2026-11-24T00:00:00Z' },
        tags: { value: ['Q4', '', 42], origin: 'agent', updatedAt: '2026-11-24T00:00:00Z' },
      },
    });
    expect(sourceDisplayTitle(s)).toBe('report.pdf');
    expect(sourceCategory(s)).toBe('Reports');
    expect(sourceTags(s)).toEqual(['Q4']);
  });
});
