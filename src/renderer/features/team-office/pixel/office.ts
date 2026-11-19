// Pixel office canvas, ported from AgenTeam packages/pixel-office/src/index.ts
// (MIT, see THIRD_PARTY_NOTICES.md). Departments become projects and people are
// the project's assistant (lead) and conversations (employees).
import { characterFrame, FRAME_MS, SPRITE_HEIGHT, SPRITE_WIDTH, type PixelAction } from "./characters";
import { OFFICE_SKINS, type OfficePalette, type OfficeSkin, type SkinPainter } from "./skins";
import type { ModelFamily } from "./types";

export interface OfficePerson {
  agentId: string;
  departmentId: string | null;
  name: string;
  title: string;
  model: string;
  /** Sprite family (legacy field name from the source package). */
  eye: ModelFamily;
  isLead: boolean;
  anim: PixelAction;
  bubble?: string | null;
  pendingCards: number;
}

export interface OfficeSnapshot {
  at: string;
  departments: Array<{ id: string; name: string; icon: string }>;
  people: OfficePerson[];
}

export type { OfficePalette } from "./skins";

export function lerpColor(a: string, b: string, t: number): string {
  const p = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [x, y] = [p(a), p(b)];
  return `#${x.map((v, i) => Math.round(v + (y[i] - v) * t).toString(16).padStart(2, "0")).join("")}`;
}



export const WORLD_H = 228;
export const ZONE_W = 240;
const TOP = 46;
const LEAD_DESK_Y = TOP + 46;
const ROW_GAP = 100;
// The desk hides the seated character's lower body; walking sprites use all 44px.
const SEATED_BASELINE = 32;

export interface Seat {
  agentId: string;
  x: number;
  y: number;
  deskX: number;
  deskY: number;
  isLead: boolean;
  zone: number;
}

export interface Layout {
  width: number;
  height: number;
  zones: Array<{ id: string; name: string; x: number; w: number }>;
  seats: Seat[];
}

/** Pure layout: one zone per department (people without a department go to a shared zone). */
export function layoutOffice(snapshot: OfficeSnapshot): Layout {
  const zoneIds = snapshot.departments.map((d) => d.id);
  if (snapshot.people.some((p) => !p.departmentId || !zoneIds.includes(p.departmentId))) zoneIds.push("__none");
  const zones = zoneIds.map((id, i) => ({
    id,
    name: snapshot.departments.find((d) => d.id === id)?.name ?? "其他",
    x: i * ZONE_W,
    w: ZONE_W,
  }));
  const seats: Seat[] = [];
  zones.forEach((z, zi) => {
    const people = snapshot.people.filter((p) => (p.departmentId && zoneIds.includes(p.departmentId) ? p.departmentId : "__none") === z.id);
    const lead = people.find((p) => p.isLead);
    const staff = people.filter((p) => p !== lead);
    if (lead) {
      const deskX = z.x + z.w / 2 - 20;
      const deskY = LEAD_DESK_Y;
      seats.push({ agentId: lead.agentId, x: deskX - 4, y: deskY - SEATED_BASELINE, deskX, deskY, isLead: true, zone: zi });
    }
    staff.forEach((p, i) => {
      const col = i % 3;
      const row = Math.floor(i / 3);
      const deskX = z.x + 16 + col * 74;
      const deskY = LEAD_DESK_Y + (row + (lead ? 1 : 0)) * ROW_GAP;
      seats.push({ agentId: p.agentId, x: deskX - 4, y: deskY - SEATED_BASELINE, deskX, deskY, isLead: false, zone: zi });
    });
  });
  const height = Math.max(WORLD_H, ...seats.map((s) => s.deskY + 36));
  return { width: Math.max(ZONE_W, zones.length * ZONE_W), height, zones, seats };
}

export interface Label {
  agentId: string;
  name: string;
  model: string;
  title: string;
  anim: PixelAction;
  bubble: string | null;
  pendingCards: number;
  /** fractions of the canvas (0..1) for HTML overlay positioning */
  left: number;
  top: number;
  bubbleTop: number;
  isLead: boolean;
}

