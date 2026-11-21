// IDs are opaque on the wire. Validate input before using it as an identifier.
export type KnowledgeBaseId = `kb_${string}`;
export type KnowledgeSourceId = `src_${string}`;
export type KnowledgeJobId = `job_${string}`;
export type KnowledgePlanId = `plan_${string}`;
export type KnowledgeChunkId = string;

export type KnowledgeMetadataOrigin = 'agent' | 'user';

export interface KnowledgeMetadataField<T> {
  value: T;
  origin: KnowledgeMetadataOrigin;
  /** ISO 8601 timestamp. */
  updatedAt: string;
}

export const KNOWLEDGE_ERROR_CODES = [
  'KB_NOT_FOUND',
  'KB_ACCESS_DENIED',
  'KB_FIELD_LOCKED',
  'KB_LIMIT_EXCEEDED',
  'KB_SOURCE_UNSUPPORTED',
  'KB_INDEXING',
  'KB_CATEGORY_NOT_ALLOWED',
  'KB_INVALID_INPUT',
  'KB_STORE_CORRUPT',
  'KB_STORE_UNAVAILABLE',
] as const;

export type KnowledgeErrorCode = (typeof KNOWLEDGE_ERROR_CODES)[number];

export interface KnowledgeRef {
  kbId: KnowledgeBaseId;
  sourceId: KnowledgeSourceId;
  page?: number;
  chunkId?: KnowledgeChunkId;
}

const MAX_PAGE = 0xffff_ffff;
const MAX_REF_LENGTH = 74;

function isLowerHex(value: string, length: number): boolean {
  return value.length === length && !/[^0-9a-f]/u.test(value);
}

function isCalendarDate(value: string): boolean {
  if (value.length !== 8 || /[^0-9]/u.test(value) || value.startsWith('0000')) return false;
  const isoDay = `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}`;
  const date = new Date(`${isoDay}T00:00:00.000Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().startsWith(isoDay);
}

export function isKnowledgeBaseId(value: string): value is KnowledgeBaseId {
  return value.startsWith('kb_') && isLowerHex(value.slice(3), 12);
}

export function isKnowledgeSourceId(value: string): value is KnowledgeSourceId {
  return value.startsWith('src_') && isLowerHex(value.slice(4), 16);
}

export function isKnowledgeChunkId(value: string): boolean {
  return isLowerHex(value, 16);
}

function isDatedId(value: string, prefix: 'job_' | 'plan_'): boolean {
  if (!value.startsWith(prefix)) return false;
  const suffix = value.slice(prefix.length);
  return suffix[8] === '_' && isCalendarDate(suffix.slice(0, 8)) && isLowerHex(suffix.slice(9), 12);
}

export function isKnowledgeJobId(value: string): value is KnowledgeJobId {
  return isDatedId(value, 'job_');
}

export function isKnowledgePlanId(value: string): value is KnowledgePlanId {
  return isDatedId(value, 'plan_');
}

function formatEntropy(entropy: Uint8Array, byteLength: number): string {
  if (entropy.length !== byteLength) throw new RangeError('Invalid knowledge ID entropy length');
  return Array.from(entropy, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

/** The caller supplies entropy. The Store owns collision handling. */
export function createKnowledgeBaseId(entropy: Uint8Array): KnowledgeBaseId {
  return `kb_${formatEntropy(entropy, 6)}`;
}

export function createKnowledgeSourceId(entropy: Uint8Array): KnowledgeSourceId {
  return `src_${formatEntropy(entropy, 8)}`;
}

export function createKnowledgeJobId(date: string, entropy: Uint8Array): KnowledgeJobId | null {
  return isCalendarDate(date) ? `job_${date}_${formatEntropy(entropy, 6)}` : null;
}

export function createKnowledgePlanId(date: string, entropy: Uint8Array): KnowledgePlanId | null {
  return isCalendarDate(date) ? `plan_${date}_${formatEntropy(entropy, 6)}` : null;
}

function isPage(page: number): boolean {
  return Number.isInteger(page) && page >= 1 && page <= MAX_PAGE;
}

/** Parse the literal reference without URL decoding or normalization. */
export function parseKnowledgeRef(value: string): KnowledgeRef | null {
  if (value.length > MAX_REF_LENGTH || !value.startsWith('kb://')) return null;
  const [target, chunkId, ...extraFragments] = value.slice(5).split('#');
  if (extraFragments.length || (chunkId !== undefined && !isKnowledgeChunkId(chunkId))) return null;

  const [path, query, ...extraQueries] = target.split('?');
  const [kbId, sourceId = '', ...extraSegments] = path.split('/');
  if (extraQueries.length || extraSegments.length || !isKnowledgeBaseId(kbId) || !isKnowledgeSourceId(sourceId)) {
    return null;
  }

  const ref: KnowledgeRef = { kbId, sourceId };
  if (query !== undefined) {
    const digits = query.slice(5);
    if (!query.startsWith('page=') || !digits || digits.startsWith('0') || /[^0-9]/u.test(digits)) return null;
    const page = Number(digits);
    if (!isPage(page)) return null;
    ref.page = page;
  }
  if (chunkId !== undefined) ref.chunkId = chunkId;
  return ref;
}

export function formatKnowledgeRef({ kbId, sourceId, page, chunkId }: KnowledgeRef): string | null {
  if (
    !isKnowledgeBaseId(kbId) || !isKnowledgeSourceId(sourceId) ||
    (page !== undefined && !isPage(page)) ||
    (chunkId !== undefined && !isKnowledgeChunkId(chunkId))
  ) return null;

  const query = page === undefined ? '' : `?page=${page}`;
  const fragment = chunkId === undefined ? '' : `#${chunkId}`;
  return `kb://${kbId}/${sourceId}${query}${fragment}`;
}
