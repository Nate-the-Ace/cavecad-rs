//! Writing a document as an R2000 ASCII DXF.

use crate::tags::{Tag, Writer, ascii_text, checksum, geometry_hash, layer_name, render};
use crate::{FORMAT_VERSION, MAX_VALUE_LINE};
use cave_doc::{Document, Field, Kind};
use cave_id::Id;
use cave_scene::{LegStyle, Scene};
use cave_style::{Ink, Linetype, dxf_color, style};
use std::collections::BTreeMap;

/// How to write the file.
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    /// Draw the adjusted station positions (the cave inside is unaffected either way).
    pub adjusted: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { adjusted: true }
    }
}

/// Width of a sketch line at pen weight 1.0, in drawing units (metres).
const BASE_WIDTH: f64 = 0.15;
/// Characters of embedded JSON per string; well under [`MAX_VALUE_LINE`].
const CHUNK: usize = 200;
/// Chunks per XRECORD.
const CHUNKS_PER_RECORD: usize = 100;
/// Chunks per hidden carrier entity (extended data is limited to about 16 KB per object).
pub(crate) const CHUNKS_PER_CARRIER: usize = 40;
/// The frozen layer holding the carrier entities.
pub(crate) const CARRIER_LAYER: &str = "CAVECAD-DATA";

/// ASCII, and escaped so no JSON string can contain a character a code page could damage.
fn ascii_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c == ' ' {
            // Spaces only occur inside JSON strings; escaping them makes the payload immune to any
            // program that trims whitespace at the ends of a stored string.
            out.push_str("\\u0020");
        } else if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// The application name CaveCAD's tags have always been stored under in DXF extended data.
pub(crate) const TAG_APP: &str = "CaveSurvey";

struct Geometry<'a> {
    w: &'a mut Writer,
    owner: String,
    /// Layers used, with the colour and linetype of the type that put them there.
    layers: BTreeMap<String, (i32, Linetype)>,
}

/// One tag value as the old suite wrote it: backslash and newline escaped so it stays on one line,
/// and anything outside ASCII as `\U+XXXX` (drawing text uses the same escape).
fn tag_value(v: &str) -> String {
    ascii_text(&v.replace('\\', "\\\\"))
}

/// A number the way the old suite's tags show it.
fn tag_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// How the old suite wrote one LRUD side: a number, `P` for open passage, or `5/P` for both.
fn lrud_text(value: Option<f64>, open: bool) -> Option<String> {
    match (value, open) {
        (Some(v), true) => Some(format!("{}/P", tag_number(v))),
        (Some(v), false) => Some(tag_number(v)),
        (None, true) => Some("P".into()),
        (None, false) => None,
    }
}

/// The label of the station a shot reference points at, or empty.
fn station_label(doc: &Document, id: Option<Id>) -> String {
    id.and_then(|i| doc.entity(i))
        .map(|e| e.text(Field::StationLabel))
        .unwrap_or_default()
}

/// Shot data in the old suite's tag vocabulary (`From`, `To`, `Trip`, `ShotSeq`, `Distance`, ...).
fn shot_tags(doc: &Document, shot: &str) -> Vec<(String, String)> {
    let Ok(id) = shot.parse::<Id>() else {
        return Vec::new();
    };
    let Some(e) = doc.entity(id) else {
        return Vec::new();
    };
    let mut tags: Vec<(String, String)> = Vec::new();
    let mut add = |k: &str, v: Option<String>| {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            tags.push((k.to_string(), v));
        }
    };
    let trip_index = e.reference(Field::ShotTrip).and_then(|t| {
        let mut trips: Vec<(f64, Id)> = doc
            .ids_of(Kind::Trip)
            .into_iter()
            .map(|i| {
                (
                    doc.entity(i)
                        .and_then(|x| x.number(Field::TripOrder))
                        .unwrap_or(f64::MAX),
                    i,
                )
            })
            .collect();
        trips.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        trips.iter().position(|(_, i)| *i == t)
    });
    add(
        "From",
        Some(station_label(doc, e.reference(Field::ShotFrom))),
    );
    add("To", Some(station_label(doc, e.reference(Field::ShotTo))));
    add("Trip", Some(trip_index.unwrap_or(0).to_string()));
    add("ShotSeq", e.number(Field::ShotOrder).map(tag_number));
    for (key, f) in [
        ("Distance", Field::ShotDistance),
        ("Azimuth", Field::ShotAzimuth),
        ("Inclination", Field::ShotInclination),
    ] {
        add(key, e.number(f).map(tag_number));
    }
    for (key, v, open) in [
        ("Left", Field::ShotLeft, Field::ShotLeftOpen),
        ("Right", Field::ShotRight, Field::ShotRightOpen),
        ("Up", Field::ShotUp, Field::ShotUpOpen),
        ("Down", Field::ShotDown, Field::ShotDownOpen),
    ] {
        add(key, lrud_text(e.number(v), e.flag(open)));
    }
    add(
        "BackAzimuth",
        e.number(Field::ShotBackAzimuth).map(tag_number),
    );
    add(
        "BackInclination",
        e.number(Field::ShotBackInclination).map(tag_number),
    );
    tags
}