/**
 * Label positions for seated people, derived from the pure layout so React can
 * render overlays without reading back from the canvas renderer. Matches
 * `OfficeRenderer.labels()` for anyone not on a delivery walk.
 */
export function seatedLabels(snapshot: OfficeSnapshot, layout: Layout): Label[] {
  return snapshot.people.flatMap((p) => {
    const seat = layout.seats.find((s) => s.agentId === p.agentId);
    if (!seat) return [];
    return [{
      agentId: p.agentId,
      name: p.name,
      model: p.model,
      title: p.title,
      anim: p.anim,
      bubble: p.bubble ?? null,
      pendingCards: p.pendingCards,
      left: (seat.x + SPRITE_WIDTH / 2) / layout.width,
      top: (seat.deskY + 18) / layout.height,
      bubbleTop: Math.max(42, seat.y - 6) / layout.height,
      isLead: p.isLead,
    }];
  });
}

type Walker = { agentId: string; path: Array<[number, number]>; i: number; x: number; y: number; back: boolean };

export class OfficeRenderer {
  private ctx: CanvasRenderingContext2D;
  private snapshot: OfficeSnapshot = { at: "", departments: [], people: [] };
  private layout: Layout = layoutOffice(this.snapshot);
  private raf = 0;
  private last = 0;
  private walkers = new Map<string, Walker>();
  private seenWalk = new Set<string>();
  selected: string | null = null;
  reducedMotion = false;
  private skin: OfficeSkin = OFFICE_SKINS.office;
  private palette: OfficePalette = OFFICE_SKINS.office.light;
  private readonly painter: SkinPainter = {
    px: (c, x, y, w, h) => this.px(c, x, y, w, h),
    clip: (x, y, w, h, draw) => {
      this.ctx.save();
      this.ctx.beginPath();
      this.ctx.rect(x, y, w, h);
      this.ctx.clip();
      draw();
      this.ctx.restore();
    },
  };

  constructor(private canvas: HTMLCanvasElement) {
    const c = canvas.getContext("2d");
    if (!c) throw new Error("canvas 2d not available");
    this.ctx = c;
    this.reducedMotion = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  }

  /** Swap the scenery and headwear; the next frame repaints with it. */
  setSkin(skin: OfficeSkin, dark: boolean) {
    this.skin = skin;
    this.palette = dark ? skin.dark : skin.light;
  }

  setSnapshot(s: OfficeSnapshot) {
    this.snapshot = s;
    this.layout = layoutOffice(s);
    for (const id of this.seenWalk) {
      if (this.reducedMotion || !s.people.some((p) => p.agentId === id && p.anim === "walk_to_lead")) {
        this.seenWalk.delete(id);
        this.walkers.delete(id);
      }
    }
    if (this.canvas.width !== this.layout.width || this.canvas.height !== this.layout.height) {
      this.canvas.width = this.layout.width;
      this.canvas.height = this.layout.height;
    }
    // one-off delivery walk to the lead
    for (const p of s.people) {
      if (p.anim !== "walk_to_lead") {
        this.seenWalk.delete(p.agentId);
        continue;
      }
      if (this.seenWalk.has(p.agentId) || this.reducedMotion) continue;
      this.seenWalk.add(p.agentId);
      const seat = this.layout.seats.find((x) => x.agentId === p.agentId);
      const lead = this.layout.seats.find((x) => x.isLead && x.zone === seat?.zone);
      if (seat && lead) {
        const midY = lead.deskY + 22;
        const aisleX = Math.min(seat.deskX + 42, (seat.zone + 1) * ZONE_W - SPRITE_WIDTH - 2);
        // Leave the first row vertically before crossing the open corridor,
        // instead of passing sideways through the neighbouring employee.
        const approach: Array<[number, number]> = seat.deskY === lead.deskY + ROW_GAP
          ? [[seat.x, midY]]
          : [[aisleX, seat.y], [aisleX, midY]];
        const path: Array<[number, number]> = [
          [seat.x, seat.y],
          ...approach,
          [lead.x, midY],
          [lead.x, lead.deskY + 14],
        ];
        this.walkers.set(p.agentId, { agentId: p.agentId, path, i: 1, x: seat.x, y: seat.y, back: false });
      }
    }
  }

