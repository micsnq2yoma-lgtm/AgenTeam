import { describe, expect, it } from 'vitest';

import { logsExportZipName } from './logs-export';

describe('logsExportZipName', () => {
  it('names the archive after AgenTeam with the export date', () => {
    expect(logsExportZipName(new Date('2026-11-24T08:00:00Z'))).toBe('AgenTeam-logs-2026-11-24.zip');
  });

  it('never carries the upstream product name', () => {
    expect(logsExportZipName()).not.toMatch(/myagents/i);
  });
});
