import { invoke } from '@tauri-apps/api/core';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';

import { BookIcon } from '@/components/icons';
import { useToastOptional } from '@/components/Toast';
import { useFileAction } from '@/context/fileActionState';

import type { KnowledgeRef } from '../../../shared/knowledge';

export interface ResolvedKnowledgeChunk {
  chunkId: string;
  ordinal: number;
  headingPath: string[];
  page?: number | null;
  prefix: string;
  text: string;
  byteRange: [number, number];
  utf16Range: [number, number];
}

export type ResolvedKnowledgeFragment =
  | { status: 'available'; chunk: ResolvedKnowledgeChunk }
  | { status: 'unavailable' };

export interface ResolvedKnowledgePreview {
  path: string;
  name: string;
  isContentFallback: boolean;
  page?: number | null;
  utf16Range?: [number, number] | null;
  lineNumber?: number | null;
}

export interface ResolvedKnowledgeRefPayload {
  reference: KnowledgeRef;
  source: {
    id: string;
    displayName?: string | null;
    currentPath?: string | null;
    status: string;
  };
  fragment?: ResolvedKnowledgeFragment | null;
  excerpt?: string | null;
  preview?: ResolvedKnowledgePreview | null;
}

type CitationCacheEntry =
  | { state: 'resolved'; data: ResolvedKnowledgeRefPayload }
  | { state: 'unavailable' };

const MAX_CACHE_ENTRIES = 256;
const HOVER_DEBOUNCE_MS = 120;
const citationCache = new Map<string, CitationCacheEntry>();

export function clearKnowledgeCitationCache(): void {
  citationCache.clear();
}

function writeCitationCache(key: string, entry: CitationCacheEntry): void {
  if (citationCache.has(key)) {
    citationCache.delete(key);
  } else if (citationCache.size >= MAX_CACHE_ENTRIES) {
    const oldest = citationCache.keys().next().value;
    if (oldest !== undefined) citationCache.delete(oldest);
  }
  citationCache.set(key, entry);
}

function isNotFoundError(error: unknown): boolean {
  if (typeof error === 'string') return error.includes('KB_NOT_FOUND');
  if (error && typeof error === 'object') {
    const record = error as { code?: unknown; message?: unknown };
    if (record.code === 'KB_NOT_FOUND') return true;
    if (typeof record.message === 'string' && record.message.includes('KB_NOT_FOUND')) {
      return true;
    }
  }
  return false;
}

function extractSourceTitle(
  payload: ResolvedKnowledgeRefPayload,
  fallbackId: string,
): string {
  const fromPreview = payload.preview?.name?.trim();
  if (fromPreview) return fromPreview;
  const fromDisplay = payload.source.displayName?.trim();
  if (fromDisplay) return fromDisplay;
  const fromPath = payload.source.currentPath?.split(/[/\\]/).pop()?.trim();
  if (fromPath) return fromPath;
  return fallbackId;
}

function extractPlainChildrenText(children: ReactNode): string {
  if (typeof children === 'string') return children.trim();
  if (typeof children === 'number') return String(children);
  if (Array.isArray(children)) {
    return children.map(extractPlainChildrenText).join('').trim();
  }
  return '';
}

function buildExcerpt(payload: ResolvedKnowledgeRefPayload): string {
  if (payload.excerpt?.trim()) return payload.excerpt.trim();
  if (payload.fragment?.status === 'available' && payload.fragment.chunk.text.trim()) {
    const raw = payload.fragment.chunk.text.trim();
    const chars = Array.from(raw);
    return chars.length <= 300 ? raw : `${chars.slice(0, 300).join('').trimEnd()}…`;
  }
  return '';
}

interface PopoverCoords {
  top: number;
  left: number;
  placement: 'above' | 'below';
}

function computePopoverCoords(anchor: HTMLElement): PopoverCoords {
  const rect = anchor.getBoundingClientRect();
  const cardWidth = 320;
  const viewportWidth = window.innerWidth || 1024;
  const left = Math.max(12, Math.min(rect.left, viewportWidth - cardWidth - 12));
  const preferAbove = rect.top > 200;
  return preferAbove
    ? { top: rect.top - 8, left, placement: 'above' }
    : { top: rect.bottom + 8, left, placement: 'below' };
}

interface KnowledgeCitationChipProps {
  reference: string;
  parsed: KnowledgeRef;
  children?: ReactNode;
}

