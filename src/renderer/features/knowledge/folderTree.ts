import type { KnowledgeSourceView } from './types';

export interface VirtualFolderNode {
  /** Full `/` separated category path (e.g. `Projects/Q4`). */
  path: string;
  /** Leaf display name (e.g. `Q4`). */
  name: string;
  /** Number of sources directly or nested under this category path. */
  count: number;
  /** True when the category is present in `ORGANIZE_RULES.md`. */
  approved: boolean;
  children: VirtualFolderNode[];
}

export type FolderSelection =
  | { kind: 'all' }
  | { kind: 'uncategorized' }
  | { kind: 'category'; path: string };

export function sourceCategory(source: KnowledgeSourceView): string | null {
  const raw = source.metadata.category?.value;
  if (typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function sourceDisplayTitle(source: KnowledgeSourceView): string {
  if (source.displayName && source.displayName.trim().length > 0) {
    return source.displayName;
  }
  if (source.currentPath) {
    const parts = source.currentPath.split(/[\\/]/);
    const tail = parts[parts.length - 1];
    if (tail) return tail;
  }
  return source.id;
}

export function sourceTags(source: KnowledgeSourceView): string[] {
  const raw = source.metadata.tags?.value;
  if (!Array.isArray(raw)) return [];
  return raw.filter((tag): tag is string => typeof tag === 'string' && tag.trim().length > 0);
}

/**
 * Build a hierarchical virtual folder tree from approved categories plus any
 * categories currently assigned on sources. Wiki sources are browsed in the
 * Wiki tab, so callers typically pass non-wiki sources here.
 */
export function buildVirtualFolderTree(
  approvedCategories: readonly string[],
  sources: readonly KnowledgeSourceView[],
): {
  roots: VirtualFolderNode[];
  uncategorizedCount: number;
  totalCount: number;
} {
  const approvedSet = new Set(approvedCategories.map((c) => c.trim()).filter(Boolean));
  const allPaths = new Set<string>(approvedSet);
  const directCounts = new Map<string, number>();
  let uncategorizedCount = 0;

  for (const source of sources) {
    const cat = sourceCategory(source);
    if (!cat) {
      uncategorizedCount += 1;
      continue;
    }
    allPaths.add(cat);
    directCounts.set(cat, (directCounts.get(cat) ?? 0) + 1);
  }

  // Ensure every intermediate prefix segment exists as a node.
  const nodeMap = new Map<string, VirtualFolderNode>();
  for (const fullPath of allPaths) {
    const segments = fullPath.split('/').filter(Boolean);
    let prefix = '';
    for (const seg of segments) {
      prefix = prefix ? `${prefix}/${seg}` : seg;
      if (!nodeMap.has(prefix)) {
        nodeMap.set(prefix, {
          path: prefix,
          name: seg,
          count: 0,
          approved: approvedSet.has(prefix),
          children: [],
        });
      }
    }
  }

  // Accumulate counts up to every ancestor folder.
  for (const [cat, count] of directCounts) {
    const segments = cat.split('/').filter(Boolean);
    let prefix = '';
    for (const seg of segments) {
      prefix = prefix ? `${prefix}/${seg}` : seg;
      const node = nodeMap.get(prefix);
      if (node) node.count += count;
    }
  }

  const roots: VirtualFolderNode[] = [];
  for (const node of nodeMap.values()) {
    const slash = node.path.lastIndexOf('/');
    if (slash === -1) {
      roots.push(node);
    } else {
      const parent = nodeMap.get(node.path.slice(0, slash));
      if (parent) parent.children.push(node);
      else roots.push(node);
    }
  }

  const sortNodes = (list: VirtualFolderNode[]) => {
    list.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    for (const child of list) sortNodes(child.children);
  };
  sortNodes(roots);

  return { roots, uncategorizedCount, totalCount: sources.length };
}

export function filterSourcesByFolder(
  sources: readonly KnowledgeSourceView[],
  selection: FolderSelection,
): KnowledgeSourceView[] {
  if (selection.kind === 'all') return [...sources];
  if (selection.kind === 'uncategorized') {
    return sources.filter((source) => sourceCategory(source) === null);
  }
  const prefix = `${selection.path}/`;
  return sources.filter((source) => {
    const cat = sourceCategory(source);
    return cat === selection.path || (cat !== null && cat.startsWith(prefix));
  });
}
