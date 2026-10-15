//! The entry point.

use crate::curve::{Flat, dist};
use crate::guides;
use crate::mode::Mode;
use crate::points::{self, Ctx};
use crate::rank::ranked;
use crate::relational;
use crate::scene::{Guide, Scene, Session, Settings, SnapResult};

/// Finds what the cursor (pixels) can snap to.
pub fn snap(scene: &Scene, cursor: [f64; 2], settings: &Settings, session: &Session) -> SnapResult {
    let reference = matches!(
        session.override_mode,
        Some(Mode::From | Mode::TrackPoint | Mode::MidBetween)
    );
    let only = session.override_mode.filter(|_| !reference);
    let on = |m: Mode| match only {
        Some(o) => m == o,
        None => settings.modes.contains(&m),
    };
    let ctx = Ctx {
        camera: &scene.camera,
        cursor,
        aperture: settings.aperture,
        on: &on,
    };

    // In id order, so the answer never depends on the order the host listed things in.
    let mut things: Vec<&crate::scene::Thing> = scene.things.iter().collect();
    things.sort_by(|a, b| a.id.cmp(&b.id));
    let flats: Vec<Flat> = things
        .iter()
        .filter_map(|t| Flat::of(t, &scene.camera))
        .collect();
    let near_enough = settings.aperture * 1.5;
    let nearby: Vec<Flat> = things
        .iter()
        .filter_map(|t| Flat::of(t, &scene.camera))
        .filter(|f| {
            f.nearest_px(cursor)
                .is_some_and(|(d, _, _)| d <= near_enough)
        })
        .collect();

    let mut all = Vec::new();
    for t in &things {
        let flat = flats.iter().find(|f| f.thing.id == t.id);
        all.extend(points::of_thing(&ctx, t, flat));
    }
    if on(Mode::Nearest) {
        all.extend(relational::nearest(&ctx, &nearby));
    }
    if let Some(last) = session.last_point {
        if on(Mode::Perpendicular) {
            all.extend(relational::perpendicular(&ctx, &nearby, last));
        }
        if on(Mode::Tangent) {
            all.extend(relational::tangent(&ctx, &nearby, last));
        }
    }
    if on(Mode::Intersection) || on(Mode::ApparentIntersection) {
        all.extend(relational::intersections(
            &ctx,
            &nearby,
            settings.intersection_tol,
        ));
    }
    if only == Some(Mode::ExtendedIntersection) {
        all.extend(relational::extended_intersection(&ctx, &flats));
    }

    // Guides: extension continuations, parallels, tracking and polar lines.
    let accept = |m: Mode| match only {
        Some(o) => m == o,
        None => settings.modes.contains(&m),
    };
    let lines = guides::lines(scene, settings, session, &accept);
    let mut shown: Vec<Guide> = Vec::new();
    for l in &lines {
        if let Some((c, g)) = guides::pull(scene, settings, l, cursor) {
            all.push(c);
            shown.push(g);
        }
    }
    // Two straight guides crossing under the cursor: the crossing is a place of its own.
    if let Some(plane) = &scene.plane {
        for (i, a) in lines.iter().enumerate() {
            for b in &lines[i + 1..] {
                if let Some(w) = guides::cross(plane, a, b)
                    && let Some(screen) = scene.camera.project(w)
                {
                    let d = dist(screen, cursor);
                    if d <= settings.aperture {
                        let mode = if matches!(a.kind, crate::scene::GuideKind::Extension)
                            && matches!(b.kind, crate::scene::GuideKind::Extension)
                        {
                            Mode::ExtendedIntersection
                        } else {
                            Mode::Tracking
                        };
                        all.push(crate::scene::Candidate {
                            mode,
                            screen,
                            world: w,
                            source: None,
                            label: mode.name().into(),
                            dist: d * 0.5,
                        });
                    }
                }
            }
        }
    }

    // A guide crossing a real line is an Intersection.
    if on(Mode::Intersection) {
        for l in &lines {
            if !matches!(l.kind, crate::scene::GuideKind::Extension) {
                continue;
            }
            let px = guides::pull_px(&scene.camera, l);
            for f in &nearby {
                for j in 0..f.segments() {
                    let Some(sb) = f.seg_px(j) else { continue };
                    for w in px.windows(2) {
                        if let Some((_, u)) = relational::cross_segments((w[0], w[1]), sb) {
                            let (_, world, (seg, t)) = f.at(j, u);
                            if let Some(screen) = scene.camera.project(world) {
                                let d = dist(screen, cursor);
                                all.push(crate::scene::Candidate {
                                    mode: Mode::Intersection,
                                    screen,
                                    world,
                                    source: Some(crate::scene::Source {
                                        id: f.thing.id.clone(),
                                        seg,
                                        t,
                                    }),
                                    label: "Intersection".into(),
                                    dist: d,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let mut list = ranked(all, settings.aperture);
    let best = if list.is_empty() {
        None
    } else {
        Some(list.remove(0))
    };

    // What hovering here could acquire: an end to extend from, a segment to run parallel to, any snap point to track from.
    let mut acquirable = Vec::new();
    for f in &nearby {
        acquirable.extend(guides::acquirable(f.thing, f, cursor, settings.aperture));
    }
    if let Some(b) = &best
        && b.mode.tier() == crate::mode::Tier::Point
    {
        acquirable.push(crate::scene::Acquired::Track { at: b.world });
    }
    SnapResult {
        best,
        others: list,
        guides: shown,
        acquirable,
    }
}

/// The point `distance` along a guide from its origin (the typed distance of a pick), or `None` for an arc.
pub fn along_guide(guide: &Guide, distance: f64) -> Option<[f64; 3]> {
    let d = guide.dir?;
    Some([
        guide.origin[0] + d[0] * distance,
        guide.origin[1] + d[1] * distance,
        guide.origin[2] + d[2] * distance,
    ])
}