impl Geometry<'_> {
    /// The layer and colour a palette type draws with; records the layer with the type's linetype.
    fn look(&mut self, key: &str) -> (&'static str, i32) {
        let st = style(key);
        let color = dxf_color(st.ink);
        self.layers
            .insert(layer_name(st.layer), (color, st.linetype));
        (st.layer, color)
    }

    fn entity(&mut self, kind: &str, layer: &str, color: Option<i32>, subclass: &str) {
        let h = self.w.handle();
        let layer = layer_name(layer);
        self.layers
            .entry(layer.clone())
            .or_insert((color.unwrap_or(7), Linetype::Solid));
        self.w.tag(0, kind);
        self.w.tag(5, h);
        self.w.tag(330, self.owner.clone());
        self.w.tag(100, "AcDbEntity");
        self.w.tag(8, layer);
        if let Some(c) = color {
            self.w.tag(62, c.to_string());
        }
        self.w.tag(100, subclass);
    }

    /// Extended data in the old suite's shape: application `CaveSurvey`, one `Key=Value` string per
    /// tag. Strings are kept under the 255-character extended-data limit.
    fn tags(&mut self, tags: &[(String, String)]) {
        if tags.is_empty() {
            return;
        }
        self.w.tag(1001, TAG_APP);
        for (k, v) in tags {
            let mut line = format!("{k}={}", tag_value(v));
            line.truncate(250);
            self.w.tag(1000, line);
        }
    }

    fn point3(&mut self, base: i32, p: [f64; 3]) {
        for (i, v) in p.iter().enumerate() {
            self.w.num(base + 10 * i as i32, *v);
        }
    }

    fn line(&mut self, key: &str, a: [f64; 3], b: [f64; 3]) {
        let (layer, color) = self.look(key);
        self.entity("LINE", layer, Some(color), "AcDbLine");
        self.point3(10, a);
        self.point3(11, b);
    }

    fn point(&mut self, key: &str, ink: Option<Ink>, p: [f64; 3]) {
        let (layer, mut color) = self.look(key);
        if let Some(i) = ink {
            color = dxf_color(i);
        }
        self.entity("POINT", layer, Some(color), "AcDbPoint");
        self.point3(10, p);
    }

    fn text(&mut self, key: &str, p: [f64; 3], height: f64, s: &str) {
        let (layer, color) = self.look(key);
        self.entity("TEXT", layer, Some(color), "AcDbText");
        self.point3(10, p);
        self.w.num(40, height);
        self.w.tag(1, ascii_text(s));
        self.w.tag(100, "AcDbText");
    }

    /// A 3D polyline (walls run through space, so they keep their elevation).
    fn polyline3d(&mut self, key: &str, pts: &[[f64; 3]]) {
        let (layer, color) = self.look(key);
        self.entity("POLYLINE", layer, Some(color), "AcDb3dPolyline");
        self.w.tag(66, "1");
        self.point3(10, [0.0, 0.0, 0.0]);
        self.w.tag(70, "8");
        for p in pts {
            self.entity("VERTEX", layer, Some(color), "AcDbVertex");
            self.w.tag(100, "AcDb3dPolylineVertex");
            self.point3(10, *p);
            self.w.tag(70, "32");
        }
        let h = self.w.handle();
        self.w.tag(0, "SEQEND");
        self.w.tag(5, h);
        self.w.tag(330, self.owner.clone());
        self.w.tag(100, "AcDbEntity");
        self.w.tag(8, layer_name(layer));
    }

    /// A flat polyline with optional pen weight per vertex, as variable-width segments.
    fn lwpolyline(&mut self, key: &str, pts: &[[f64; 2]], widths: Option<&[f64]>) {
        let (layer, color) = self.look(key);
        self.lwpolyline_on(layer.to_string(), color, pts, widths);
    }

    /// A flat polyline of `key`'s type on its layer with `suffix` added (profile linework gets its
    /// own layers, apart from the plan's).
    fn lwpolyline_suffixed(
        &mut self,
        key: &str,
        suffix: &str,
        pts: &[[f64; 2]],
        widths: Option<&[f64]>,
    ) {
        let st = style(key);
        let color = dxf_color(st.ink);
        let layer = cave_style::dxf_layer(key, Some(suffix));
        self.layers.insert(layer_name(&layer), (color, st.linetype));
        self.lwpolyline_on(layer, color, pts, widths);
    }

    /// A circle of `key`'s type on its layer with `suffix` added.
    fn circle_suffixed(&mut self, key: &str, suffix: &str, c: [f64; 2], r: f64) {
        let st = style(key);
        let color = dxf_color(st.ink);
        let layer = cave_style::dxf_layer(key, Some(suffix));
        self.layers.insert(layer_name(&layer), (color, st.linetype));
        self.entity("CIRCLE", &layer, Some(color), "AcDbCircle");
        self.point3(10, [c[0], c[1], 0.0]);
        self.w.num(40, r.max(1e-6));
    }

    /// A placed symbol as polylines on its catalogue layer (with `suffix`, as other profile
    /// linework has).
    fn symbol(&mut self, sym: &cave_symbols::Symbol, at: [f64; 2], aim: [f64; 2], suffix: &str) {
        let layer = format!("{}-{suffix}", sym.layer);
        let color = dxf_color(cave_style::Ink::Text);
        self.layers
            .insert(layer_name(&layer), (color, cave_style::Linetype::Solid));
        for (mut pts, closed) in sym.placed(at, aim) {
            if closed {
                // the polylines written here are open, so a closed shape repeats its first point
                pts.push(pts[0]);
            }
            self.lwpolyline_on(layer.clone(), color, &pts, None);
        }
    }

    fn lwpolyline_on(
        &mut self,
        layer: String,
        color: i32,
        pts: &[[f64; 2]],
        widths: Option<&[f64]>,
    ) {
        self.entity("LWPOLYLINE", &layer, Some(color), "AcDbPolyline");
        self.w.tag(90, pts.len().to_string());
        self.w.tag(70, "0");
        for (i, p) in pts.iter().enumerate() {
            self.w.num(10, p[0]);
            self.w.num(20, p[1]);
            if let Some(ws) = widths {
                let next = ws.get(i + 1).copied().unwrap_or(ws[i]);
                self.w.num(40, BASE_WIDTH * ws[i]);
                self.w.num(41, BASE_WIDTH * next);
            }
        }
    }
}

