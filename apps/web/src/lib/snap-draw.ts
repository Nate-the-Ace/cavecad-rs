// Drawing the feedback of object snap on an overlay canvas (AutoSnap): a marker per mode, a tooltip, the guide lines, the
// track points, and the aperture box. Pure drawing: it reads a result and draws, and decides nothing.

import type { SnapMode, SnapPrefs, SnapResult, SnapSession, V3 } from './snap-types';

const TAU = Math.PI * 2;

/** Draws the marker of `mode` centred on (`x`, `y`), `s` pixels across. */
export function glyph(g: CanvasRenderingContext2D, mode: SnapMode | 'guide', x: number, y: number, s: number, col = '#ffd400') {
  g.save();
  g.strokeStyle = col;
  g.fillStyle = col;
  g.lineWidth = 2;
  g.lineJoin = 'miter';
  const R = s;
  g.beginPath();
  switch (mode) {
    case 'endpoint': g.rect(x - R, y - R, 2 * R, 2 * R); break;
    case 'midpoint': g.moveTo(x, y - R); g.lineTo(x + R, y + R * 0.8); g.lineTo(x - R, y + R * 0.8); g.closePath(); break;
    case 'center': g.arc(x, y, R, 0, TAU); break;
    case 'geometric_center': g.arc(x, y, R, 0, TAU); g.moveTo(x - R, y); g.lineTo(x + R, y); break;
    case 'quadrant': g.moveTo(x, y - R); g.lineTo(x + R, y); g.lineTo(x, y + R); g.lineTo(x - R, y); g.closePath(); break;
    case 'intersection':
    case 'extended_intersection':
      g.moveTo(x - R, y - R); g.lineTo(x + R, y + R); g.moveTo(x + R, y - R); g.lineTo(x - R, y + R);
      if (mode === 'extended_intersection') { g.moveTo(x + R * 1.5 + 1.2, y); g.arc(x + R * 1.5, y, 1.2, 0, TAU); }
      break;
    case 'apparent_intersection': g.moveTo(x - R, y - R); g.lineTo(x + R, y + R); g.moveTo(x + R, y - R); g.lineTo(x - R, y + R); g.rect(x - R, y - R, 2 * R, 2 * R); break;
    case 'extension': for (const dx of [-R, 0, R]) { g.moveTo(x + dx + 1.6, y); g.arc(x + dx, y, 1.6, 0, TAU); } break;
    case 'perpendicular': g.moveTo(x - R, y + R); g.lineTo(x + R, y + R); g.moveTo(x - R * 0.6, y + R); g.lineTo(x - R * 0.6, y - R); g.moveTo(x - R * 0.6, y + R * 0.1); g.lineTo(x + R * 0.1, y + R * 0.1); g.lineTo(x + R * 0.1, y + R); break;
    case 'tangent': g.arc(x, y, R * 0.8, 0, TAU); g.moveTo(x - R * 1.1, y - R * 0.8); g.lineTo(x + R * 1.1, y - R * 0.8); break;
    case 'parallel': g.moveTo(x - R, y + R * 0.3); g.lineTo(x + R * 0.3, y - R); g.moveTo(x - R * 0.3, y + R); g.lineTo(x + R, y - R * 0.3); break;
    case 'nearest': g.moveTo(x - R, y - R); g.lineTo(x + R, y - R); g.lineTo(x - R, y + R); g.lineTo(x + R, y + R); g.closePath(); break;
    case 'insertion': g.rect(x - R, y - R * 0.2, R * 1.4, R * 1.2); g.rect(x - R * 0.4, y - R, R * 1.4, R * 1.2); break;
    case 'node': g.arc(x, y, R * 0.8, 0, TAU); g.moveTo(x - R, y - R); g.lineTo(x + R, y + R); g.moveTo(x + R, y - R); g.lineTo(x - R, y + R); break;
    case 'station': g.arc(x, y, R, 0, TAU); g.moveTo(x + 1.5, y); g.arc(x, y, 1.5, 0, TAU); break;
    case 'tracking':
    case 'guide': g.moveTo(x - R * 0.6, y - R * 0.6); g.lineTo(x + R * 0.6, y + R * 0.6); g.moveTo(x + R * 0.6, y - R * 0.6); g.lineTo(x - R * 0.6, y + R * 0.6); break;
    default: g.rect(x - R, y - R, 2 * R, 2 * R);
  }
  g.stroke();
  g.restore();
}

