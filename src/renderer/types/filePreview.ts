export interface FilePreviewFocusTarget {
  /** Source-local navigation event id. Same line can be focused repeatedly. */
  requestId: number;
  lineNumber: number;
  /** 1-based page number for paginated rich-doc previews (e.g. PDF citations). */
  page?: number;
  /** UTF-16 character range `[start, end)` inside the previewed Markdown/text document. */
  utf16Range?: [number, number];
  query?: string;
  highlights?: [number, number][];
}

/** A navigation intent, before any document content has been loaded. */
export interface FilePreviewTarget {
  name: string;
  path: string;
  sourceScope?: 'workspace' | 'local';
  localPath?: string;
  initialLineNumber?: number;
  focusTarget?: FilePreviewFocusTarget;
  initialEditMode?: boolean;
}

export interface FilePreviewOptions {
  initialEditMode?: boolean;
  /** The initiating surface retains authority over a superseded navigation. */
  isCurrent?: () => boolean;
}