fn draw_scene(g: &mut Geometry, doc: &Document, scene: &Scene) {
    for l in &scene.legs {
        let key = match l.style {
            LegStyle::New => "centerline",
            LegStyle::Closure => "closure",
            LegStyle::Tie => "tie",
        };
        g.line(key, l.from, l.to);
        g.tags(&shot_tags(doc, &l.shot));
    }
    for (n, s) in scene.splays.iter().enumerate() {
        g.line("splay", [s[0], s[1], s[2]], [s[3], s[4], s[5]]);
        let mut tags = scene
            .splay_shots
            .get(n)
            .map(|id| shot_tags(doc, id))
            .unwrap_or_default();
        tags.push(("Splay".into(), "1".into()));
        g.tags(&tags);
    }
    for run in &scene.walls {
        if run.len() >= 2 {
            g.polyline3d("wall", run);
        }
    }
    for (seq, st) in scene.stations.iter().enumerate() {
        g.point("station", st.anchor.then_some(Ink::Accent), st.at);
        g.tags(&[
            ("Station".into(), st.label.clone()),
            ("Seq".into(), seq.to_string()),
        ]);
        if !st.label.is_empty() {
            g.text(
                "label",
                [st.at[0] + 0.4, st.at[1] + 0.4, st.at[2]],
                1.0,
                &st.label,
            );
        }
    }
    for c in &scene.closures {
        let pct = c.percent.map_or(String::new(), |p| format!(" {p:.2}%"));
        g.text(
            "closure-note",
            [c.at[0] + 0.6, c.at[1] - 1.0, c.at[2]],
            1.0,
            &format!("closure {:.2} m{pct}", c.horizontal.hypot(c.vertical)),
        );
    }
}

/// One stroke as the drawing holds it (fit points in the drawing's own units).
struct Stroke<'a> {
    id: &'a str,
    kind: &'a str,
    points: &'a [[f64; 2]],
    widths: Option<&'a [f64]>,
    closed: bool,
    text: &'a str,
}

