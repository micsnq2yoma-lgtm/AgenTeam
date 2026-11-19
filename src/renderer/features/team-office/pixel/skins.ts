// Team skins for the pixel office. A skin owns the scenery (palette, windows,
// wall centrepiece, corner prop, desk props) and the headwear drawn on top of
// every ported employee sprite; the layout, sprites and status actions stay
// shared so every skin shows the same facts.
import type { TeamOfficeSkinId } from "@/config/types";

import type { PixelAction } from "./characters";

export interface OfficePalette {
  floorA: string;
  floorB: string;
  wall: string;
  wallDark: string;
  trim: string;
  sky: string;
  desk: string;
  deskTop: string;
  deskLeg: string;
  monitor: string;
  screenIdle: string;
  screenOn: string;
  code: string;
  plant: string;
  plantDark: string;
  pot: string;
  rug: string;
  rugB: string;
  chair: string;
  mug: string;
  shelf: string;
  books: string[];
  accent: string;
}

/** Canvas access handed to skin painters; `px` snaps to whole pixels. */
export interface SkinPainter {
  px(color: string, x: number, y: number, w: number, h: number): void;
  clip(x: number, y: number, w: number, h: number, draw: () => void): void;
}

export interface OfficeSkin {
  id: TeamOfficeSkinId;
  light: OfficePalette;
  dark: OfficePalette;
  /** Floor checker size in world pixels. */
  floorTile: number;
  /** Peach → lavender in the office; each skin picks its own two-stop outline for the lead's rug. */
  leadOutline: readonly [string, string];
  /** A 30×20 opening at (x, 10), framed from (x-2, 8). */
  window(d: SkinPainter, P: OfficePalette, x: number, t: number, index: number): void;
  /** A 32×30 wall piece at (x, 8), between the two windows. */
  centerpiece(d: SkinPainter, P: OfficePalette, x: number, t: number): void;
  /** A ~10×15 floor prop at the zone's top-right corner. */
  corner(d: SkinPainter, P: OfficePalette, x: number, y: number, t: number): void;
  /** Everything standing on a desk whose top edge is at (dx, dy). */
  deskProps(d: SkinPainter, P: OfficePalette, dx: number, dy: number, anim: PixelAction, t: number, isLead: boolean): void;
  /** Drawn over the sprite: `cx` is its centre column, `top` its highest painted row. */
  headwear?(d: SkinPainter, P: OfficePalette, cx: number, top: number, t: number, isLead: boolean): void;
}

const blink = (t: number, ms: number) => Math.floor(t / ms) % 2 === 1;

/* ---------------------------------------------------------------- office */

