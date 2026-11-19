import { describe, expect, it } from 'vitest';

import { normalizeTeamOfficeSkin, TEAM_OFFICE_SKIN_IDS } from '@/config/types';

import type { PixelAction } from './characters';
import { OFFICE_SKINS, type SkinPainter } from './skins';

const ACTIONS: PixelAction[] = ['idle', 'working', 'thinking', 'reading', 'paused', 'failed', 'waiting'];
const HEX = /^#[0-9a-f]{6}$/i;

function recorder() {
  const colors = new Set<string>();
  let calls = 0;
  const painter: SkinPainter = {
    px: (color, _x, _y, w, h) => {
      calls += 1;
      colors.add(color);
      expect(w).toBeGreaterThan(0);
      expect(h).toBeGreaterThan(0);
    },
    clip: (_x, _y, _w, _h, draw) => draw(),
  };
  return { painter, colors, calls: () => calls };
}

describe('team office skins', () => {
  it('implements every configured skin id and normalises unknown values to the office', () => {
    expect(Object.keys(OFFICE_SKINS).sort()).toEqual([...TEAM_OFFICE_SKIN_IDS].sort());
    expect(normalizeTeamOfficeSkin('court')).toBe('court');
    expect(normalizeTeamOfficeSkin('vaporwave')).toBe('office');
    expect(normalizeTeamOfficeSkin(undefined)).toBe('office');
  });

  it.each(TEAM_OFFICE_SKIN_IDS)('%s paints its scenery, props and headwear in both themes', (id) => {
    const skin = OFFICE_SKINS[id];
    for (const palette of [skin.light, skin.dark]) {
      for (const [key, value] of Object.entries(palette)) {
        for (const color of Array.isArray(value) ? value : [value]) expect(color, key).toMatch(HEX);
      }
      const { painter, calls } = recorder();
      for (const t of [0, 1000]) {
        skin.window(painter, palette, 14, t, 0);
        skin.window(painter, palette, 104, t, 1);
        skin.centerpiece(painter, palette, 58, t);
        skin.corner(painter, palette, 226, 50, t);
        for (const action of ACTIONS) for (const lead of [true, false]) skin.deskProps(painter, palette, 16, 192, action, t, lead);
        skin.headwear?.(painter, palette, 40, 160, t, true);
      }
      expect(calls()).toBeGreaterThan(0);
    }
  });

  it('gives the court its officials’ caps and the starship its headsets; the office stays bare-headed', () => {
    expect(OFFICE_SKINS.office.headwear).toBeUndefined();
    for (const id of ['court', 'starship'] as const) {
      const { painter, calls } = recorder();
      OFFICE_SKINS[id].headwear?.(painter, OFFICE_SKINS[id].light, 24, 10, 0, false);
      expect(calls()).toBeGreaterThan(0);
    }
  });
});