/// Samples per span when a curve is written out as a polyline.
const PER_SPAN: usize = 10;

/// The curve through the fit points, with the pen weight at every sample (linear between fit points).
fn curve_with_widths(
    pts: &[[f64; 2]],
    widths: Option<&[f64]>,
    closed: bool,
) -> (Vec<[f64; 2]>, Option<Vec<f64>>) {
    let curve = cave_geom::spline::smooth(pts, closed, PER_SPAN);
    let n = pts.len();
    let w = widths.filter(|w| w.len() == n && n >= 3).map(|w| {
        let spans = if closed { n } else { n - 1 };
        let mut out = vec![w[0]];
        for i in 0..spans {
            let next = w[(i + 1) % n];
            for s in 1..=PER_SPAN {
                out.push(w[i] + (next - w[i]) * s as f64 / PER_SPAN as f64);
            }
        }
        out
    });
    (
        curve,
        w.or_else(|| widths.filter(|w| w.len() == pts.len()).map(<[f64]>::to_vec)),
    )
}

/// Draws one stroke from any view, in its own look: a symbol as its drawing (with its label), text
/// as text, a fill as its outline and marks, a shaped line as its spine and ornament, and any other
/// line as the smooth curve through its fit points. `suffix` names the view (`PROFILE`, `MAP`,
/// `SECTION`) so each view's linework has its own layers.
fn draw_stroke(g: &mut Geometry, doc: &Document, s: &Stroke, suffix: &str) {
    if let (Some(sym), [a, b, ..]) = (cave_symbols::symbol(s.kind), s.points) {
        // A placed symbol: its drawing on the symbol's own layer, not a line between two points.
        g.symbol(sym, *a, *b, suffix);
        if !s.text.is_empty() {
            // its label sits to the symbol's right, a little larger than the symbol's radius
            let r = (b[0] - a[0]).hypot(b[1] - a[1]);
            if r > 0.0 {
                let (ux, uy) = ((b[0] - a[0]) / r, (b[1] - a[1]) / r);
                g.text(
                    "text",
                    [a[0] + 1.3 * r * uy, a[1] - 1.3 * r * ux, 0.0],
                    0.9 * r,
                    s.text,
                );
            }
        }
        return;
    }
    if s.kind == "text" {
        if let [a, b, ..] = s.points {
            // A text stroke: its words at the first point, as tall as the distance to the second.
            g.text(
                "text",
                [a[0], a[1], 0.0],
                (b[0] - a[0]).hypot(b[1] - a[1]),
                s.text,
            );
        }
        return;
    }
    if cave_fill::is_fill_key(s.kind) {
        let curve = cave_geom::spline::smooth(s.points, true, PER_SPAN);
        let mut outline = curve.clone();
        if let Some(first) = outline.first().copied() {
            outline.push(first);
        }
        g.lwpolyline_suffixed(s.kind, suffix, &outline, None);
        let marks = cave_fill::marks(s.kind, &curve, cave_fill::seed_of(s.id));
        for line in &marks.lines {
            g.lwpolyline_suffixed(s.kind, suffix, line, None);
        }
        for c in &marks.circles {
            g.circle_suffixed(s.kind, suffix, [c[0], c[1]], c[2]);
        }
        return;
    }
    let (curve, widths) = curve_with_widths(s.points, s.widths, s.closed);
    let shape = cave_shape::shape(s.kind);
    if shape.is_none_or(|sh| !sh.hide_spine) {
        let mut line = curve.clone();
        if s.closed
            && let Some(first) = line.first().copied()
        {
            line.push(first);
            if let Some(w) = &widths {
                // the closing sample repeats the first weight
                let mut w = w.clone();
                w.push(w[0]);
                g.lwpolyline_suffixed(s.kind, suffix, &line, Some(&w));
                draw_ornament(g, doc, s, &curve, suffix);
                return;
            }
        }
        g.lwpolyline_suffixed(s.kind, suffix, &line, widths.as_deref());
    }
    draw_ornament(g, doc, s, &curve, suffix);
}

/// The marks of a shaped line (hachures, fans, scallops), on the side the stroke says.
fn draw_ornament(g: &mut Geometry, doc: &Document, s: &Stroke, curve: &[[f64; 2]], suffix: &str) {
    if cave_shape::shape(s.kind).is_none() {
        return;
    }
    let flip =
        s.id.parse::<cave_id::Id>()
            .ok()
            .and_then(|id| doc.entity(id))
            .is_some_and(|e| e.flag(cave_doc::Field::FeatureFlip));
    let side = if flip { -1.0 } else { 1.0 };
    for line in cave_shape::ornament(s.kind, curve, s.closed, side, 1.0) {
        g.lwpolyline_suffixed(s.kind, suffix, &line, None);
    }
}

