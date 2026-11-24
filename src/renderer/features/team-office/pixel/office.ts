// Pixel office canvas, ported from AgenTeam packages/pixel-office/src/index.ts
// (MIT, see THIRD_PARTY_NOTICES.md). Departments become projects and people are
// the project's assistant (lead) and conversations (employees).
//
// Adaptive layout ("growing building"): as projects and staff grow, layoutOffice
// steps through 5 tiers — room → dense → floors → tower → campus — choosing the
// first tier that fits inside the viewport without scrolling.
import {
  characterFrame,
  FRAME_MS,
  modelFamilyColors,
  SPRITE_HEIGHT,
  SPRITE_WIDTH,
  type PixelAction,
} from "./characters";
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

export type OfficeTier = "room" | "dense" | "floors" | "tower" | "campus";
export type OfficeDensity = "full" | "compact" | "mini";
export type BuildingShell = "none" | "floors" | "tower";

export interface OfficeViewport {
  width: number;
  height: number;
}

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

interface DensitySpec {
  density: OfficeDensity;
  top: number;
  leadDeskOff: number;
  rowGap: number;
  padX: number;
  pitch: number;
  deskW: number;
  leadDeskW: number;
  bottomPad: number;
  minHeight: number;
  spriteScale: number;
  seatedBaseline: number;
  cols: number;
}

const FULL_SPEC: DensitySpec = {
  density: "full",
  top: TOP,
  leadDeskOff: LEAD_DESK_Y - TOP,
  rowGap: ROW_GAP,
  padX: 16,
  pitch: 74,
  deskW: 34,
  leadDeskW: 40,
  bottomPad: 36,
  minHeight: WORLD_H,
  spriteScale: 1,
  seatedBaseline: SEATED_BASELINE,
  cols: 3,
};

function compactSpec(cols: number): DensitySpec {
  return {
    density: "compact",
    top: 24,
    leadDeskOff: 26,
    rowGap: 48,
    padX: 8,
    pitch: 42,
    deskW: 17,
    leadDeskW: 20,
    bottomPad: 14,
    minHeight: 110,
    spriteScale: 0.5,
    seatedBaseline: 16,
    cols,
  };
}

const MINI_SPEC: DensitySpec = {
  density: "mini",
  top: 12,
  leadDeskOff: 12,
  rowGap: 18,
  padX: 4,
  pitch: 14,
  deskW: 10,
  leadDeskW: 12,
  bottomPad: 8,
  minHeight: 46,
  spriteScale: 0,
  seatedBaseline: 7,
  cols: 6,
};

interface ShellMetrics {
  shaftW: number;
  rightW: number;
  roofH: number;
  slabH: number;
  groundH: number;
}

const SHELL_METRICS: Record<BuildingShell, ShellMetrics> = {
  none: { shaftW: 0, rightW: 0, roofH: 0, slabH: 0, groundH: 0 },
  floors: { shaftW: 22, rightW: 5, roofH: 20, slabH: 4, groundH: 12 },
  tower: { shaftW: 16, rightW: 4, roofH: 24, slabH: 3, groundH: 12 },
};

const SCALE_FULL = [4, 3, 2] as const;
const SCALE_COMPACT = [3, 2.5, 2, 1.5] as const;
const SCALE_MINI = [3, 2.5, 2, 1.5, 1] as const;
const VIEWPORT_PAD = 12;
const BUILDING_GAP = 14;
const MAX_TOWER_ZONES_PER_FLOOR = 5;
const MAX_CAMPUS_BUILDINGS = 5;

export interface LayoutZone {
  id: string;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
  topH: number;
  building: number;
  /** 0-based floor index from bottom (0 = 1F). */
  floor: number;
  hasDivider: boolean;
}

export interface LayoutFloor {
  building: number;
  /** 0-based floor index from bottom (0 = 1F). */
  floor: number;
  y: number;
  h: number;
}

export interface LayoutBuilding {
  index: number;
  x: number;
  y: number;
  w: number;
  h: number;
  shell: BuildingShell;
  shaftW: number;
  roofH: number;
  groundH: number;
  slabH: number;
  floors: LayoutFloor[];
}

export interface Seat {
  agentId: string;
  x: number;
  y: number;
  deskX: number;
  deskY: number;
  deskW: number;
  hitX: number;
  hitY: number;
  hitW: number;
  hitH: number;
  isLead: boolean;
  zone: number;
}

export interface Layout {
  tier: OfficeTier;
  density: OfficeDensity;
  scale: number;
  width: number;
  height: number;
  zones: LayoutZone[];
  buildings: LayoutBuilding[];
  seats: Seat[];
}

interface DeptGroup {
  id: string;
  name: string;
  lead: OfficePerson | null;
  staff: OfficePerson[];
}

