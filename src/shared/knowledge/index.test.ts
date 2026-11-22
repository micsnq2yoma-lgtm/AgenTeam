import { describe, expect, it } from 'vitest';

import refs from './__fixtures__/refs.json';
import {
  createKnowledgeBaseId,
  createKnowledgeJobId,
  createKnowledgePlanId,
  createKnowledgeSourceId,
  formatKnowledgeRef,
  isKnowledgeBaseId,
  isKnowledgeJobId,
  isKnowledgePlanId,
  isKnowledgeSourceId,
  KNOWLEDGE_ERROR_CODES,
  parseKnowledgeRef,
  type KnowledgeRef,
} from './index';

it('exposes the native oversized grapheme error to clients', () => {
  expect(KNOWLEDGE_ERROR_CODES).toContain('KB_CHUNK_UNSPLITTABLE');
});

describe('knowledge references', () => {
  it('parses and formats the shared native fixture', () => {
    expect(refs.length).toBeGreaterThanOrEqual(30);
    for (const { input, expected } of refs) {
      const parsed = parseKnowledgeRef(input);
      expect(parsed, JSON.stringify(input)).toEqual(expected);
      if (parsed) expect(formatKnowledgeRef(parsed)).toBe(input);
    }
  });

  it('refuses to serialize invalid reference components', () => {
    const reference: KnowledgeRef = {
      kbId: 'kb_0123456789ab',
      sourceId: 'src_0123456789abcdef',
    };
    for (const fields of [
      { kbId: 'kb_0123456789AB' },
      { sourceId: 'src_0123456789abcdef\n' },
      { page: 0 },
      { page: -1 },
      { page: 1.5 },
      { page: 4_294_967_296 },
      { page: Number.NaN },
      { chunkId: 'abcdef0123456789?' },
    ] as const) {
      expect(formatKnowledgeRef({ ...reference, ...fields })).toBeNull();
    }
  });
});

describe('knowledge IDs', () => {
  it('formats 100,000 unique IDs per kind from deterministic entropy', () => {
    const entropy = new Uint8Array(8);
    const counter = new DataView(entropy.buffer);
    const shortEntropy = entropy.subarray(2);
    const ids = new Set<string>();
    const kinds = [
      [() => createKnowledgeBaseId(shortEntropy), /^kb_[0-9a-f]{12}$/, isKnowledgeBaseId],
      [() => createKnowledgeSourceId(entropy), /^src_[0-9a-f]{16}$/, isKnowledgeSourceId],
      [() => createKnowledgeJobId('20261121', shortEntropy), /^job_20261121_[0-9a-f]{12}$/, isKnowledgeJobId],
      [() => createKnowledgePlanId('20261121', shortEntropy), /^plan_20261121_[0-9a-f]{12}$/, isKnowledgePlanId],
    ] as const;

    let invalid = 0;
    for (let index = 0; index < 100_000; index += 1) {
      counter.setUint32(4, index);
      for (const [create, pattern, validate] of kinds) {
        const id = create();
        if (id && pattern.test(id) && validate(id)) ids.add(id);
        else invalid += 1;
      }
    }
    expect(invalid).toBe(0);
    expect(ids.size).toBe(400_000);
  });

  it('requires canonical dates and the prescribed entropy width', () => {
    const entropy = new Uint8Array(6).fill(0xab);
    for (const invalid of ['00000101', '19000229', '20260229', '20261131', '20261301', '2026-11-21']) {
      expect(createKnowledgeJobId(invalid, entropy)).toBeNull();
      expect(createKnowledgePlanId(invalid, entropy)).toBeNull();
      expect(isKnowledgeJobId('job_' + invalid + '_000000000000')).toBe(false);
      expect(isKnowledgePlanId('plan_' + invalid + '_000000000000')).toBe(false);
    }
    expect(createKnowledgePlanId('20000229', entropy)).toBe('plan_20000229_abababababab');
    expect(() => createKnowledgeBaseId(new Uint8Array(5))).toThrow(RangeError);
    expect(() => createKnowledgeSourceId(new Uint8Array(9))).toThrow(RangeError);
  });
});
