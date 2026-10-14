<script lang="ts">
  // Cross Section: the passage cut across at a station, drawn from the walls measured there (the
  // same ring the 3D tube is built from), with a pen or mouse to sketch the real shape on top.
  // Sections live at stations: pick one here (or click a station on the map). Sketches belong to
  // their station and follow it when the survey is adjusted.
  import { drawHalo, type HaloLine } from '../lib/halo';
  import { onMount, untrack } from 'svelte';
  import { fitRect, registerMirror, roundRect, visibleRect, type Rect } from '../lib/classroom-mirror';
  import { classroom } from '../lib/classroom.svelte';
  import { onTheme } from '../lib/theme';
  import { Maximize, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { looks } from '../lib/ink';
  import { drawHandles, drawStrokes, ornamentFor } from '../lib/section-glyph';
  import { LineEditor, endpointNear, joinFit } from '../lib/line-editor';
  import { Touches } from '../lib/touch';
  import { PenGuard, isEraser, widthFor } from '../lib/pen';
  import { drawStationMarker, isStampKind, leaderLook, liveWords, needsWords, stampEditSpot, withLabelSpot } from '../lib/symbols';
  import { isFillKind } from '../lib/fills';
  import { cursorsFor, live as room, reportCursor } from '../lib/sync.svelte';
  import { drawCursor } from '../lib/presence-draw';
  import { fitStroke, smooth } from '../lib/spline';
  import { drawing, liveStyle } from '../lib/line-style.svelte';
  import { tolerancePx } from '../lib/commands';
  import { markViewUsed, registerSelectAll } from '../lib/select-all';
  import { copyGhost, copyOp, copyPress, endCopy, registerClipView, snapToSelection } from '../lib/view-clip.svelte';
  import { paintCopy } from '../lib/copy-ghost';
  import type { Drawn } from '../lib/clip';
  import { openNewText, openTextEdit } from '../lib/text-edit.svelte';
  import { openLineMenu } from '../lib/line-menu.svelte';
  import { cave, changed, option, selectedIds, selectIds, toggleSelected, filter, wireFilter, haloFade, haloReach, toolStyle } from '../lib/cave.svelte';
  import { drawScan } from '../lib/scan-draw';
  import { givePoint, have, scanImage, scans, waitingForPoint } from '../lib/scans.svelte';
  import { engine, loadEngine, type ScanPlacement, type SectionStation, type StationSection } from '../lib/engine';
  import { metresPer, niceNumber, unitName } from '../lib/units';

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let ready = $state(false);
  let stations = $state.raw<SectionStation[]>([]);
  let section = $state.raw<StationSection | null>(null);
  let message = $state('');
  let barH = $state(50);

  // View: pixels per metre, and where the station (the origin) sits on the canvas.
  let scale = 20;
  let ox = 0;
  let oy = 0;
  let frame = 0;
  // In a class the teacher's view is mirrored: what is on show is the part of the section in the window, so a window of
  // another size shows the same part.
  $effect(() =>
    registerMirror('section', {
      read: () => (width > 0 && scale > 0 ? { rect: roundRect(visibleRect(scale, ox, oy, width, height)) } : undefined),
      write: (state) => {
        const rect = (state as { rect?: Rect }).rect;
        if (!rect || width === 0) return;
        ({ scale, ox, oy } = fitRect(rect, width, height));
        schedule();
      },
    })
  );
  let framed = '';
  let live: { pts: [number, number][]; widths: number[]; pressure: boolean } | null = null;

  // Drawn lines can be selected and their fit points moved, added and deleted.
  const editor = new LineEditor({
    lines: () => section?.sketches ?? [],
    toScreen: (p) => toScreen(p[0], p[1]),
    toWorld: (x, y) => toWorld(x, y),
    selected: () => (selectedIds().length === 1 ? cave.selected : ''),
    ids: () => selectedIds(),
    arrowOp: (id) => (cave.arrowOp.id === id ? cave.arrowOp.op : ''),
    arrowDone: () => (cave.arrowOp = { id: '', op: '' }),
    commitLeader: (id, pts, style) => {
      const r = engine.editLeader(id, pts, style, cave.adjusted);
      message = r === 'ok' ? '' : r;
      changed();
    },
    select: (id, additive) => (additive ? toggleSelected(id) : selectIds(id ? [id] : [])),
    commit: (id, pts, widths) => {
      const r = engine.editLine(id, pts, widths, cave.adjusted);
      message = r === 'ok' ? '' : r;
      changed();
    },
    remove: (id) => {
      engine.deleteLine(id);
      changed();
    },
    redraw: () => schedule(),
    paperPx: () => (scale * drawing.scale) / 1000,
    ornament: (l) => ornamentFor(l.kind ?? '', l.id ?? '', smooth(l.points, null, 10, !!l.closed).pts, !!l.closed, [[0, 0]] as [number, number][]).lines,
    hidden: (kind) => !!kind && filter.hiddenTypes.includes(kind),
  });
  $effect(() => {
    void cave.selected;
    void cave.also;
    schedule();
  });
  // the selected line is fitted again at another detail
  let handledRefit = cave.refit.seq; // a panel opened later does not redo old requests
  $effect(() => {
    const r = cave.refit;
    if (r.seq === handledRefit) return;
    handledRefit = r.seq;
    untrack(() => editor.refitLine(r.id, r.detail));
  });
  // other people's pointers move
  $effect(() => {
    for (const o of Object.values(room.cursors)) void o.seen;
    void room.people.length;
    schedule();
  });

  const index = $derived(stations.findIndex((s) => s.id === cave.station));
  const toWorld = (x: number, y: number): [number, number] => [(x - ox) / scale, (oy - y) / scale];
  const toScreen = (u: number, v: number): [number, number] => [ox + u * scale, oy - v * scale];

  /** Scans placed on this section, drawn under the sketch. */
  let placed: ScanPlacement[] = [];

  /** The station shown is a fixed one of the survey (its triangle gets a dot). */
  let fixedHere = false;

  function refresh() {
    if (!ready) return;
    stations = engine.sectionStations(cave.adjusted, true, wireFilter());
    if (!stations.some((s) => s.id === cave.station)) cave.station = (stations.find((s) => s.outlined) ?? stations[0])?.id ?? '';
    section = cave.station ? engine.sectionAt(cave.adjusted, cave.station, true, wireFilter()) : null;
    placed = cave.station ? engine.scanPlacements(`section:${cave.station}`, cave.adjusted).filter((p) => !filter.hiddenTrips.includes(p.trip) && have(p.path)) : [];
    fixedHere = !!cave.station && engine.scene(cave.adjusted).stations.some((x) => x.id === cave.station && x.benchmark);
    frameOnce();
    schedule();
  }

  // Re-read whenever the cave changes, a station is picked, or the adjustment or trip filter does.
  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void cave.station;
    void filter.hiddenTrips.length;
    void scans.paths.length; // scans appear when their folder is opened
    void option('showScans', true);
    void option('halo', true);
    void haloReach();
    void haloFade();
    void option('scanOpacity', 0.5);
    void ready;
    untrack(refresh);
  });

  $effect(() => {
    void cave.units;
    void barH;
    void width;
    void height;
    untrack(frameOnce);
    schedule();
  });

  /** Frames the section whenever a different station is shown (not after every stroke). */
  function frameOnce() {
    if (section && framed !== section.id && width > 0) {
      framed = section.id;
      fit();
    }
  }

  function fit() {
    if (classroom.role === 'student') return; // a student's view is the teacher's
    const s = section;
    if (!s || width === 0) return;
    const all = [...s.outline, ...s.measured, ...s.sketches.flatMap((k) => k.points)];
    const reach = Math.max(1, ...all.flatMap((p) => [Math.abs(p[0]), Math.abs(p[1])])) * 1.12;
    const top = barH + 16;
    const room = [width / 2 - 20, (height - top - 48) / 2];
    scale = Math.max(2, Math.min(room[0], room[1]) / reach);
    ox = width / 2;
    oy = top + (height - top - 48) / 2;
    schedule();
  }

  // A style being dragged in the ribbon shows at once (a redraw only: nothing is read again).
  $effect(() => {
    void liveStyle.style;
    void liveStyle.id;
    void drawing.scale; // a new drawing scale sizes dashes and fills again
    schedule();
  });

  function schedule() {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), draw()));
  }

  const css = () => {
    const c = getComputedStyle(document.documentElement);
    const v = (n: string) => c.getPropertyValue(n).trim();
    return { text: v('--text'), muted: v('--muted'), line: v('--line'), accent: v('--accent'), accentBg: v('--accent-bg'), panel: v('--panel') };
  };

  function draw() {
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, height);
    const c = css();
    const s = section;
    if (!s) return;

    // the rock halo: a soft glow from the measured walls out into the rock, with rock marks fading with it
    if (option('halo', true)) {
      // the glow follows the walls as drawn (as you sketch); with none drawn yet, the measured outline
      const drawn: HaloLine[] = editor
        .display()
        .filter((k) => k.kind === 'wall' && k.points.length >= 2)
        .map((k) => ({ pts: smooth(k.points as [number, number][], null, 8, k.closed).pts.map((p) => toScreen(p[0], p[1])), closed: k.closed }));
      const lines = drawn.length ? drawn : s.outline.length >= 3 ? [{ pts: s.outline.map((p) => toScreen(p[0], p[1])), closed: true }] : [];
      if (lines.length) drawHalo(g, width, height, lines, { reach: Math.min(Math.max(haloReach() * 0.3 * scale, 14), 300), fade: haloFade(), tint: c.accent, ink: c.muted, toWorld, toScreen });
    }

    // scans placed on this section lie under everything drawn here
    if (option('showScans', true)) {
      for (const sp of placed) {
        const img = scanImage(sp.path, schedule);
        if (img) drawScan(g, img, sp, { ox, oy, scale }, option('scanOpacity', 0.5));
      }
    }

    // the station: its survey point triangle, where the section is cut
    const [cx, cy] = toScreen(0, 0);
    g.strokeStyle = c.text;
    g.lineWidth = 1.6;
    drawStationMarker(g, cx, cy, fixedHere, 9);

    // the measured outline: a guide to trace against, never the passage itself
    if (s.outline.length >= 3) {
      g.beginPath();
      s.outline.forEach((p, i) => {
        const [x, y] = toScreen(p[0], p[1]);
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      });
      g.closePath();
      g.fillStyle = c.accentBg;
      g.globalAlpha = 0.4;
      g.fill();
      g.globalAlpha = 1;
      g.setLineDash([6, 4]);
      g.strokeStyle = c.muted;
      g.lineWidth = 1.5;
      g.lineJoin = 'round';
      g.stroke();
      g.setLineDash([]);
    }
    g.fillStyle = c.muted;
    for (const p of s.measured) {
      const [x, y] = toScreen(p[0], p[1]);
      g.beginPath();
      g.arc(x, y, 3.2, 0, Math.PI * 2);
      g.fill();
    }

    // what has been sketched
    const look = looks();
    drawStrokes(g, editor.display(), toScreen, look, 1, new Set(selectedIds()), [[0, 0]]);
    drawHandles(g, editor.handles(), c.accent, c.panel);
    if (copyOp.on && copyOp.view === 'section' && cursorW) {
      const ghost = copyGhost('section', cursorW);
      paintCopy(g, copyOp.base ? toScreen(copyOp.base[0], copyOp.base[1]) : null, toScreen(cursorW[0], cursorW[1]), ghost?.lines ?? null, (tg, lines) => drawStrokes(tg, lines.map((l) => ({ ...l, points: l.points as [number, number][] })), toScreen, look, 1, '', [[0, 0]]));
    }
    if (pickupAt && cave.tool && !live) {
      // the end of an open line the next stroke would carry on from
      g.strokeStyle = c.accent;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(pickupAt[0], pickupAt[1], 9, 0, Math.PI * 2);
      g.stroke();
    }
    if (live && live.pts.length > 1) drawStrokes(g, [{ kind: cave.tool || 'sketch', text: isStampKind(cave.tool) ? liveWords(cave.tool, option('label', '')) : undefined, points: live.pts, widths: live.pressure ? live.widths : null, closed: false }], toScreen, look);
    // other people's pointers in this view
    for (const o of cursorsFor('section', cave.station)) {
      const [ox, oy] = toScreen(o.x, o.y);
      drawCursor(g, ox, oy, o.person?.name ?? '', o.person?.color ?? '#888888');
    }

    // up arrow and what it means
    g.fillStyle = c.muted;
    g.strokeStyle = c.muted;
    g.font = '11px system-ui, sans-serif';
    g.lineWidth = 1.5;
    const ay = height - 56;
    g.beginPath();
    g.moveTo(width - 28, ay);
    g.lineTo(width - 28, ay - 26);
    g.moveTo(width - 33, ay - 20);
    g.lineTo(width - 28, ay - 26);
    g.lineTo(width - 23, ay - 20);
    g.stroke();
    g.textAlign = 'right';
    g.fillText(s.up_means === 'north' ? 'north' : 'up', width - 38, ay - 10);

    // scale bar in the chosen units
    const per = metresPer(cave.units);
    const len = niceNumber(100 / scale / per);
    g.strokeStyle = c.text;
    g.fillStyle = c.text;
    g.lineWidth = 2;
    g.textAlign = 'left';
    g.beginPath();
    g.moveTo(16, height - 44);
    g.lineTo(16 + len * per * scale, height - 44);
    g.stroke();
    g.fillText(`${len} ${unitName(cave.units)}`, 16, height - 50);
  }

  // ---- input: pen and mouse draw, fingers and middle button pan, wheel zooms -------------------
  // Fingers: one draws (with a tool running) or pans, two pan and pinch-zoom together.
  const touches = new Touches();
  const guard = new PenGuard();
  let erasing = false;
  let gesturing = false;
  let panning: { x: number; y: number } | null = null;
  let lastMiddle = 0;

  function addPoint(e: PointerEvent) {
    if (!live) return;
    const rect = canvas.getBoundingClientRect();
    const p = toWorld(e.clientX - rect.left, e.clientY - rect.top);
    const last = live.pts[live.pts.length - 1];
    if (last && Math.hypot(p[0] - last[0], p[1] - last[1]) * scale < 1.5) return;
    live.pts.push(p);
    // Pressure only changes how thick the line is drawn, never where it goes.
    live.widths.push(live.pressure ? widthFor(e.pressure) : 1);
  }

  function onDown(e: PointerEvent) {
    markViewUsed('section');
    canvas.setPointerCapture(e.pointerId);
    canvas.focus();
    if (e.button === 0 && cave.station && waitingForPoint(`section:${cave.station}`)) {
      // Aligning a scan: this place is what the spot picked on the scan is.
      const r = canvas.getBoundingClientRect();
      const w = toWorld(e.clientX - r.left, e.clientY - r.top);
      givePoint(w[0], w[1]);
      return;
    }
    if (e.pointerType === 'pen') {
      guard.pen(e);
      // A pen landing means anything a resting hand started was never a gesture.
      if (touches.count) {
        touches.clear();
        gesturing = false;
        live = null;
        panning = null;
        editor.cancel();
      }
      if (isEraser(e)) {
        erasing = true;
        eraseAt(e);
        return;
      }
    }
    if (e.pointerType === 'touch' && !guard.touchDown(e)) {
      engine.log('warn', 'touch', `A touch was ignored: ${guard.lastRejected}.`);
      return; // a palm or a fist: ignore it completely
    }
    if (e.pointerType === 'touch' && touches.down(e) >= 2) {
      // A second finger: this was the start of a gesture, not a line.
      gesturing = true;
      live = null;
      panning = null;
      editor.cancel();
      schedule();
      return;
    }
    const middle = e.button === 1;
    if (middle) {
      e.preventDefault();
      if (e.timeStamp - lastMiddle < 350) fit();
      lastMiddle = e.timeStamp;
    }
    if (e.button === 0 && !cave.tool && copyOp.on && copyOp.view === 'section') {
      // the Copy command: the first press is the base point, each after drops a copy
      const r = canvas.getBoundingClientRect();
      const w = toWorld(e.clientX - r.left, e.clientY - r.top);
      copyPress('section', copyOp.base ? w : snapToSelection('section', w, 10 / (scale || 1)));
      schedule();
      return;
    }
    if (e.button === 0 && !cave.tool) {
      const r = canvas.getBoundingClientRect();
      if (editor.press(e.clientX - r.left, e.clientY - r.top, e.shiftKey || e.metaKey || e.ctrlKey)) return;
    }
    const draws = !!cave.tool && !middle && e.button === 0 && (e.pointerType !== 'touch' || option('finger', true)) && !!section;
    if (draws) {
      if (isStampKind(cave.tool)) {
        // A symbol: press where it goes, drag to say which way is up and how big.
        const r = canvas.getBoundingClientRect();
        const p = toWorld(e.clientX - r.left, e.clientY - r.top);
        live = { pts: [p, [p[0], p[1]]], widths: [1, 1], pressure: false };
      } else {
        live = { pts: [], widths: [], pressure: e.pointerType === 'pen' && option('pressure', true) };
        addPoint(e);
      }
    } else {
      panning = { x: e.clientX, y: e.clientY };
    }
  }

  // Where the next stroke would pick an open line up: shown while a drawing tool waits.
  let pickupAt: [number, number] | null = null;
  function showPickup(x: number, y: number) {
    const hit = cave.tool && !isStampKind(cave.tool) ? endpointNear(section?.sketches ?? [], cave.tool, (p) => toScreen(p[0], p[1]), x, y) : null;
    const next = hit ? hit.at : null;
    if ((next?.[0] ?? -1) !== (pickupAt?.[0] ?? -1) || (next?.[1] ?? -1) !== (pickupAt?.[1] ?? -1)) {
      pickupAt = next;
      schedule();
    }
  }

  function eraseAt(e: PointerEvent) {
    const r = canvas.getBoundingClientRect();
    if (editor.eraseAt(e.clientX - r.left, e.clientY - r.top)) schedule();
  }

  /** Where the pointer last was, in this view's own space (for paste and the Copy command). */
  let cursorW: [number, number] | null = null;
  function onMove(e: PointerEvent) {
    if (e.pointerType !== 'touch') {
      // tell the others where this pointer is (rate limited; nothing happens when not sharing)
      const rr = canvas.getBoundingClientRect();
      const w = toWorld(e.clientX - rr.left, e.clientY - rr.top);
      cursorW = w;
      if (copyOp.on) schedule();
      reportCursor('section', cave.station, w[0], w[1], cave.tool);
    }
    if (e.pointerType === 'pen') {
      guard.pen(e);
      if (erasing) {
        eraseAt(e);
        return;
      }
    }
    if (e.pointerType === 'touch' && guard.ignored(e)) return;
    if (e.pointerType === 'touch') {
      const g = touches.move(e);
      if (g) {
        // Two fingers: pan with the middle of them, zoom with how far apart they are.
        const r0 = canvas.getBoundingClientRect();
        const mx = g.cx - r0.left;
        const my = g.cy - r0.top;
        ox = mx - (mx - ox) * g.scale + g.dx;
        oy = my - (my - oy) * g.scale + g.dy;
        scale *= g.scale;
        schedule();
        return;
      }
      if (gesturing) return; // one finger left after a gesture: wait until all are lifted
    }
    const r = canvas.getBoundingClientRect();
    if (editor.move(e.clientX - r.left, e.clientY - r.top)) return;
    if (!live && !panning && cave.tool) showPickup(e.clientX - r.left, e.clientY - r.top);
    if (live) {
      if (isStampKind(cave.tool)) {
        const r2 = canvas.getBoundingClientRect();
        live.pts[1] = toWorld(e.clientX - r2.left, e.clientY - r2.top);
      } else {
        const samples = e.getCoalescedEvents?.() ?? [];
        for (const ev of samples.length > 0 ? samples : [e]) addPoint(ev);
      }
      schedule();
    } else if (panning) {
      ox += e.clientX - panning.x;
      oy += e.clientY - panning.y;
      panning = { x: e.clientX, y: e.clientY };
      schedule();
    }
  }

  function onUp(e?: PointerEvent) {
    if (e?.pointerType === 'pen') {
      guard.pen(e);
      if (erasing) {
        erasing = false;
        return;
      }
    }
    if (e?.pointerType === 'touch' && guard.ignored(e)) {
      guard.touchUp(e);
      return;
    }
    if (e?.pointerType === 'touch') {
      const left = touches.up(e);
      if (gesturing) {
        // A gesture never commits a stroke; it ends when every finger is up.
        if (left === 0) gesturing = false;
        panning = null;
        return;
      }
      if (e.type === 'pointercancel') {
        // The system took the touch (a palm, a system gesture): drop the stroke.
        live = null;
        panning = null;
        editor.cancel();
        schedule();
        return;
      }
    }
    panning = null;
    if (editor.release()) return;
    if (!live) return;
    const stroked = live;
    live = null;
    if (isStampKind(cave.tool) && stroked.pts.length === 2 && cave.station) {
      const [p0, q] = stroked.pts;
      const a = toScreen(p0[0], p0[1]);
      const b = toScreen(q[0], q[1]);
      // Just a click: a symbol of the chosen size, pointing up.
      const p1: [number, number] = Math.hypot(a[0] - b[0], a[1] - b[1]) < 6 ? [p0[0], p0[1] + option('symbolSize', 1.5)] : q;
      if (needsWords(cave.tool) && !option('label', '').trim()) {
        // the words are typed in place, after the click and drag that set where, how big and which way
        const spot = stampEditSpot(cave.tool, [a, toScreen(p1[0], p1[1])], leaderLook('', (scale * drawing.scale) / 1000));
        const r = canvas.getBoundingClientRect();
        const tool = cave.tool;
        const where = cave.station;
        if (spot)
          openNewText(r.left + spot.x, r.top + spot.y, spot.size, (words) => {
            const made = engine.addSectionSketch(where, tool, [p0, p1], null, false, cave.adjusted, words);
            message = made === 'ok' ? '' : made;
            changed();
            schedule();
          }, 'Type the text', spot.angle);
        schedule();
        return;
      }
      const result = engine.addSectionSketch(cave.station, cave.tool, withLabelSpot(cave.tool, [p0, p1], option('label', '')), null, false, cave.adjusted, option('label', ''));
      message = result === 'ok' ? '' : result;
      changed();
      schedule();
      // a leader placed without words asks for them at once
      if (result === 'ok' && cave.tool === 'callout' && !option('label', '').trim()) {
        const spot = stampEditSpot('callout', [a, toScreen(p1[0], p1[1])], leaderLook('', (scale * drawing.scale) / 1000));
        const r = canvas.getBoundingClientRect();
        if (spot) openTextEdit(engine.lastAddedId(), '', r.left + spot.x, r.top + spot.y, spot.size, 'Type the note');
      }
      return;
    }
    if (stroked.pts.length >= 2 && cave.station) {
      // A freehand stroke becomes a smooth curve through a few fit points.
      const wantClosed = option('close', false) || isFillKind(cave.tool); // an area fill is always a loop
      const fit = fitStroke(stroked.pts, stroked.pressure ? stroked.widths : null, tolerancePx(option('detail', 5)) / scale, wantClosed);
      const closed = wantClosed && fit.pts.length >= 3;
      if (isFillKind(cave.tool) && !closed) {
        message = 'A fill needs a loop: draw all the way around the area.';
        schedule();
        return;
      }
      // Starting on the end of an open line of the same kind carries that line on instead of starting a new one.
      const start = toScreen(stroked.pts[0][0], stroked.pts[0][1]);
      const pickup = endpointNear(section?.sketches ?? [], cave.tool, (p) => toScreen(p[0], p[1]), start[0], start[1]);
      let result: string;
      if (pickup) {
        const joined = joinFit(pickup.line, fit, pickup.atStart);
        result = engine.editLine(pickup.line.id, joined.pts, joined.widths, cave.adjusted);
        cave.selected = '';
      } else {
        result = engine.addSectionSketch(cave.station, cave.tool, fit.pts, fit.widths, closed, cave.adjusted, '', toolStyle());
      }
      message = result === 'ok' ? '' : result;
      changed();
    }
    schedule();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const f = Math.exp(-e.deltaY * 0.0015);
    ox = mx - (mx - ox) * f;
    oy = my - (my - oy) * f;
    scale *= f;
    schedule();
  }

  function doUndo() {
    if (engine.undo('section')) changed();
  }
  function doRedo() {
    if (engine.redo('section')) changed();
  }
  function onKey(e: KeyboardEvent) {
    if (editor.key(e)) {
      e.preventDefault();
      return;
    }
    if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== 'z') return;
    e.preventDefault();
    if (e.shiftKey) doRedo();
    else doUndo();
  }

  function step(by: number) {
    const next = stations[index + by];
    if (next) cave.station = next.id;
  }

  const caption = $derived.by(() => {
    const s = section;
    if (!s) return '';
    if (s.note) return s.note;
    const n = s.measured.length;
    const base = `${n} measured wall point${n === 1 ? '' : 's'} at ${s.label}${s.inferred ? ', inferred from neighbouring stations (no measurements here)' : ''}. The dashed outline is a guide to trace against.`;
    return base;
  });

  onMount(() => {
    const offClip = registerClipView('section', {
      lines: () => (section?.sketches ?? []) as unknown as (Drawn & { id: string })[],
      target: () => cave.station,
      cursor: () => cursorW,
      centre: () => (width > 0 ? toWorld(width / 2, height / 2) : null),
    });
    const offSelectAll = registerSelectAll('section', () => selectIds(editor.allIds()));
    const ro = new ResizeObserver(() => {
      width = wrap.clientWidth;
      height = wrap.clientHeight;
      schedule();
    });
    ro.observe(wrap);
    const offTheme = onTheme(schedule);
    loadEngine().then(() => (ready = true));
    return () => {
      offSelectAll();
      offClip();
      ro.disconnect();
      offTheme();
      cancelAnimationFrame(frame);
    };
  });