function groupDepartments(snapshot: OfficeSnapshot): DeptGroup[] {
  const zoneIds = snapshot.departments.map((d) => d.id);
  if (snapshot.people.some((p) => !p.departmentId || !zoneIds.includes(p.departmentId))) {
    zoneIds.push("__none");
  }
  return zoneIds.map((id) => {
    const people = snapshot.people.filter(
      (p) => (p.departmentId && zoneIds.includes(p.departmentId) ? p.departmentId : "__none") === id,
    );
    const lead = people.find((p) => p.isLead) ?? null;
    const staff = people.filter((p) => p !== lead);
    return {
      id,
      name: snapshot.departments.find((d) => d.id === id)?.name ?? "其他",
      lead,
      staff,
    };
  });
}

function zoneWidth(spec: DensitySpec): number {
  if (spec.density === "full") return ZONE_W;
  return spec.padX * 2 + spec.cols * spec.pitch;
}

function zoneHeight(dept: DeptGroup, spec: DensitySpec): number {
  const leadRows = dept.lead ? 1 : 0;
  const staffRows = dept.staff.length > 0 ? Math.ceil(dept.staff.length / spec.cols) : 0;
  const totalRows = leadRows + staffRows;
  if (totalRows === 0) return spec.minHeight;
  const lastDeskY = spec.top + spec.leadDeskOff + (totalRows - 1) * spec.rowGap;
  return Math.max(spec.minHeight, lastDeskY + spec.bottomPad);
}

interface BuildingPlan {
  w: number;
  h: number;
  shell: BuildingShell;
  spec: DensitySpec;
  rows: Array<{ depts: DeptGroup[]; h: number }>;
}

function planBuilding(depts: DeptGroup[], spec: DensitySpec, zonesPerFloor: number, shell: BuildingShell): BuildingPlan {
  const m = SHELL_METRICS[shell];
  const zw = zoneWidth(spec);
  const perFloor = Math.max(1, zonesPerFloor);
  const rows: Array<{ depts: DeptGroup[]; h: number }> = [];
  for (let i = 0; i < depts.length; i += perFloor) {
    const slice = depts.slice(i, i + perFloor);
    const h = Math.max(spec.minHeight, ...slice.map((d) => zoneHeight(d, spec)));
    rows.push({ depts: slice, h });
  }
  const colsUsed = Math.max(1, Math.min(perFloor, depts.length));
  const w = m.shaftW + colsUsed * zw + m.rightW;
  const floorsH = rows.reduce((acc, row) => acc + row.h, 0) + Math.max(0, rows.length - 1) * m.slabH;
  const h = Math.max(spec.minHeight, m.roofH + floorsH + m.groundH);
  return { w, h, shell, spec, rows };
}

function materializeLayout(tier: OfficeTier, scale: number, plans: BuildingPlan[]): Layout {
  const totalH = Math.max(1, ...plans.map((p) => p.h));
  const totalW = plans.length === 0
    ? ZONE_W
    : plans.reduce((acc, p, idx) => acc + p.w + (idx > 0 ? BUILDING_GAP : 0), 0);

  const zones: LayoutZone[] = [];
  const buildings: LayoutBuilding[] = [];
  const seats: Seat[] = [];
  const density = plans[0]?.spec.density ?? "full";

  let bx = 0;
  plans.forEach((plan, bi) => {
    const m = SHELL_METRICS[plan.shell];
    const by = totalH - plan.h;
    const zw = zoneWidth(plan.spec);
    const floors: LayoutFloor[] = [];
    let fy = by + m.roofH;

    plan.rows.forEach((row, ri) => {
      const floorNum = plan.rows.length - 1 - ri;
      floors.push({ building: bi, floor: floorNum, y: fy, h: row.h });

      row.depts.forEach((dept, ci) => {
        const zx = bx + m.shaftW + ci * zw;
        const zoneIndex = zones.length;
        zones.push({
          id: dept.id,
          name: dept.name,
          x: zx,
          y: fy,
          w: zw,
          h: row.h,
          topH: plan.spec.top,
          building: bi,
          floor: floorNum,
          hasDivider: ci > 0,
        });

        const spec = plan.spec;
        const pushSeat = (person: OfficePerson, deskX: number, deskY: number, isLead: boolean) => {
          const deskW = isLead ? spec.leadDeskW : spec.deskW;
          if (spec.density === "mini") {
            const sx = deskX + Math.round((deskW - 8) / 2);
            const sy = deskY - spec.seatedBaseline;
            seats.push({
              agentId: person.agentId,
              x: sx,
              y: sy,
              deskX,
              deskY,
              deskW,
              hitX: deskX - 1,
              hitY: sy - 2,
              hitW: deskW + 2,
              hitH: spec.seatedBaseline + 7,
              isLead,
              zone: zoneIndex,
            });
            return;
          }
          const spriteW = SPRITE_WIDTH * spec.spriteScale;
          const spriteH = SPRITE_HEIGHT * spec.spriteScale;
          const sx = deskX + (deskW - spriteW) / 2;
          const sy = deskY - spec.seatedBaseline;
          seats.push({
            agentId: person.agentId,
            x: sx,
            y: sy,
            deskX,
            deskY,
            deskW,
            hitX: Math.min(sx, deskX),
            hitY: sy,
            hitW: Math.max(spriteW, deskW),
            hitH: Math.max(spriteH, deskY + 14 * spec.spriteScale - sy),
            isLead,
            zone: zoneIndex,
          });
        };

        if (dept.lead) {
          const deskX = zx + Math.round((zw - spec.leadDeskW) / 2);
          const deskY = fy + spec.top + spec.leadDeskOff;
          pushSeat(dept.lead, deskX, deskY, true);
        }

        dept.staff.forEach((person, si) => {
          const col = si % spec.cols;
          const staffRow = Math.floor(si / spec.cols);
          const deskX = spec.density === "full"
            ? zx + spec.padX + col * spec.pitch
            : zx + spec.padX + col * spec.pitch + Math.round((spec.pitch - spec.deskW) / 2);
          const deskY = fy + spec.top + spec.leadDeskOff + (staffRow + (dept.lead ? 1 : 0)) * spec.rowGap;
          pushSeat(person, deskX, deskY, false);
        });
      });

      fy += row.h + m.slabH;
    });

    buildings.push({
      index: bi,
      x: bx,
      y: by,
      w: plan.w,
      h: plan.h,
      shell: plan.shell,
      shaftW: m.shaftW,
      roofH: m.roofH,
      groundH: m.groundH,
      slabH: m.slabH,
      floors,
    });
    bx += plan.w + BUILDING_GAP;
  });

  return { tier, density, scale, width: totalW, height: totalH, zones, buildings, seats };
}