const office: OfficeSkin = {
  id: "office",
  /** DESIGN §5 light office. */
  light: {
    floorA: "#EFEDE9", floorB: "#E8E5E1", wall: "#F6F5F3", wallDark: "#E4E1DD", trim: "#D8D2CB", sky: "#F7E4D6",
    desk: "#3A3F45", deskTop: "#CDBBA8", deskLeg: "#3A3F45", monitor: "#2B3138", screenIdle: "#46566B", screenOn: "#9FC4BA",
    code: "#ffffff", plant: "#8FA58A", plantDark: "#7A9475", pot: "#E3876A", rug: "#EDEAE6", rugB: "#E3876A",
    chair: "#5F6266", mug: "#ffffff", shelf: "#8E7F73", books: ["#E3876A", "#C6BCC4", "#EBC3A0", "#A9B79B", "#9AA7B5"], accent: "#E3876A",
  },
  /** DESIGN §5 dark office. */
  dark: {
    floorA: "#191C20", floorB: "#1D2125", wall: "#1E2226", wallDark: "#2A2F34", trim: "#3A3F45", sky: "#3B3240",
    desk: "#2B3035", deskTop: "#8C7C6C", deskLeg: "#2B3035", monitor: "#14181C", screenIdle: "#33465E", screenOn: "#5CC4AE",
    code: "#ffffff", plant: "#6E8A6A", plantDark: "#5D7859", pot: "#C9765D", rug: "#22262A", rugB: "#C9765D",
    chair: "#3A4047", mug: "#D8D2CB", shelf: "#5B5048", books: ["#E3876A", "#C6BCC4", "#EBC3A0", "#A9B79B", "#9AA7B5"], accent: "#E3876A",
  },
  floorTile: 10,
  leadOutline: ["#F2BC94", "#BFB6D6"],
  window(d, P, wx, t, i) {
    d.px(P.trim, wx - 2, 8, 34, 24);
    d.px(P.sky, wx, 10, 30, 20);
    const cx = wx + ((Math.floor(t / 140) + i * 11) % 40) - 8;
    d.clip(wx, 10, 30, 20, () => {
      d.px("#ffffff", cx, 15, 9, 3);
      d.px("#ffffff", cx + 2, 13, 5, 2);
    });
    d.px(P.trim, wx + 14, 10, 2, 20);
    d.px(P.trim, wx, 19, 30, 1);
  },
  centerpiece(d, P, sx) {
    d.px(P.shelf, sx, 8, 32, 30);
    for (let s = 0; s < 2; s++) for (let b = 0; b < 6; b++) {
      const hgt = 6 + ((b * 3 + s) % 3);
      d.px(P.books[(b + s) % P.books.length], sx + 2 + b * 5, 10 + s * 13 + (9 - hgt), 3, hgt);
    }
  },
  corner(d, P, x, y) {
    d.px(P.plantDark, x + 2, y, 6, 4);
    d.px(P.plant, x, y + 3, 10, 5);
    d.px(P.plantDark, x + 3, y + 6, 4, 3);
    d.px(P.pot, x + 2, y + 9, 7, 6);
  },
  deskProps(d, P, dx, dy, anim, t, isLead) {
    d.px(P.monitor, dx + 2, dy - 9, 13, 10);
    let scr = anim === "working" ? P.screenOn : P.screenIdle;
    if (anim === "paused") scr = P.monitor;
    if (anim === "failed") scr = blink(t, 500) ? "#c23b3b" : P.screenIdle;
    if (anim === "waiting") scr = blink(t, 400) ? "#ffcf5a" : P.screenIdle;
    d.px(scr, dx + 3, dy - 8, 11, 7);
    if (anim === "working") {
      for (let i = 0; i < 3; i++) {
        const lw = 2 + ((Math.floor(t / 160) + i * 3 + dx) % 8);
        d.px(P.code, dx + 4, dy - 7 + i * 2, Math.min(9, lw), 1);
      }
    }
    if (isLead) d.px(P.accent, dx + 30, dy + 1, 7, 2);
    else d.px(P.mug, dx + 26, dy - 1, 3, 3);
  },
};

/* ----------------------------------------------------------------- court */

const COURT_HAT = "#1B1714";
const COURT_JADE = "#7FBFA4";