/// Where a station's section sits beside the station when nobody placed it: just east of the
/// station on a map, clear of the strokes (the same rule the app uses).
fn default_map_offset(sketches: &[cave_scene::SectionSketch]) -> [f64; 3] {
    let min_u = sketches
        .iter()
        .flat_map(|k| k.points.iter().map(|p| p[0]))
        .fold(f64::INFINITY, f64::min);
    [(-min_u).max(0.0) + 1.5, 0.0, 0.0]
}

/// Where a station's section sits on a band when nobody placed it: just above the station.
fn default_band_offset(sketches: &[cave_scene::SectionSketch]) -> [f64; 2] {
    let min_v = sketches
        .iter()
        .flat_map(|k| k.points.iter().map(|p| p[1]))
        .fold(f64::INFINITY, f64::min);
    [0.0, (-min_v).max(0.0) + 2.0]
}

/// A station's cross section drawn with its station's `base` as the origin (across = x, up = y).
fn draw_section<S: SectionLike>(
    g: &mut Geometry,
    doc: &Document,
    sec: &S,
    base: [f64; 2],
    suffix: &str,
) {
    for sk in sec.sketches() {
        let pts: Vec<[f64; 2]> = sk
            .points
            .iter()
            .map(|p| [base[0] + p[0], base[1] + p[1]])
            .collect();
        let stroke = Stroke {
            id: &sk.id,
            kind: &sk.kind,
            points: &pts,
            widths: sk.widths.as_deref(),
            closed: sk.closed,
            text: &sk.text,
        };
        draw_stroke(g, doc, &stroke, suffix);
    }
    if !sec.label().is_empty() {
        g.text(
            "label",
            [base[0], base[1] - 1.2, 0.0],
            0.8,
            &format!("Section at {}", sec.label()),
        );
    }
}

/// What both kinds of placed section (on the map, on a band) share.
trait SectionLike {
    fn label(&self) -> &str;
    fn sketches(&self) -> &[cave_scene::SectionSketch];
}
impl SectionLike for cave_scene::BandSection {
    fn label(&self) -> &str {
        &self.label
    }
    fn sketches(&self) -> &[cave_scene::SectionSketch] {
        &self.sketches
    }
}
impl SectionLike for cave_scene::MapSection {
    fn label(&self) -> &str {
        &self.label
    }
    fn sketches(&self) -> &[cave_scene::SectionSketch] {
        &self.sketches
    }
}

/// Everything drawn on the map (strokes lifted to the passage's height are written flat, in plan),
/// and every station's cross section beside its station.
fn draw_map_work(g: &mut Geometry, doc: &Document, adjusted: bool) {
    for sk in cave_scene::map_lines_3d(doc, adjusted) {
        let pts: Vec<[f64; 2]> = sk.points.iter().map(|p| [p[0], p[1]]).collect();
        let stroke = Stroke {
            id: &sk.id,
            kind: &sk.kind,
            points: &pts,
            widths: sk.widths.as_deref(),
            closed: sk.closed,
            text: &sk.text,
        };
        draw_stroke(g, doc, &stroke, "MAP");
    }
    for sec in cave_scene::map_sections(doc, adjusted, &cave_scene::ViewFilter::default()) {
        let off = sec
            .placement
            .unwrap_or_else(|| default_map_offset(&sec.sketches));
        draw_section(
            g,
            doc,
            &sec,
            [sec.at[0] + off[0], sec.at[1] + off[1]],
            "SECTION",
        );
    }
}