  labels(): Label[] {
    return this.snapshot.people.map((p) => {
      const seat = this.layout.seats.find((s) => s.agentId === p.agentId);
      const w = this.walkers.get(p.agentId);
      const x = w ? w.x : (seat?.x ?? 0);
      const y = w ? w.y : (seat?.y ?? 0);
      return {
        agentId: p.agentId,
        name: p.name,
        model: p.model,
        title: p.title,
        anim: p.anim,
        bubble: p.bubble ?? null,
        pendingCards: p.pendingCards,
        left: (x + SPRITE_WIDTH / 2) / this.layout.width,
        top: (w || !seat ? y + SPRITE_HEIGHT + 5 : seat.deskY + 18) / this.layout.height,
        bubbleTop: Math.max(42, y - 6) / this.layout.height,
        isLead: p.isLead,
      };
    });
  }

  zoneLabels() {
    return this.layout.zones.map((z) => ({ id: z.id, name: z.name, left: (z.x + z.w / 2) / this.layout.width }));
  }

  /** canvas pixel -> agent id */
  hitTest(fx: number, fy: number): string | null {
    const x = fx * this.layout.width;
    const y = fy * this.layout.height;
    for (const s of this.layout.seats) {
      const w = this.walkers.get(s.agentId);
      const px = w ? w.x : s.x;
      const py = w ? w.y : s.y;
      if (x >= px && x <= px + SPRITE_WIDTH && y >= py && y <= py + SPRITE_HEIGHT) return s.agentId;
      if (!w && x >= s.deskX && x <= s.deskX + 40 && y >= s.deskY - 12 && y <= s.deskY + 14) return s.agentId;
    }
    return null;
  }

  start() {
    const loop = (t: number) => {
      this.frame(t);
      this.raf = requestAnimationFrame(loop);
    };
    this.raf = requestAnimationFrame(loop);
  }

  stop() {
    cancelAnimationFrame(this.raf);
  }

  /* ------------------------------------------------------------ */

  private px(c: string, x: number, y: number, w: number, h: number) {
    this.ctx.fillStyle = c;
    this.ctx.fillRect(Math.round(x), Math.round(y), w, h);
  }