/** What drawing the feedback needs to know about the view. */
export interface DrawContext {
  /** The cursor, pixels. */
  cursor: [number, number];
  aperture: number;
  width: number;
  /** Where a world point is on the screen (the view's own camera). */
  project: (p: V3) => [number, number] | null;
  prefs: SnapPrefs;
  /** The one-pick override in force, named (shown when nothing is in reach). */
  overrideName: string;
  /** Which of `best` and `others` Tab has chosen: 0 is the best. */
  tab: number;
}

function tip(g: CanvasRenderingContext2D, text: string, at: [number, number], width: number, fill = '#fffbe0') {
  g.save();
  g.font = '12px system-ui, sans-serif';
  const w = g.measureText(text).width + 12;
  const tx = Math.min(at[0] + 14, width - w - 6);
  const ty = at[1] + 18;
  g.fillStyle = fill;
  g.strokeStyle = '#555';
  g.lineWidth = 1;
  g.beginPath();
  g.rect(tx, ty, w, 20);
  g.fill();
  g.stroke();
  g.fillStyle = '#222';
  g.fillText(text, tx + 6, ty + 14);
  g.restore();
}

/**
 * Draws a snap result and the state of the pick: guide lines, track points and bases, the faint markers of what Tab
 * would cycle to, the marker and tooltip of the chosen candidate. Returns where the crosshair should be drawn (pulled onto
 * the snap point by the magnet when it is already inside the aperture).
 */
export function drawSnap(g: CanvasRenderingContext2D, result: SnapResult | null, session: SnapSession, d: DrawContext): [number, number] {
  let [cx, cy] = d.cursor;
  g.save();
  // The track points and bases the person has set (a plus), and the points of Mid Between 2 Points.
  const mark = (p: V3 | null | undefined, col: string, shape: 'plus' | 'ring') => {
    const s = p ? d.project(p) : null;
    if (!s) return;
    g.strokeStyle = col;
    g.lineWidth = 2;
    g.beginPath();
    if (shape === 'plus') { g.moveTo(s[0] - 6, s[1]); g.lineTo(s[0] + 6, s[1]); g.moveTo(s[0], s[1] - 6); g.lineTo(s[0], s[1] + 6); }
    else g.arc(s[0], s[1], 5, 0, TAU);
    g.stroke();
  };
  for (const a of session.acquired ?? []) if (a.type === 'extension' || a.type === 'track') mark(a.at, '#ffd400', 'plus');
  for (const t of session.track_points ?? []) mark(t, '#4fd17a', 'plus');
  mark(session.from_base, '#4fd17a', 'ring');
  mark(session.m2p_first, '#4fd17a', 'ring');
  if (result) {
    // Guides: dotted lines; the one the best candidate is on is solid.
    for (const guide of result.guides) {
      if (guide.px.length < 2) continue;
      g.strokeStyle = guide.kind === 'extension' ? '#ffd400' : guide.kind === 'parallel' ? '#c58bf2' : '#4fd17a';
      g.lineWidth = 1;
      g.setLineDash([3, 5]);
      g.beginPath();
      guide.px.forEach((p, i) => (i ? g.lineTo(p[0], p[1]) : g.moveTo(p[0], p[1])));
      g.stroke();
    }
    g.setLineDash([]);
    for (const c of result.others) if (c !== result.best) glyph(g, c.mode, c.screen[0], c.screen[1], 5, 'rgba(255,212,0,0.35)');
    const best = result.best ? (d.tab > 0 ? [result.best, ...result.others][d.tab % (result.others.length + 1)] : result.best) : null;
    if (best) {
      const s = best.screen;
      if (d.prefs.magnet && Math.hypot(s[0] - cx, s[1] - cy) <= d.aperture) [cx, cy] = s;
      if (d.prefs.marker) glyph(g, best.mode, s[0], s[1], 7);
      if (d.prefs.tooltip) {
        const n = result.others.length + 1;
        tip(g, best.label + (n > 1 ? `   [Tab ${(d.tab % n) + 1}/${n}]` : ''), s, d.width);
      }
    } else if (d.overrideName) {
      g.font = '12px system-ui, sans-serif';
      g.fillStyle = '#ff8a65';
      g.fillText(`No ${d.overrideName} here`, cx + 16, cy + 22);
    }
  }
  g.restore();
  return [cx, cy];
}

/** The aperture box and crosshair (drawn while a pick is wanted and snapping is on). */
export function drawAperture(g: CanvasRenderingContext2D, at: [number, number], aperture: number) {
  g.save();
  g.strokeStyle = 'rgba(224,230,236,0.55)';
  g.lineWidth = 1;
  g.strokeRect(at[0] - aperture, at[1] - aperture, 2 * aperture, 2 * aperture);
  g.restore();
}