function fitScale(w: number, h: number, availW: number, availH: number, steps: readonly number[]): number {
  for (const s of steps) {
    if (w * s <= availW && h * s <= availH) return s;
  }
  return 0;
}

/**
 * Pure layout: places departments and seats into a growing building that
 * adapts to the viewport (`room → dense → floors → tower → campus`).
 * When `viewport` is omitted (e.g. unmeasured DOM or legacy callers), falls back
 * to the single-row full-density `room` layout at scale 2.
 */
export function layoutOffice(snapshot: OfficeSnapshot, viewport?: OfficeViewport | null): Layout {
  const depts = groupDepartments(snapshot);
  if (depts.length === 0) {
    const emptyPlan = planBuilding([], FULL_SPEC, 1, "none");
    return materializeLayout("room", 2, [emptyPlan]);
  }

  const hasViewport = Boolean(viewport && viewport.width > 0 && viewport.height > 0);
  if (!hasViewport) {
    const plan = planBuilding(depts, FULL_SPEC, depts.length, "none");
    return materializeLayout("room", 2, [plan]);
  }

  const availW = Math.max(120, viewport!.width - VIEWPORT_PAD);
  const availH = Math.max(120, viewport!.height - VIEWPORT_PAD);
  const maxStaff = Math.max(0, ...depts.map((d) => d.staff.length));
  const compactCols = maxStaff > 6 ? 4 : 3;
  const cmpSpec = compactSpec(compactCols);

  // 1. Single-row full office (inline nameplates, crisp integer scale ≥ 2).
  {
    const plan = planBuilding(depts, FULL_SPEC, depts.length, "none");
    const s = fitScale(plan.w, plan.h, availW, availH, SCALE_FULL);
    if (s > 0) return materializeLayout("room", s, [plan]);
  }

  // 2. Single-row compact office (higher density before stacking floors).
  {
    const singleCompactCols = depts.length === 1 && maxStaff > 9 ? 5 : compactCols;
    const plan = planBuilding(depts, compactSpec(singleCompactCols), depts.length, "none");
    const s = fitScale(plan.w, plan.h, availW, availH, SCALE_COMPACT);
    if (s > 0) return materializeLayout("dense", s, [plan]);
  }

  // 3. Multi-floor compact building (2+ floors with elevator shaft & roof).
  if (depts.length > 1) {
    let best: { s: number; plan: BuildingPlan; floors: number } | null = null;
    for (let perFloor = depts.length - 1; perFloor >= 1; perFloor--) {
      const plan = planBuilding(depts, cmpSpec, perFloor, "floors");
      const s = fitScale(plan.w, plan.h, availW, availH, SCALE_COMPACT);
      if (s > 0 && (!best || s > best.s || (s === best.s && plan.rows.length < best.floors))) {
        best = { s, plan, floors: plan.rows.length };
      }
    }
    if (best) return materializeLayout("floors", best.s, [best.plan]);
  }

  // 4. Single office tower (mini sprites, 6 columns per room, max 5 rooms per floor).
  {
    let best: { s: number; plan: BuildingPlan; aspectDiff: number } | null = null;
    const targetAspect = availW / availH;
    const maxPerFloor = Math.min(depts.length, MAX_TOWER_ZONES_PER_FLOOR);
    for (let perFloor = maxPerFloor; perFloor >= 1; perFloor--) {
      const plan = planBuilding(depts, MINI_SPEC, perFloor, "tower");
      const s = fitScale(plan.w, plan.h, availW, availH, SCALE_MINI);
      const aspectDiff = Math.abs(plan.w / plan.h - targetAspect);
      if (s > 0 && (!best || s > best.s || (s === best.s && aspectDiff < best.aspectDiff))) {
        best = { s, plan, aspectDiff };
      }
    }
    if (best) return materializeLayout("tower", best.s, [best.plan]);
  }

  // 5. Campus (2..5 towers side by side).
  const maxBuildings = Math.min(MAX_CAMPUS_BUILDINGS, depts.length);
  let fallback: { s: number; plans: BuildingPlan[]; tier: OfficeTier } | null = null;

  for (let k = 2; k <= maxBuildings; k++) {
    const perBuilding = Math.ceil(depts.length / k);
    const chunks: DeptGroup[][] = [];
    for (let i = 0; i < depts.length; i += perBuilding) {
      chunks.push(depts.slice(i, i + perBuilding));
    }
    let best: { s: number; plans: BuildingPlan[] } | null = null;
    for (let perFloor = Math.min(perBuilding, MAX_TOWER_ZONES_PER_FLOOR); perFloor >= 1; perFloor--) {
      const plans = chunks.map((chunk) => planBuilding(chunk, MINI_SPEC, perFloor, "tower"));
      const w = plans.reduce((acc, p, idx) => acc + p.w + (idx > 0 ? BUILDING_GAP : 0), 0);
      const h = Math.max(...plans.map((p) => p.h));
      const s = fitScale(w, h, availW, availH, SCALE_MINI);
      if (s > 0 && (!best || s > best.s)) best = { s, plans };
      const continuous = Math.max(0.5, Math.min(availW / w, availH / h));
      if (!fallback || continuous > fallback.s) {
        fallback = { s: continuous, plans, tier: "campus" };
      }
    }
    if (best) return materializeLayout("campus", best.s, best.plans);
  }

  // Extreme small viewport fallback: fit single tower or campus continuously.
  const singleTower = planBuilding(depts, MINI_SPEC, Math.max(1, Math.ceil(Math.sqrt(depts.length))), "tower");
  const singleScale = Math.max(0.5, Math.min(availW / singleTower.w, availH / singleTower.h));
  if (!fallback || singleScale >= fallback.s) {
    return materializeLayout("tower", singleScale, [singleTower]);
  }
  return materializeLayout(fallback.tier, fallback.s, fallback.plans);
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
  /** normalized hit-box fractions (0..1) for interactive overlays in compact/mini modes */
  hitLeft: number;
  hitTop: number;
  hitWidth: number;
  hitHeight: number;
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
    const centerX = seat.hitX + seat.hitW / 2;
    const labelY = layout.density === "full" ? seat.deskY + 18 : seat.hitY + seat.hitH;
    return [{
      agentId: p.agentId,
      name: p.name,
      model: p.model,
      title: p.title,
      anim: p.anim,
      bubble: p.bubble ?? null,
      pendingCards: p.pendingCards,
      left: centerX / layout.width,
      top: labelY / layout.height,
      bubbleTop: Math.max(4, seat.hitY - 6) / layout.height,
      hitLeft: seat.hitX / layout.width,
      hitTop: seat.hitY / layout.height,
      hitWidth: seat.hitW / layout.width,
      hitHeight: seat.hitH / layout.height,
      isLead: p.isLead,
    }];
  });
}