  private frame(t: number) {
    const dt = Math.min(64, t - (this.last || t));
    this.last = t;
    if (!this.reducedMotion) this.stepWalkers(dt);
    if (this.reducedMotion) t = 0;
    const P = this.palette;
    const skin = this.skin;
    const d = this.painter;
    const { width: W, height: H } = this.layout;
    this.ctx.imageSmoothingEnabled = false;
    // floor
    const tile = skin.floorTile;
    for (let iy = 0; iy * tile + TOP < H; iy++) for (let ix = 0; ix * tile < W; ix++) this.px((ix + iy) % 2 ? P.floorA : P.floorB, ix * tile, TOP + iy * tile, tile, tile);
    // wall
    this.px(P.wall, 0, 0, W, 38);
    this.px(P.wallDark, 0, 36, W, 8);
    this.px(P.trim, 0, 44, W, 2);
    for (const z of this.layout.zones) {
      // zone divider
      if (z.x > 0) this.px(P.trim, z.x, TOP, 1, H - TOP);
      // windows and the wall piece between them
      [z.x + 14, z.x + 104].forEach((wx, i) => skin.window(d, P, wx, t, i));
      skin.centerpiece(d, P, z.x + 58, t);
      // lead rug
      const lead = this.layout.seats.find((s) => s.isLead && s.zone === this.layout.zones.indexOf(z));
      if (lead) {
        const rx = lead.deskX - 8, ry = lead.deskY - 22, rw = 56, rh = 40;
        this.px(P.rug, rx, ry, rw, rh);
        // 3px outline, peach at the top-left fading to lavender at the bottom-right
        for (let i = 0; i < rw; i++) {
          const c = lerpColor(skin.leadOutline[0], skin.leadOutline[1], i / (rw - 1));
          this.px(c, rx + i, ry - 3, 1, 3);
          this.px(c, rx + i, ry + rh, 1, 3);
        }
        for (let j = -3; j < rh + 3; j++) {
          const t = (j + 3) / (rh + 5);
          this.px(lerpColor(skin.leadOutline[0], skin.leadOutline[1], t * 0.5), rx - 3, ry + j, 3, 1);
          this.px(lerpColor(skin.leadOutline[0], skin.leadOutline[1], 0.5 + t * 0.5), rx + rw, ry + j, 3, 1);
        }
      }
      skin.corner(d, P, z.x + z.w - 14, TOP + 4, t);
    }
    // seats
    const byId = new Map(this.snapshot.people.map((p) => [p.agentId, p]));
    for (const s of this.layout.seats) {
      const p = byId.get(s.agentId);
      if (!p) continue;
      this.px(P.chair, s.deskX + 12, s.deskY - 20, 18, 21);
      const walking = this.walkers.has(s.agentId);
      if (!walking) this.drawRobot(s.x, s.y, p, t);
      this.drawDesk(s.deskX, s.deskY, walking ? "idle" : p.anim, t, s.isLead);
    }
    for (const w of this.walkers.values()) {
      const p = byId.get(w.agentId);
      if (p) {
        this.drawRobot(w.x, w.y, p, t, true);
      }
    }
    // selection marker
    if (this.selected) {
      const s = this.layout.seats.find((x) => x.agentId === this.selected);
      const w = this.walkers.get(this.selected ?? "");
      if (s) {
        const x = w ? w.x : s.x;
        const y = w ? w.y : s.y;
        const b = this.reducedMotion ? 0 : Math.floor(t / 300) % 2;
        this.px(P.accent, x + 21, y - 10 + b, 6, 2);
        this.px(P.accent, x + 22, y - 8 + b, 4, 1);
        this.px(P.accent, x + 23, y - 7 + b, 2, 1);
      }
    }
  }

  private drawRobot(x: number, y: number, p: OfficeSnapshot["people"][number], t: number, walking = false) {
    const frame = this.reducedMotion ? 0 : Math.floor(t / FRAME_MS);
    const action = walking ? "walk_to_lead" : p.anim;
    let top = SPRITE_HEIGHT;
    for (const [dx, dy, w, h, color] of characterFrame(p.eye, action, frame)) {
      this.px(color, x + dx, y + dy, w, h);
      if (dy < top) top = dy;
    }
    this.skin.headwear?.(this.painter, this.palette, x + SPRITE_WIDTH / 2, y + top, t, p.isLead);
  }

  private drawDesk(dx: number, dy: number, anim: PixelAction, t: number, isLead: boolean) {
    const P = this.palette;
    const w = isLead ? 40 : 34;
    this.px(P.deskTop, dx, dy, w, 4);
    this.px(P.desk, dx, dy + 4, w, 7);
    this.px(P.deskLeg, dx, dy + 10, w, 1);
    this.px(P.deskLeg, dx + 1, dy + 11, 3, 3);
    this.px(P.deskLeg, dx + w - 4, dy + 11, 3, 3);
    this.skin.deskProps(this.painter, P, dx, dy, anim, t, isLead);
  }

  private stepWalkers(dt: number) {
    for (const [id, w] of this.walkers) {
      let budget = dt * 0.05;
      while (budget > 0 && w.i < w.path.length) {
        const [tx, ty] = w.path[w.i];
        const dx = tx - w.x;
        const dy = ty - w.y;
        const d = Math.hypot(dx, dy);
        if (d <= budget) {
          w.x = tx;
          w.y = ty;
          w.i++;
          budget -= d;
        } else {
          w.x += (dx / d) * budget;
          w.y += (dy / d) * budget;
          budget = 0;
        }
      }
      if (w.i >= w.path.length) {
        if (w.back) this.walkers.delete(id);
        else {
          // walk back to the seat
          w.path = [...w.path].reverse();
          w.i = 1;
          w.back = true;
        }
      }
    }
  }
}
