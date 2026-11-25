import { describe, expect, it } from 'vitest';

import { buildKnowledgeInput } from './knowledge';

describe('buildKnowledgeInput K14 organize commands', () => {
  const kbId = 'kb_0123456789ab';
  const planId = 'plan_20261125_0123456789ab';

  it('parses propose, show, list, and feedback arguments', () => {
    expect(buildKnowledgeInput('kb/organize/propose', [], { kb: [kbId], planFile: 'plan.json' }))
      .toEqual({ kbId, planFile: 'plan.json' });
    expect(buildKnowledgeInput('kb/organize/show', [planId], {}))
      .toEqual({ planId });
    expect(buildKnowledgeInput('kb/organize/list', [], {}))
      .toEqual({});
    expect(buildKnowledgeInput('kb/organize/list', [], { kb: [kbId] }))
      .toEqual({ kbId });
    expect(buildKnowledgeInput('kb/organize/feedback', [], { kb: [kbId], limit: '50' }))
      .toEqual({ kbId, limit: 50 });
  });

  it('rejects invalid plan IDs, missing plan-file, and out-of-range limits', () => {
    expect(() => buildKnowledgeInput('kb/organize/show', ['invalid-plan'], {})).toThrow();
    expect(() => buildKnowledgeInput('kb/organize/propose', [], { kb: [kbId] })).toThrow();
    expect(() => buildKnowledgeInput('kb/organize/feedback', [], { kb: [kbId], limit: '0' })).toThrow();
    expect(() => buildKnowledgeInput('kb/organize/feedback', [], { kb: [kbId], limit: '501' })).toThrow();
  });
});