</script>

<div class="section" bind:this={wrap}>
  <canvas
    bind:this={canvas}
    tabindex="0"
    style="width:{width}px;height:{height}px;cursor:{cave.tool ? 'crosshair' : 'grab'}"
    onpointerdown={onDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
    onwheel={onWheel}
    ondblclick={(e) => {
      const r = canvas.getBoundingClientRect();
      if (cave.tool) return;
      // a double click on a symbol, text or leader edits its words in place; on a curve it adds a fit point
      const w = editor.wordsAt(e.clientX - r.left, e.clientY - r.top);
      if (w) openTextEdit(w.id, w.text, r.left + w.x, r.top + w.y, w.size, w.kind === 'text' ? 'Type the text' : w.kind === 'callout' ? 'Type the note' : 'Label', w.angle);
      else editor.addPoint(e.clientX - r.left, e.clientY - r.top);
    }}
    onkeydown={onKey}
    oncontextmenu={(e) => {
      e.preventDefault();
      if (copyOp.on) {
        endCopy(); // a right click ends the Copy command
        return;
      }
      if (cave.tool) {
        cave.tool = ''; // a right click ends the command
        return;
      }
      const r = canvas.getBoundingClientRect();
      const similar = editor.contextAt(e.clientX - r.left, e.clientY - r.top);
      if (similar) openLineMenu(e.clientX, e.clientY, similar);
    }}
  ></canvas>
  <div class="bar" bind:clientHeight={barH}>
    <button title="Previous station" disabled={index <= 0} onclick={() => step(-1)}><ChevronLeft size={15} /></button>
    <select bind:value={cave.station} aria-label="Station">
      {#each stations as s}<option value={s.id}>{s.label}{s.outlined ? '' : ' (no walls)'}{s.sketches ? ` · ${s.sketches} sketch${s.sketches === 1 ? '' : 'es'}` : ''}</option>{/each}
    </select>
    <button title="Next station" disabled={index < 0 || index >= stations.length - 1} onclick={() => step(1)}><ChevronRight size={15} /></button>
    <PanelUndo scope="section" label="in this cross section" />
    <button title="Fit (or double middle-click)" onclick={fit}><Maximize size={15} /></button>
  </div>
  {#if ready && !stations.length}
    <div class="empty">Open a cave with surveyed legs to cut cross sections at its stations.</div>
  {:else if message}
    <div class="note warn" role="status">{message}</div>
  {:else if caption}
    <div class="note" class:warn={!!section?.note} role="status">{caption}</div>
  {/if}
</div>

<style>
  .section { position: relative; height: 100%; overflow: hidden; background: var(--panel-2); }
  canvas { position: absolute; inset: 0; display: block; touch-action: none; outline: none; }
  .bar { position: absolute; top: 8px; left: 8px; right: 8px; display: flex; gap: 6px; align-items: center; flex-wrap: wrap; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; width: fit-content; max-width: calc(100% - 16px); }
  select, button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); cursor: pointer; display: inline-grid; place-items: center; }
  select { max-width: 200px; }
  button:disabled { opacity: 0.4; cursor: default; }
  .note { position: absolute; left: 8px; right: 8px; bottom: 8px; padding: 6px 10px; font-size: 12px; color: var(--muted); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); }
  .note.warn { color: var(--text); border-color: var(--warn); }
  .empty { position: absolute; inset: 0; display: grid; place-items: center; padding: 16px; text-align: center; color: var(--muted); font-size: 13px; }
</style>