export default function KnowledgeCitationChip({
  reference,
  parsed,
  children,
}: KnowledgeCitationChipProps) {
  const { t } = useTranslation('app');
  const fileAction = useFileAction();
  const toast = useToastOptional();

  const fileActionRef = useRef(fileAction);
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    fileActionRef.current = fileAction;
    toastRef.current = toast;
  }, [fileAction, toast]);

  const [entry, setEntry] = useState<CitationCacheEntry | null>(
    () => citationCache.get(reference) ?? null,
  );
  const [hoverOpen, setHoverOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [coords, setCoords] = useState<PopoverCoords | null>(null);

  const chipRef = useRef<HTMLButtonElement>(null);
  const mountedRef = useRef(false);
  const hoverTimerRef = useRef<number | null>(null);
  const requestSeqRef = useRef(0);

  const cancelPendingHover = useCallback(() => {
    if (hoverTimerRef.current !== null) {
      window.clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
    requestSeqRef.current += 1;
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      cancelPendingHover();
    };
  }, [cancelPendingHover]);

  const resolveReference = useCallback(async (): Promise<CitationCacheEntry | null> => {
    const cached = citationCache.get(reference);
    if (cached) return cached;
    try {
      const data = await invoke<ResolvedKnowledgeRefPayload>('cmd_kb_resolve_ref', {
        reference,
      });
      const next: CitationCacheEntry = { state: 'resolved', data };
      writeCitationCache(reference, next);
      return next;
    } catch (error) {
      if (isNotFoundError(error)) {
        const next: CitationCacheEntry = { state: 'unavailable' };
        writeCitationCache(reference, next);
        return next;
      }
      return null;
    }
  }, [reference]);

  const handleHoverStart = useCallback(() => {
    if (chipRef.current) {
      setCoords(computePopoverCoords(chipRef.current));
    }
    setHoverOpen(true);

    const cached = citationCache.get(reference);
    if (cached) {
      setEntry(cached);
      setLoading(false);
      return;
    }

    cancelPendingHover();
    const seq = requestSeqRef.current;
    setLoading(true);

    hoverTimerRef.current = window.setTimeout(() => {
      hoverTimerRef.current = null;
      void resolveReference().then((result) => {
        if (!mountedRef.current || requestSeqRef.current !== seq) return;
        setLoading(false);
        if (result) setEntry(result);
      });
    }, HOVER_DEBOUNCE_MS);
  }, [cancelPendingHover, reference, resolveReference]);

  const handleHoverEnd = useCallback(() => {
    cancelPendingHover();
    setHoverOpen(false);
    setLoading(false);
  }, [cancelPendingHover]);

  useEffect(() => {
    if (!hoverOpen) return;
    const dismiss = () => {
      cancelPendingHover();
      setHoverOpen(false);
      setLoading(false);
    };
    window.addEventListener('resize', dismiss);
    window.addEventListener('scroll', dismiss, true);
    return () => {
      window.removeEventListener('resize', dismiss);
      window.removeEventListener('scroll', dismiss, true);
    };
  }, [cancelPendingHover, hoverOpen]);

  const handleClick = useCallback(
    async (event: React.MouseEvent<HTMLButtonElement>) => {
      event.preventDefault();
      event.stopPropagation();

      const selection = window.getSelection();
      if (
        selection?.toString() &&
        (event.currentTarget.contains(selection.anchorNode) ||
          event.currentTarget.contains(selection.focusNode))
      ) {
        return;
      }

      cancelPendingHover();
      setHoverOpen(false);
      setLoading(false);

      const seq = ++requestSeqRef.current;
      const result = await resolveReference();
      if (!mountedRef.current || requestSeqRef.current !== seq) return;

      if (!result) {
        toastRef.current?.error(t('knowledgeCitation.resolveFailed'));
        return;
      }
      setEntry(result);
      if (result.state === 'unavailable') {
        toastRef.current?.info(t('knowledgeCitation.sourceUnavailable'));
        return;
      }

      const { data } = result;
      if (data.fragment?.status === 'unavailable') {
        toastRef.current?.info(t('knowledgeCitation.fragmentUpdated'));
      }

      const preview = data.preview;
      if (!preview?.path) {
        toastRef.current?.info(t('knowledgeCitation.previewUnavailable'));
        return;
      }

      const action = fileActionRef.current;
      if (!action) return;

      action.openFileTarget(
        {
          scope: 'local',
          path: preview.path,
          initialLineNumber:
            typeof preview.lineNumber === 'number' ? preview.lineNumber : undefined,
        },
        {
          displayName: preview.name,
          displayPath: preview.name,
          forceExternal: event.metaKey || event.ctrlKey,
          page: typeof preview.page === 'number' ? preview.page : undefined,
          utf16Range: Array.isArray(preview.utf16Range) ? preview.utf16Range : undefined,
        },
      );
    },
    [cancelPendingHover, resolveReference, t],
  );

  const isUnavailable = entry?.state === 'unavailable';
  const resolved = entry?.state === 'resolved' ? entry.data : null;
  const resolvedPage =
    (typeof resolved?.preview?.page === 'number' ? resolved.preview.page : undefined) ??
    (resolved?.fragment?.status === 'available' &&
    typeof resolved.fragment.chunk.page === 'number'
      ? resolved.fragment.chunk.page
      : undefined) ??
    parsed.page;

  const rawLabelText = extractPlainChildrenText(children);
  const labelIsRawRef = !rawLabelText || rawLabelText === reference;
  const chipLabel = isUnavailable
    ? t('knowledgeCitation.sourceUnavailable')
    : resolved && labelIsRawRef
      ? extractSourceTitle(resolved, parsed.sourceId)
      : labelIsRawRef
        ? parsed.sourceId
        : rawLabelText;

  const showPageBadge =
    !isUnavailable &&
    typeof resolvedPage === 'number' &&
    !new RegExp(`(?:p\\.?|第\\s*)${resolvedPage}\\b`, 'i').test(chipLabel);

  const excerpt = resolved ? buildExcerpt(resolved) : '';
  const fragmentUpdated = resolved?.fragment?.status === 'unavailable';
  const prefix =
    resolved?.fragment?.status === 'available' ? resolved.fragment.chunk.prefix : undefined;

  return (
    <>
      <button
        ref={chipRef}
        type="button"
        data-testid="kb-citation-chip"
        data-kb-ref={reference}
        data-kb-unavailable={isUnavailable ? 'true' : undefined}
        onMouseEnter={handleHoverStart}
        onMouseLeave={handleHoverEnd}
        onFocus={handleHoverStart}
        onBlur={handleHoverEnd}
        onClick={(event) => void handleClick(event)}
        className={`kb-citation-chip mx-0.5 inline-flex max-w-[260px] items-center gap-1 rounded-md border px-1.5 py-0.5 align-baseline text-[0.82em] font-medium leading-tight transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--accent)] ${
          isUnavailable
            ? 'cursor-default border-[var(--line-subtle)] bg-[var(--paper-inset)]/60 text-[var(--ink-muted)] line-through decoration-[var(--ink-subtle)]'
            : 'cursor-pointer border-[var(--line)] bg-[var(--paper-inset)]/75 text-[var(--accent-warm)] hover:border-[var(--accent)]/50 hover:bg-[var(--paper-inset)] hover:text-[var(--accent-warm-hover)]'
        }`}
      >
        <BookIcon aria-hidden className="size-[0.95em] shrink-0 opacity-80" />
        <span className="truncate">{chipLabel}</span>
        {showPageBadge && (
          <span
            data-testid="kb-citation-page"
            className="shrink-0 rounded bg-[var(--paper)]/80 px-1 text-[0.88em] text-[var(--ink-muted)]"
          >
            {t('knowledgeCitation.pageShort', { page: resolvedPage })}
          </span>
        )}
      </button>

      {hoverOpen &&
        coords &&
        typeof document !== 'undefined' &&
        createPortal(
          <div
            role="tooltip"
            data-testid="kb-citation-popover"
            style={{
              position: 'fixed',
              left: coords.left,
              top: coords.top,
              transform: coords.placement === 'above' ? 'translateY(-100%)' : undefined,
            }}
            className="pointer-events-none z-[9990] w-80 max-w-[calc(100vw-24px)] rounded-xl border border-[var(--line-strong)] bg-[var(--paper-elevated)] p-3 text-left text-xs text-[var(--ink)] shadow-lg"
          >
            {loading ? (
              <p className="text-[var(--ink-muted)]">{t('knowledgeCitation.loading')}</p>
            ) : isUnavailable ? (
              <p className="text-[var(--ink-muted)]">{t('knowledgeCitation.sourceUnavailableHint')}</p>
            ) : resolved ? (
              <div className="space-y-1.5">
                <div className="flex items-start justify-between gap-2">
                  <span className="font-semibold text-[var(--ink)] break-words">
                    {extractSourceTitle(resolved, parsed.sourceId)}
                  </span>
                  {typeof resolvedPage === 'number' && (
                    <span className="shrink-0 rounded bg-[var(--paper-inset)] px-1.5 py-0.5 text-xs text-[var(--ink-muted)]">
                      {t('knowledgeCitation.pageLabel', { page: resolvedPage })}
                    </span>
                  )}
                </div>
                {prefix && (
                  <p className="truncate text-xs text-[var(--accent)]">{prefix}</p>
                )}
                {fragmentUpdated && (
                  <p className="rounded bg-[var(--warning-bg)] px-2 py-1 text-xs text-[var(--warning)]">
                    {t('knowledgeCitation.fragmentUpdated')}
                  </p>
                )}
                {excerpt ? (
                  <p
                    data-testid="kb-citation-excerpt"
                    className="line-clamp-6 whitespace-pre-wrap leading-relaxed text-[var(--ink-muted)]"
                  >
                    {excerpt}
                  </p>
                ) : (
                  <p className="text-[var(--ink-subtle)]">{t('knowledgeCitation.noExcerpt')}</p>
                )}
              </div>
            ) : (
              <p className="text-[var(--ink-muted)]">{t('knowledgeCitation.resolveFailed')}</p>
            )}
          </div>,
          document.body,
        )}
    </>
  );
}