const court: OfficeSkin = {
  id: "court",
  light: {
    floorA: "#B8AB95", floorB: "#AD9F88", wall: "#A8352B", wallDark: "#7C261F", trim: "#D9A441", sky: "#F3E6C6",
    desk: "#5A2A22", deskTop: "#82402F", deskLeg: "#3B1C17", monitor: "#6B4A2E", screenIdle: "#E7DABA", screenOn: "#F6EDD3",
    code: "#2B2420", plant: "#E9E3DA", plantDark: "#B7ADA2", pot: "#8C6A2F", rug: "#E2B85C", rugB: "#A8352B",
    chair: "#4A2620", mug: "#CFE3D6", shelf: "#231D19", books: ["#D9A441", "#E8C468", "#C8922E"], accent: "#D9A441",
  },
  dark: {
    floorA: "#2A2622", floorB: "#2F2A25", wall: "#5E1F1A", wallDark: "#431612", trim: "#B98A36", sky: "#4A4232",
    desk: "#3A1C17", deskTop: "#5C2E22", deskLeg: "#261210", monitor: "#4E3621", screenIdle: "#BDB091", screenOn: "#D9CEAF",
    code: "#1E1915", plant: "#9C948A", plantDark: "#6F675F", pot: "#76592A", rug: "#9C7E3C", rugB: "#7C261F",
    chair: "#33190F", mug: "#9FB8A9", shelf: "#16120F", books: ["#B98A36", "#D1AE5A", "#A47826"], accent: "#D9A441",
  },
  floorTile: 20,
  leadOutline: ["#E8C468", "#C0462E"],
  window(d, P, wx, t, i) {
    // Lattice window with paper panes and a hanging red lantern.
    d.px(P.trim, wx - 2, 8, 34, 24);
    d.px(P.sky, wx, 10, 30, 20);
    for (let x = wx + 5; x < wx + 30; x += 6) d.px(P.wallDark, x, 10, 1, 20);
    for (let y = 14; y < 30; y += 5) d.px(P.wallDark, wx, y, 30, 1);
    const sway = blink(t + i * 700, 900) ? 1 : 0;
    const lx = wx + 12 + sway;
    d.px(P.trim, wx + 15, 2, 1, 6);
    d.px("#C9302C", lx, 6 + 2, 7, 7);
    d.px("#E65A46", lx + 1, 6 + 3, 5, 5);
    d.px(P.trim, lx + 1, 7, 5, 1);
    d.px(P.trim, lx + 1, 15, 5, 1);
    d.px(P.trim, lx + 3, 16, 1, 3);
  },
  centerpiece(d, P, sx) {
    // A black plaque with gold border and four gilt glyph blocks.
    d.px(P.trim, sx - 1, 9, 34, 16);
    d.px(P.shelf, sx + 1, 11, 30, 12);
    for (let g = 0; g < 4; g++) {
      const gx = sx + 4 + g * 7;
      const c = P.books[g % P.books.length];
      d.px(c, gx, 13, 4, 1); d.px(c, gx + 1, 14, 2, 5); d.px(c, gx, 19, 4, 1);
      d.px(c, gx + (g % 2 ? 3 : 0), 16, 1, 2);
    }
    d.px(P.trim, sx + 6, 25, 1, 11); d.px(P.trim, sx + 25, 25, 1, 11);
  },
  corner(d, P, x, y, t) {
    // Bronze incense burner with drifting smoke.
    const s = Math.floor(t / 300) % 3;
    d.px(P.plant, x + 4 + (s === 1 ? 1 : 0), y - 4, 1, 3);
    d.px(P.plantDark, x + 5 - (s === 2 ? 1 : 0), y - 1, 1, 2);
    d.px(P.pot, x + 1, y + 2, 9, 2);
    d.px(P.pot, x + 2, y + 4, 7, 6);
    d.px(P.trim, x + 2, y + 6, 7, 1);
    d.px(P.pot, x + 2, y + 10, 1, 4); d.px(P.pot, x + 8, y + 10, 1, 4); d.px(P.pot, x + 5, y + 10, 1, 3);
  },
  deskProps(d, P, dx, dy, anim, t, isLead) {
    if (anim === "working") {
      // Memorial unrolled on the desk, ink lines being written out.
      d.px(P.monitor, dx + 1, dy - 6, 2, 7); d.px(P.monitor, dx + 18, dy - 6, 2, 7);
      d.px(P.screenOn, dx + 3, dy - 6, 15, 6);
      for (let i = 0; i < 4; i++) {
        const h = 1 + ((Math.floor(t / 160) + i * 2 + dx) % 4);
        d.px(P.code, dx + 15 - i * 3, dy - 5, 1, h);
      }
      d.px(P.code, dx + 21, dy - 8, 1, 6); // brush
    } else {
      // Stack of folded memorials.
      const paper = anim === "paused" ? P.monitor : P.screenIdle;
      d.px(paper, dx + 3, dy - 3, 12, 3);
      d.px(P.monitor, dx + 3, dy - 1, 12, 1);
      d.px(paper, dx + 4, dy - 6, 10, 3);
      d.px(P.monitor, dx + 4, dy - 4, 10, 1);
      if (anim === "failed" && blink(t, 500)) d.px("#C9302C", dx + 8, dy - 6, 3, 3); // red seal
      if (anim === "waiting" && blink(t, 400)) d.px("#E8C468", dx + 13, dy - 9, 2, 5); // tag
    }
    if (isLead) {
      // Imperial seal.
      d.px(P.accent, dx + 29, dy - 3, 7, 4);
      d.px(COURT_JADE, dx + 31, dy - 5, 3, 2);
    } else d.px(P.mug, dx + 26, dy - 1, 3, 3); // celadon tea cup
  },
  headwear(d, P, cx, top, _t, isLead) {
    // Black official's cap (wushamao) with two horizontal wings.
    d.px(COURT_HAT, cx - 6, top - 4, 12, 4);
    d.px(COURT_HAT, cx - 4, top - 7, 8, 3);
    d.px(COURT_HAT, cx - 15, top - 3, 9, 2);
    d.px(COURT_HAT, cx + 6, top - 3, 9, 2);
    if (isLead) {
      d.px(P.trim, cx - 6, top - 1, 12, 1);
      d.px(COURT_JADE, cx - 1, top - 6, 2, 2);
    }
  },
};

/* -------------------------------------------------------------- starship */

const STARS: ReadonlyArray<readonly [number, number]> = [[3, 4], [11, 13], [19, 6], [26, 15], [7, 17], [23, 2], [15, 10]];