/// Profile bands and their sketches, laid out in a column below the plan so they cannot be
/// mistaken for it. Band coordinates: x is distance along the run, y is elevation.
fn draw_profiles(g: &mut Geometry, doc: &Document, adjusted: bool, plan_min: [f64; 2]) {
    let mut base_y = plan_min[1] - 150.0;
    for run in cave_scene::runs(doc, adjusted) {
        let Some(p) = cave_scene::profile_scene(doc, &run.key, adjusted) else {
            continue;
        };
        let Some(b) = p.bounds else { continue };
        let (dx, dy) = (plan_min[0] - b[0], base_y - b[3]);
        let at = |s: f64, z: f64| [s + dx, z + dy];
        g.text(
            "label",
            [at(b[0], b[3])[0], at(b[0], b[3])[1] + 3.0, 0.0],
            2.0,
            &format!(
                "Profile of run {} (x: distance along the run, y: elevation)",
                run.key
            ),
        );
        for l in &p.legs {
            g.lwpolyline("profile", &[at(l[0], l[1]), at(l[2], l[3])], None);
        }
        for line in p.ceiling.iter().chain(&p.floor) {
            let pts: Vec<[f64; 2]> = line.iter().map(|q| at(q[0], q[1])).collect();
            g.lwpolyline("profile-wall", &pts, None);
        }
        for q in &p.flat {
            let a = at(q[0], q[1]);
            g.point("profile-wall", None, [a[0], a[1], 0.0]);
        }
        for st in &p.stations {
            let a = at(st.s, st.z);
            g.point("station", None, [a[0], a[1], 0.0]);
            g.text("label", [a[0] + 0.4, a[1] + 0.4, 0.0], 1.0, &st.label);
        }
        for sk in &p.sketches {
            let pts: Vec<[f64; 2]> = sk.points.iter().map(|q| at(q[0], q[1])).collect();
            let stroke = Stroke {
                id: &sk.id,
                kind: &sk.kind,
                points: &pts,
                widths: sk.widths.as_deref(),
                closed: sk.closed,
                text: &sk.text,
            };
            draw_stroke(g, doc, &stroke, "PROFILE");
        }
        // cross sections drawn at stations, where the band shows them (placed by hand, or beside the station)
        for sec in &p.sections {
            let off = sec
                .placement
                .unwrap_or_else(|| default_band_offset(&sec.sketches));
            let base = at(sec.s + off[0], sec.z + off[1]);
            draw_section(g, doc, sec, base, "PROFILE");
        }
        base_y -= (b[3] - b[1]) + 60.0;
    }
}

/// A second copy of the cave, on hidden points in extended data. Other CAD programs keep extended
/// data when they re-save a drawing but drop the dictionary the primary copy lives in, so this is
/// what lets the cave survive being opened and saved elsewhere.
fn draw_carriers(g: &mut Geometry, payload: &str, chunks: &[&str]) {
    let parts: Vec<&[&str]> = chunks.chunks(CHUNKS_PER_CARRIER).collect();
    for (i, part) in parts.iter().enumerate() {
        g.entity("POINT", CARRIER_LAYER, Some(8), "AcDbPoint");
        g.point3(10, [0.0, 0.0, 0.0]);
        g.w.tag(1001, "CAVECAD");
        for line in [
            "CAVECAD-DOC-PART".to_string(),
            format!("version={FORMAT_VERSION}"),
            format!("part={i}"),
            format!("parts={}", parts.len()),
            format!("bytes={}", payload.len()),
            format!("chunks={}", chunks.len()),
            format!("hash={}", checksum(payload)),
        ] {
            g.w.tag(1000, line);
        }
        for c in *part {
            g.w.tag(1000, (*c).to_string());
        }
    }
}