type Walker = { agentId: string; path: Array<[number, number]>; i: number; x: number; y: number; back: boolean };

const MINI_DIGITS: Record<string, readonly string[]> = {
  "0": ["111", "101", "101", "101", "111"],
  "1": ["010", "110", "010", "010", "111"],
  "2": ["111", "001", "111", "100", "111"],
  "3": ["111", "001", "111", "001", "111"],
  "4": ["101", "101", "111", "001", "001"],
  "5": ["111", "100", "111", "001", "111"],
  "6": ["111", "100", "111", "101", "111"],
  "7": ["111", "001", "001", "001", "001"],
  "8": ["111", "101", "111", "101", "111"],
  "9": ["111", "101", "111", "001", "111"],
  F: ["111", "100", "110", "100", "100"],
  A: ["010", "101", "111", "101", "101"],
  B: ["110", "101", "110", "101", "110"],
  C: ["111", "100", "100", "100", "111"],
  D: ["110", "101", "101", "101", "110"],
  E: ["111", "100", "110", "100", "111"],
};

export class OfficeRenderer {
  private ctx: CanvasRenderingContext2D;
  private snapshot: OfficeSnapshot = { at: "", departments: [], people: [] };
  private viewport: OfficeViewport | null = null;
  private layout: Layout = layoutOffice(this.snapshot);
  private raf = 0;
  private last = 0;
  private walkers = new Map<string, Walker>();
  private seenWalk = new Set<string>();
  selected: string | null = null;
  reducedMotion = false;
  private skin: OfficeSkin = OFFICE_SKINS.office;
  private palette: OfficePalette = OFFICE_SKINS.office.light;
  private dark = false;
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
    this.dark = dark;
    this.palette = dark ? skin.dark : skin.light;
  }

  setSnapshot(s: OfficeSnapshot, viewport?: OfficeViewport | null) {
    this.snapshot = s;
    if (viewport !== undefined) this.viewport = viewport;
    this.layout = layoutOffice(s, this.viewport);
    this.syncBackingSize();
    this.syncWalkers(s);
  }

  setLayout(layout: Layout) {
    this.layout = layout;
    this.syncBackingSize();
    this.syncWalkers(this.snapshot);
  }

  private backingScale(): number {
    return this.layout.density === "compact" ? 2 : 1;
  }

  private syncBackingSize() {
    const backing = this.backingScale();
    const bw = Math.max(1, Math.ceil(this.layout.width * backing));
    const bh = Math.max(1, Math.ceil(this.layout.height * backing));
    if (this.canvas.width !== bw || this.canvas.height !== bh) {
      this.canvas.width = bw;
      this.canvas.height = bh;
    }
  }

  private syncWalkers(s: OfficeSnapshot) {
    const allowWalk = this.layout.density === "full" && !this.reducedMotion;
    for (const id of this.seenWalk) {
      if (!allowWalk || !s.people.some((p) => p.agentId === id && p.anim === "walk_to_lead")) {
        this.seenWalk.delete(id);
        this.walkers.delete(id);
      }
    }
    if (!allowWalk) return;
    // one-off delivery walk to the lead
    for (const p of s.people) {
      if (p.anim !== "walk_to_lead") {
        this.seenWalk.delete(p.agentId);
        continue;
      }
      if (this.seenWalk.has(p.agentId)) continue;
      this.seenWalk.add(p.agentId);
      const seat = this.layout.seats.find((x) => x.agentId === p.agentId);
      const lead = this.layout.seats.find((x) => x.isLead && x.zone === seat?.zone);
      const zone = seat ? this.layout.zones[seat.zone] : null;
      if (seat && lead && zone) {
        const midY = lead.deskY + 22;
        const aisleX = Math.min(seat.deskX + 42, zone.x + zone.w - SPRITE_WIDTH - 2);
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
      const hitX = w ? w.x : (seat?.hitX ?? 0);
      const hitY = w ? w.y : (seat?.hitY ?? 0);
      const hitW = w ? SPRITE_WIDTH : (seat?.hitW ?? SPRITE_WIDTH);
      const hitH = w ? SPRITE_HEIGHT : (seat?.hitH ?? SPRITE_HEIGHT);
      return {
        agentId: p.agentId,
        name: p.name,
        model: p.model,
        title: p.title,
        anim: p.anim,
        bubble: p.bubble ?? null,
        pendingCards: p.pendingCards,
        left: (hitX + hitW / 2) / this.layout.width,
        top: (w || !seat
          ? y + SPRITE_HEIGHT + 5
          : this.layout.density === "full" ? seat.deskY + 18 : hitY + hitH) / this.layout.height,
        bubbleTop: Math.max(4, hitY - 6) / this.layout.height,
        hitLeft: hitX / this.layout.width,
        hitTop: hitY / this.layout.height,
        hitWidth: hitW / this.layout.width,
        hitHeight: hitH / this.layout.height,
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
      if (w) {
        if (x >= w.x && x <= w.x + SPRITE_WIDTH && y >= w.y && y <= w.y + SPRITE_HEIGHT) return s.agentId;
        continue;
      }
      if (x >= s.hitX && x <= s.hitX + s.hitW && y >= s.hitY && y <= s.hitY + s.hitH) return s.agentId;
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

  private drawMiniText(text: string, x: number, y: number, color: string) {
    let cx = Math.round(x);
    const cy = Math.round(y);
    for (const ch of text) {
      const glyph = MINI_DIGITS[ch];
      if (!glyph) {
        cx += 3;
        continue;
      }
      for (let r = 0; r < glyph.length; r++) {
        for (let c = 0; c < glyph[r]!.length; c++) {
          if (glyph[r]![c] === "1") this.px(color, cx + c, cy + r, 1, 1);
        }
      }
      cx += 4;
    }
  }

  private frame(t: number) {
    const dt = Math.min(64, t - (this.last || t));
    this.last = t;
    if (!this.reducedMotion) this.stepWalkers(dt);
    if (this.reducedMotion) t = 0;

    const backing = this.backingScale();
    this.ctx.setTransform(backing, 0, 0, backing, 0, 0);
    this.ctx.imageSmoothingEnabled = false;

    const P = this.palette;
    const { width: W, height: H, density, buildings, zones } = this.layout;
    const hasShell = buildings.some((b) => b.shell !== "none");

    if (hasShell) {
      this.drawBackdrop(W, H, t);
    } else {
      this.ctx.clearRect(0, 0, W, H);
    }

    for (const b of buildings) {
      if (b.shell !== "none") this.drawBuildingShell(b, buildings.length, t);
    }

    for (let zi = 0; zi < zones.length; zi++) {
      this.drawZone(zones[zi]!, zi, density, t);
    }

    const byId = new Map(this.snapshot.people.map((p) => [p.agentId, p]));
    for (const s of this.layout.seats) {
      const p = byId.get(s.agentId);
      if (!p) continue;
      const walking = this.walkers.has(s.agentId);
      if (density === "mini") {
        this.drawMiniSeat(s, p, t);
      } else if (density === "compact") {
        this.drawCompactSeat(s, p, t);
      } else {
        this.px(P.chair, s.deskX + 12, s.deskY - 20, 18, 21);
        if (!walking) this.drawRobot(s.x, s.y, p, t);
        this.drawDesk(s.deskX, s.deskY, walking ? "idle" : p.anim, t, s.isLead);
      }
    }

    for (const w of this.walkers.values()) {
      const p = byId.get(w.agentId);
      if (p) this.drawRobot(w.x, w.y, p, t, true);
    }

    if (this.selected) {
      const s = this.layout.seats.find((x) => x.agentId === this.selected);
      const w = this.walkers.get(this.selected ?? "");
      if (s) {
        const b = this.reducedMotion ? 0 : Math.floor(t / 300) % 2;
        if (density === "mini") {
          this.px(P.accent, s.hitX, s.hitY - 2 + b, s.hitW, 1);
          this.px(P.accent, s.hitX + Math.floor(s.hitW / 2) - 1, s.hitY - 3 + b, 2, 1);
        } else if (density === "compact") {
          const cx = s.hitX + Math.round(s.hitW / 2);
          this.px(P.accent, cx - 2, s.hitY - 5 + b, 4, 1);
          this.px(P.accent, cx - 1, s.hitY - 4 + b, 2, 1);
        } else {
          const x = w ? w.x : s.x;
          const y = w ? w.y : s.y;
          this.px(P.accent, x + 21, y - 10 + b, 6, 2);
          this.px(P.accent, x + 22, y - 8 + b, 4, 1);
          this.px(P.accent, x + 23, y - 7 + b, 2, 1);
        }
      }
    }
  }

  private drawBackdrop(W: number, H: number, t: number) {
    const P = this.palette;
    const sky = this.skin.id === "starship"
      ? P.sky
      : this.dark ? lerpColor(P.wall, P.sky, 0.35) : lerpColor(P.wall, P.sky, 0.45);
    this.px(sky, 0, 0, W, H);
    if (this.skin.id === "starship") {
      for (let i = 0; i < 24; i++) {
        const sx = (i * 53 + Math.floor(t / 300)) % Math.max(1, W);
        const sy = (i * 29) % Math.max(1, H - 12);
        this.px("#ffffff", sx, sy, 1, 1);
      }
    } else if (!this.dark) {
      // subtle drifting pixel clouds in daylight
      for (let i = 0; i < 4; i++) {
        const cx = ((i * 97 + Math.floor(t / 400)) % (W + 30)) - 15;
        const cy = 4 + (i % 2) * 8;
        this.px("#ffffff", cx, cy, 12, 2);
        this.px("#ffffff", cx + 3, cy - 2, 6, 2);
      }
    }
    this.px(P.trim, 0, H - 4, W, 4);
  }

  private drawBuildingShell(b: LayoutBuilding, totalBuildings: number, t: number) {
    const P = this.palette;
    const facade = lerpColor(P.wallDark, P.trim, 0.45);
    const shaftBg = this.dark ? lerpColor(P.wall, P.desk, 0.45) : lerpColor(P.wallDark, P.trim, 0.7);
    const slab = this.skin.id === "court" ? P.trim : lerpColor(P.trim, P.shelf, 0.35);
    const roofColor = this.skin.id === "court" ? P.trim : P.shelf;
    const signBg = this.skin.id === "court" ? P.shelf : P.desk;

    // Exterior shell behind all rooms & shaft
    this.px(facade, b.x, b.y + b.roofH - 2, b.w, b.h - b.roofH + 2 - b.groundH);

    // Roof cornice & rooftop signboard
    this.px(roofColor, b.x - 2, b.y + b.roofH - 5, b.w + 4, 4);
    const signW = Math.min(b.w - 10, b.shell === "tower" ? 36 : 48);
    const signX = b.x + Math.round((b.w - signW) / 2);
    const signY = b.y + b.roofH - 14;
    this.px(signBg, signX, signY, signW, 8);
    this.px(roofColor, signX + 3, signY + 8, 2, 2);
    this.px(roofColor, signX + signW - 5, signY + 8, 2, 2);
    if (totalBuildings > 1) {
      const wing = String.fromCharCode(65 + (b.index % 5));
      this.drawMiniText(wing, signX + Math.floor((signW - 3) / 2), signY + 2, P.accent);
    } else {
      // decorative glowing bar on rooftop sign
      this.px(P.accent, signX + 4, signY + 3, signW - 8, 2);
    }

    // Antenna on towers
    if (b.shell === "tower") {
      const ax = signX + signW - 6;
      this.px(roofColor, ax, b.y + 3, 1, Math.max(2, signY - (b.y + 3)));
      const beacon = (!this.reducedMotion && Math.floor(t / 600) % 2 === 0) ? P.accent : P.screenOn;
      this.px(beacon, ax - 1, b.y + 1, 3, 2);
    }

    // Elevator shaft & floor labels
    this.px(shaftBg, b.x, b.y + b.roofH, b.shaftW, b.h - b.roofH - b.groundH);
    for (let i = 0; i < b.floors.length; i++) {
      const f = b.floors[i]!;
      const label = `${f.floor + 1}F`;
      const textW = label.length * 4 - 1;
      const tx = b.x + Math.max(1, Math.floor((b.shaftW - textW) / 2));
      const floorLabelColor = this.dark ? lerpColor(P.trim, P.code, 0.55) : P.desk;
      this.drawMiniText(label, tx, f.y + 3, floorLabelColor);
      // Elevator door near bottom of floor
      const doorH = Math.min(f.h - 10, b.shell === "tower" ? 10 : 16);
      const doorW = Math.min(b.shaftW - 6, b.shell === "tower" ? 8 : 10);
      if (doorH >= 6 && doorW >= 6) {
        const dx = b.x + Math.floor((b.shaftW - doorW) / 2);
        const dy = f.y + f.h - doorH;
        this.px(P.trim, dx - 1, dy - 1, doorW + 2, doorH + 1);
        this.px(P.screenIdle, dx, dy, doorW, doorH);
        this.px(P.trim, dx + Math.floor(doorW / 2), dy, 1, doorH);
      }
      if (i < b.floors.length - 1) {
        this.px(slab, b.x, f.y + f.h, b.w, b.slabH);
      }
    }

    // Ground floor lobby strip & entrance doors
    const gy = b.y + b.h - b.groundH;
    this.px(slab, b.x, gy, b.w, 2);
    this.px(shaftBg, b.x, gy + 2, b.w, b.groundH - 2);
    const lobbyW = Math.min(16, Math.max(8, Math.floor(b.w * 0.14)));
    const lobbyX = b.x + Math.floor((b.w - lobbyW) / 2);
    this.px(P.screenIdle, lobbyX, gy + 3, lobbyW, b.groundH - 3);
    this.px(P.trim, lobbyX + Math.floor(lobbyW / 2), gy + 3, 1, b.groundH - 3);
  }

  private drawZone(z: LayoutZone, zi: number, density: OfficeDensity, t: number) {
    const P = this.palette;
    const skin = this.skin;
    const d = this.painter;
    const topH = z.topH;

    // Floor tiles clipped to zone
    const tile = density === "mini" ? 4 : density === "compact" ? Math.max(6, Math.round(skin.floorTile * 0.6)) : skin.floorTile;
    for (let iy = 0; iy * tile + topH < z.h; iy++) {
      for (let ix = 0; ix * tile < z.w; ix++) {
        const tx = z.x + ix * tile;
        const ty = z.y + topH + iy * tile;
        const tw = Math.min(tile, z.x + z.w - tx);
        const th = Math.min(tile, z.y + z.h - ty);
        this.px((ix + iy) % 2 ? P.floorA : P.floorB, tx, ty, tw, th);
      }
    }

    // Wall
    if (density === "full") {
      this.px(P.wall, z.x, z.y, z.w, 38);
      this.px(P.wallDark, z.x, z.y + 36, z.w, 8);
      this.px(P.trim, z.x, z.y + 44, z.w, 2);
      if (z.hasDivider) this.px(P.trim, z.x, z.y + topH, 1, z.h - topH);
      this.ctx.save();
      this.ctx.translate(z.x, z.y);
      [14, 104].forEach((wx, i) => skin.window(d, P, wx, t, i));
      skin.centerpiece(d, P, 58, t);
      skin.corner(d, P, z.w - 14, topH + 4, t);
      this.ctx.restore();
    } else if (density === "compact") {
      this.px(P.wall, z.x, z.y, z.w, 19);
      this.px(P.wallDark, z.x, z.y + 18, z.w, 4);
      this.px(P.trim, z.x, z.y + 22, z.w, 2);
      if (z.hasDivider) this.px(P.trim, z.x, z.y + topH, 1, z.h - topH);
      this.ctx.save();
      this.ctx.translate(z.x, z.y);
      this.ctx.scale(0.5, 0.5);
      const scaledW = z.w * 2;
      skin.window(d, P, 14, t, 0);
      if (scaledW >= 180) skin.centerpiece(d, P, Math.round(scaledW / 2 - 16), t);
      if (scaledW >= 240) skin.window(d, P, scaledW - 50, t, 1);
      skin.corner(d, P, scaledW - 16, topH * 2 + 4, t);
      this.ctx.restore();
    } else {
      this.px(P.wall, z.x, z.y, z.w, topH - 2);
      this.px(P.wallDark, z.x, z.y + topH - 2, z.w, 1);
      this.px(P.trim, z.x, z.y + topH - 1, z.w, 1);
      if (z.hasDivider) this.px(P.trim, z.x, z.y + topH, 1, z.h - topH);
    }

    // Lead rug
    const lead = this.layout.seats.find((s) => s.isLead && s.zone === zi);
    if (lead) {
      if (density === "full") {
        const rx = lead.deskX - 8, ry = lead.deskY - 22, rw = 56, rh = 40;
        this.px(P.rug, rx, ry, rw, rh);
        for (let i = 0; i < rw; i++) {
          const c = lerpColor(skin.leadOutline[0], skin.leadOutline[1], i / (rw - 1));
          this.px(c, rx + i, ry - 3, 1, 3);
          this.px(c, rx + i, ry + rh, 1, 3);
        }
        for (let j = -3; j < rh + 3; j++) {
          const frac = (j + 3) / (rh + 5);
          this.px(lerpColor(skin.leadOutline[0], skin.leadOutline[1], frac * 0.5), rx - 3, ry + j, 3, 1);
          this.px(lerpColor(skin.leadOutline[0], skin.leadOutline[1], 0.5 + frac * 0.5), rx + rw, ry + j, 3, 1);
        }
      } else if (density === "compact") {
        const rx = lead.deskX - 5, ry = lead.deskY - 12, rw = lead.deskW + 10, rh = 22;
        this.px(skin.leadOutline[0], rx - 1, ry - 1, rw + 2, rh + 2);
        this.px(P.rug, rx, ry, rw, rh);
      } else {
        const rx = lead.deskX - 2, ry = lead.deskY - 8, rw = lead.deskW + 4, rh = 13;
        this.px(skin.leadOutline[0], rx - 1, ry - 1, rw + 2, rh + 2);
        this.px(P.rug, rx, ry, rw, rh);
      }
    }
  }

  private drawCompactSeat(s: Seat, p: OfficePerson, t: number) {
    const P = this.palette;
    this.ctx.save();
    this.ctx.translate(s.deskX, s.deskY);
    this.ctx.scale(0.5, 0.5);
    this.px(P.chair, 12, -20, 18, 21);
    this.ctx.restore();

    this.ctx.save();
    this.ctx.translate(s.x, s.y);
    this.ctx.scale(0.5, 0.5);
    this.drawRobot(0, 0, p, t);
    this.ctx.restore();

    this.ctx.save();
    this.ctx.translate(s.deskX, s.deskY);
    this.ctx.scale(0.5, 0.5);
    this.drawDesk(0, 0, p.anim, t, s.isLead);
    this.ctx.restore();
  }

  private drawMiniSeat(s: Seat, p: OfficePerson, t: number) {
    const P = this.palette;
    const pal = modelFamilyColors(p.eye);
    const resting = p.anim === "paused";
    const working = p.anim === "working";
    const blink = !this.reducedMotion && Math.floor(t / 450 + s.deskX) % 2 === 1;

    // Mini character (8×7)
    this.ctx.save();
    if (resting) this.ctx.globalAlpha = 0.45;
    this.px(pal.main, s.x + 1, s.y, 6, 5);
    this.px(pal.shade, s.x + 2, s.y + 5, 4, 2);
    this.px(pal.eye, s.x + 2, s.y + 2, 1, 1);
    this.px(pal.eye, s.x + 5, s.y + 2, 1, 1);
    if (s.isLead) {
      this.px(this.skin.leadOutline[0], s.x + 2, s.y - 2, 4, 2);
    } else if (this.skin.id === "court") {
      this.px("#1B1714", s.x, s.y - 1, 8, 1);
      this.px("#1B1714", s.x + 2, s.y - 2, 4, 1);
    } else if (this.skin.id === "starship") {
      this.px(P.accent, s.x + 6, s.y - 2, 1, 2);
    }
    this.ctx.restore();

    // Mini desk & status-colored monitor
    let screen = P.screenIdle;
    if (working) screen = blink ? P.screenOn : P.code;
    else if (p.anim === "thinking") screen = "#D9A441";
    else if (p.anim === "reading") screen = "#5B7FD6";
    else if (resting) screen = P.monitor;

    this.px(P.monitor, s.deskX + 1, s.deskY - 4, 5, 4);
    this.px(screen, s.deskX + 2, s.deskY - 3, 3, 2);
    this.px(P.deskTop, s.deskX, s.deskY, s.deskW, 2);
    this.px(P.desk, s.deskX, s.deskY + 2, s.deskW, 2);
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
        const [tx, ty] = w.path[w.i]!;
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