const starship: OfficeSkin = {
  id: "starship",
  light: {
    floorA: "#CBD3DC", floorB: "#C1CAD4", wall: "#DEE4EB", wallDark: "#A3AEBB", trim: "#7D8997", sky: "#13203B",
    desk: "#46505C", deskTop: "#A9B5C2", deskLeg: "#2E353D", monitor: "#1C232B", screenIdle: "#2C4A63", screenOn: "#5FE0D0",
    code: "#E8FFFB", plant: "#7FE3FF", plantDark: "#3BA8D0", pot: "#46505C", rug: "#D4DBE3", rugB: "#5FE0D0",
    chair: "#58626E", mug: "#F2B544", shelf: "#2E353D", books: ["#5FE0D0", "#F2B544", "#E36B6B", "#7FB2FF", "#B6F27A"], accent: "#5FE0D0",
  },
  dark: {
    floorA: "#161B22", floorB: "#1A2028", wall: "#1C222B", wallDark: "#28303B", trim: "#3B4552", sky: "#05080F",
    desk: "#262D36", deskTop: "#56626F", deskLeg: "#161B22", monitor: "#0D1117", screenIdle: "#1F3A50", screenOn: "#3FC9B8",
    code: "#E8FFFB", plant: "#58C8EA", plantDark: "#2C86AA", pot: "#2E353D", rug: "#1F252D", rugB: "#3FC9B8",
    chair: "#323A45", mug: "#D9A03A", shelf: "#10141A", books: ["#3FC9B8", "#D9A03A", "#C95757", "#6A9CE6", "#9ED868"], accent: "#3FC9B8",
  },
  floorTile: 12,
  leadOutline: ["#5FE0D0", "#9B8CFF"],
  window(d, P, wx, t, i) {
    // Viewport onto drifting stars; the second one has a planet.
    d.px(P.trim, wx - 2, 8, 34, 24);
    d.px(P.wallDark, wx - 1, 9, 32, 22);
    d.px(P.sky, wx, 10, 30, 20);
    const shift = Math.floor(t / 180);
    d.clip(wx, 10, 30, 20, () => {
      for (const [sx, sy] of STARS) d.px("#FFFFFF", wx + ((sx - shift + i * 7) % 30 + 30) % 30, 10 + sy, 1, 1);
      if (i === 1) {
        d.px("#E3876A", wx + 19, 15, 7, 7);
        d.px("#F2BC94", wx + 20, 16, 3, 2);
        d.px("#BFB6D6", wx + 16, 18, 13, 1);
      }
    });
  },
  centerpiece(d, P, sx, t) {
    // Control console with blinking indicator lights.
    d.px(P.shelf, sx, 8, 32, 30);
    d.px(P.screenIdle, sx + 3, 11, 26, 8);
    const sweep = Math.floor(t / 120) % 26;
    d.px(P.screenOn, sx + 3 + sweep, 11, 1, 8);
    for (let r = 0; r < 2; r++) for (let b = 0; b < 6; b++) {
      const on = (Math.floor(t / 400) + b * 3 + r * 5) % 4 !== 0;
      d.px(on ? P.books[(b + r) % P.books.length] : P.trim, sx + 3 + b * 5, 23 + r * 6, 3, 3);
    }
  },
  corner(d, P, x, y, t) {
    // Hologram projector.
    const flicker = blink(t, 700);
    d.px(P.plantDark, x + 2, y + 1, 6, 1);
    d.px(flicker ? P.plant : P.plantDark, x + 3, y - 3, 4, 4);
    d.px(P.plant, x + 4, y + 2, 2, 6);
    d.px(P.pot, x + 1, y + 9, 8, 6);
    d.px(P.accent, x + 3, y + 11, 4, 1);
  },
  deskProps(d, P, dx, dy, anim, t, isLead) {
    office.deskProps(d, P, dx, dy, anim, t, false);
    d.px(P.screenOn, dx + 3, dy - 10, 11, 1); // holo glow strip
    if (isLead) {
      d.px(P.trim, dx + 31, dy - 5, 1, 6);
      d.px(blink(t, 600) ? "#E36B6B" : P.trim, dx + 30, dy - 7, 3, 2); // beacon
    } else d.px(P.mug, dx + 26, dy - 1, 3, 3);
  },
  headwear(d, P, cx, top, t, isLead) {
    // Comms headset: band, ear pieces and an antenna light.
    d.px(P.trim, cx - 8, top - 2, 16, 1);
    d.px(P.trim, cx - 9, top - 2, 1, 4);
    d.px(P.trim, cx + 8, top - 2, 1, 4);
    d.px(P.trim, cx + 4, top - 6, 1, 4);
    d.px(blink(t, isLead ? 450 : 900) ? P.accent : P.plantDark, cx + 3, top - 8, 3, 2);
  },
};

export const OFFICE_SKINS: Readonly<Record<TeamOfficeSkinId, OfficeSkin>> = { office, court, starship };