/// Writes the document as DXF text. The file always contains the complete cave.
pub fn export(doc: &Document, opts: &ExportOptions) -> String {
    let mut w = Writer::new();
    let model_record = w.handle();
    let paper_record = w.handle();
    let scene = cave_scene::build(doc, opts.adjusted);
    let payload = ascii_json(&doc.to_json_compact());
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(CHUNK)
        .map(|c| std::str::from_utf8(c).expect("payload is ASCII"))
        .collect();

    // ---- entities (the derived drawing) ----
    let layers: BTreeMap<String, (i32, Linetype)>;
    {
        let mut g = Geometry {
            w: &mut w,
            owner: model_record.clone(),
            layers: BTreeMap::new(),
        };
        draw_scene(&mut g, doc, &scene);
        draw_map_work(&mut g, doc, opts.adjusted);
        let plan_min = scene.bounds.map_or([0.0, 0.0], |b| [b[0], b[1]]);
        draw_profiles(&mut g, doc, opts.adjusted, plan_min);
        draw_carriers(&mut g, &payload, &chunks);
        layers = std::mem::take(&mut g.layers);
    }
    let entity_tags = w.drain();
    let geometry = geometry_hash(&entity_tags);

    // ---- tables ----
    w.tag(0, "SECTION");
    w.tag(2, "TABLES");
    let table = |w: &mut Writer, name: &str, count: usize| -> String {
        let handle = w.handle();
        w.tag(0, "TABLE");
        w.tag(2, name);
        w.tag(5, handle.clone());
        w.tag(330, "0");
        w.tag(100, "AcDbSymbolTable");
        w.tag(70, count.to_string());
        handle
    };
    let record =
        |w: &mut Writer, kind: &str, handle: Option<String>, owner: &str, sub: &str, name: &str| {
            let handle = handle.unwrap_or_else(|| w.handle());
            w.tag(0, kind);
            w.tag(5, handle);
            w.tag(330, owner);
            w.tag(100, "AcDbSymbolTableRecord");
            w.tag(100, sub);
            w.tag(2, name);
        };
    let mut names = vec!["ByBlock", "ByLayer", "Continuous"];
    names.extend(cave_style::LINETYPES.iter().map(|l| l.dxf_name()));
    let lt = table(&mut w, "LTYPE", names.len());
    for name in names {
        record(&mut w, "LTYPE", None, &lt, "AcDbLinetypeTableRecord", name);
        w.tag(70, "0");
        w.tag(
            3,
            match name {
                "Continuous" => "Solid line",
                "DASHED" => "Dashed ____ ____ ____",
                "LONGDASH" => "Long dash ________ ________",
                "SPARSE" => "Sparse dash __   __   __",
                "DASHDOT" => "Dash dot __ . __ . __ .",
                _ => "",
            },
        );
        w.tag(72, "65");
        let pattern = cave_style::LINETYPES
            .iter()
            .find(|l| l.dxf_name() == name)
            .map_or(&[][..], |l| l.pattern());
        if pattern.is_empty() {
            w.tag(73, "0");
            w.tag(40, "0.0");
        } else {
            w.tag(73, pattern.len().to_string());
            w.tag(40, pattern.iter().map(|x| x.abs()).sum::<f64>().to_string());
            for x in pattern {
                w.tag(49, x.to_string());
                w.tag(74, "0");
            }
        }
    }
    w.tag(0, "ENDTAB");
    let mut all_layers: Vec<(String, i32, Linetype)> = vec![("0".to_string(), 7, Linetype::Solid)];
    all_layers.extend(layers.iter().map(|(n, (c, lt))| (n.clone(), *c, *lt)));
    let ly = table(&mut w, "LAYER", all_layers.len());
    for (name, color, linetype) in &all_layers {
        record(&mut w, "LAYER", None, &ly, "AcDbLayerTableRecord", name);
        // The carrier layer is frozen: its points hold data, not drawing.
        w.tag(70, if name == CARRIER_LAYER { "1" } else { "0" });
        w.tag(62, color.to_string());
        w.tag(6, linetype.dxf_name());
    }
    w.tag(0, "ENDTAB");
    let st = table(&mut w, "STYLE", 1);
    record(
        &mut w,
        "STYLE",
        None,
        &st,
        "AcDbTextStyleTableRecord",
        "Standard",
    );
    for (c, v) in [
        (70, "0"),
        (40, "0.0"),
        (41, "1.0"),
        (50, "0.0"),
        (71, "0"),
        (42, "2.5"),
        (3, "txt"),
        (4, ""),
    ] {
        w.tag(c, v);
    }
    w.tag(0, "ENDTAB");
    let ap = table(&mut w, "APPID", 3);
    for name in ["ACAD", "CAVECAD", TAG_APP] {
        record(&mut w, "APPID", None, &ap, "AcDbRegAppTableRecord", name);
        w.tag(70, "0");
    }
    w.tag(0, "ENDTAB");
    let br = table(&mut w, "BLOCK_RECORD", 2);
    for (name, h) in [
        ("*Model_Space", &model_record),
        ("*Paper_Space", &paper_record),
    ] {
        record(
            &mut w,
            "BLOCK_RECORD",
            Some(h.clone()),
            &br,
            "AcDbBlockTableRecord",
            name,
        );
    }
    w.tag(0, "ENDTAB");
    w.tag(0, "ENDSEC");
    let tables = w.drain();

    // ---- blocks ----
    w.tag(0, "SECTION");
    w.tag(2, "BLOCKS");
    for (name, rec) in [
        ("*Model_Space", &model_record),
        ("*Paper_Space", &paper_record),
    ] {
        let begin = w.handle();
        w.tag(0, "BLOCK");
        w.tag(5, begin);
        w.tag(330, rec.clone());
        w.tag(100, "AcDbEntity");
        w.tag(8, "0");
        w.tag(100, "AcDbBlockBegin");
        w.tag(2, name);
        w.tag(70, "0");
        for (c, v) in [(10, "0.0"), (20, "0.0"), (30, "0.0")] {
            w.tag(c, v);
        }
        w.tag(3, name);
        w.tag(1, "");
        let end = w.handle();
        w.tag(0, "ENDBLK");
        w.tag(5, end);
        w.tag(330, rec.clone());
        w.tag(100, "AcDbEntity");
        w.tag(8, "0");
        w.tag(100, "AcDbBlockEnd");
    }
    w.tag(0, "ENDSEC");
    let blocks = w.drain();

    // ---- objects: the embedded cave ----
    let payload = ascii_json(&doc.to_json_compact());
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(CHUNK)
        .map(|c| std::str::from_utf8(c).expect("payload is ASCII"))
        .collect();
    let groups: Vec<&[&str]> = chunks.chunks(CHUNKS_PER_RECORD).collect();
    let (root, ours, meta) = (w.handle(), w.handle(), w.handle());
    let record_handles: Vec<String> = groups.iter().map(|_| w.handle()).collect();
    w.tag(0, "SECTION");
    w.tag(2, "OBJECTS");
    w.tag(0, "DICTIONARY");
    w.tag(5, root.clone());
    w.tag(330, "0");
    w.tag(100, "AcDbDictionary");
    w.tag(281, "1");
    w.tag(3, "CAVECAD");
    w.tag(350, ours.clone());
    w.tag(0, "DICTIONARY");
    w.tag(5, ours.clone());
    w.tag(330, root);
    w.tag(100, "AcDbDictionary");
    w.tag(281, "1");
    w.tag(3, "META");
    w.tag(350, meta.clone());
    for (i, h) in record_handles.iter().enumerate() {
        w.tag(3, format!("DOC_{i:04}"));
        w.tag(350, h.clone());
    }
    let xrecord = |w: &mut Writer, handle: String, owner: &str, strings: &[String]| {
        w.tag(0, "XRECORD");
        w.tag(5, handle);
        w.tag(330, owner);
        w.tag(100, "AcDbXrecord");
        w.tag(280, "1");
        for s in strings {
            debug_assert!(s.len() <= MAX_VALUE_LINE);
            w.tag(1, s.clone());
        }
    };
    xrecord(
        &mut w,
        meta,
        &ours,
        &[
            "CAVECAD-DOC".into(),
            format!("version={FORMAT_VERSION}"),
            format!("bytes={}", payload.len()),
            format!("chunks={}", chunks.len()),
            format!("records={}", groups.len()),
            format!("hash={}", checksum(&payload)),
            format!("geometry={geometry}"),
        ],
    );
    for (h, group) in record_handles.iter().zip(&groups) {
        xrecord(
            &mut w,
            h.clone(),
            &ours,
            &group.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
    }
    w.tag(0, "ENDSEC");
    let objects = w.drain();

    // ---- header (last, because it records the final handle) ----
    let mut head = Writer::new();
    head.tag(0, "SECTION");
    head.tag(2, "HEADER");
    head.tag(9, "$ACADVER");
    head.tag(1, "AC1015");
    head.tag(9, "$INSUNITS");
    head.tag(70, "6");
    head.tag(9, "$DWGCODEPAGE");
    head.tag(3, "ANSI_1252");
    if let Some(b) = scene.bounds {
        for (name, v) in [
            ("$EXTMIN", [b[0], b[1], b[2]]),
            ("$EXTMAX", [b[3], b[4], b[5]]),
        ] {
            head.tag(9, name);
            for (i, c) in v.iter().enumerate() {
                head.num(10 + 10 * i as i32, *c);
            }
        }
    }
    head.tag(9, "$HANDSEED");
    head.tag(5, format!("{:X}", w.peek_handle()));
    head.tag(0, "ENDSEC");

    let mut file = head.drain();
    file.extend(tables);
    file.extend(blocks);
    file.push(Tag {
        code: 0,
        value: "SECTION".into(),
    });
    file.push(Tag {
        code: 2,
        value: "ENTITIES".into(),
    });
    file.extend(entity_tags);
    file.push(Tag {
        code: 0,
        value: "ENDSEC".into(),
    });
    file.extend(objects);
    file.push(Tag {
        code: 0,
        value: "EOF".into(),
    });
    render(&file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_is_made_ascii_without_changing_meaning() {
        let j = ascii_json("{\"n\":\"Cueva \u{d1}and\u{fa} \u{1F573}\"}");
        assert!(
            j.is_ascii() && !j.contains(' '),
            "no spaces survive to be trimmed"
        );
        assert!(
            j.contains("\\u00d1") && j.contains("\\ud83d\\udd73"),
            "astral characters become surrogate pairs"
        );
    }
}
