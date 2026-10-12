//! What the Cave Shelf shows about a cave without opening it: its name, how big it is, each trip's
//! date, team, shots and where it started and stopped, and the open ends where a new trip could begin.
//! Worked out from the cave every time it is needed (the shelf keeps the last answer so it can list
//! caves without reading them), never typed in.

use cave_doc::{Document, Field, Kind};
use cave_id::Id;
use cave_notebook::{RowKind, page, trips};
use serde::Serialize;

/// One trip's line in the shelf's table.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TripSummary {
    /// The trip.
    pub id: Id,
    /// Its place among the trips (0 first).
    pub index: usize,
    /// Trip name.
    pub name: String,
    /// Date, or empty.
    pub date: String,
    /// Who was on it.
    pub team: String,
    /// Declination its compass readings were corrected by, degrees.
    pub declination: Option<f64>,
    /// `ft` or `m`: what its notes were written in.
    pub unit: String,
    /// Shots between stations.
    pub shots: usize,
    /// Surveyed length, metres.
    pub length_m: f64,
    /// Where it started (the first station on its page), or empty.
    pub start: String,
    /// Where it stopped (the last station on its page), or empty.
    pub end: String,
    /// The open ends it left, by station name.
    pub ends: Vec<String>,
}

/// An open end: a station the survey stops at that nobody wrote off, where a new trip could pick up.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OpenEnd {
    /// The station's name.
    pub station: String,
    /// The trip (by position) that reached it last.
    pub trip: usize,
}

/// What the shelf shows about one cave.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CaveSummary {
    /// The cave's name (kept on its first trip), or `Untitled cave`.
    pub name: String,
    /// `ft` or `m`: the unit its first trip was written in, for showing lengths the way its notes read.
    pub unit: String,
    /// Total surveyed length, metres.
    pub length_m: f64,
    /// Depth range from the highest station to the lowest, metres (`None` before anything is placed).
    pub depth_m: Option<f64>,
    /// Stations placed.
    pub stations: usize,
    /// Shots between stations.
    pub shots: usize,
    /// Each trip, in order.
    pub trips: Vec<TripSummary>,
    /// Where the survey still stops, newest trip first.
    pub ends: Vec<OpenEnd>,
    /// Loops that were closed.
    pub loops: usize,
    /// The worst loop's error as a percent of its length, if any loop closed.
    pub worst_percent: Option<f64>,
    /// Nothing surveyed yet (no shots at all).
    pub empty: bool,
}

/// What the shelf shows about a cave.
pub fn summarize(doc: &Document) -> CaveSummary {
    let choices = trips(doc);
    let first = choices.first().and_then(|c| doc.entity(c.id));
    let name = first
        .map(|e| e.text(Field::TripCaveName))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Untitled cave".into());
    let unit = first
        .map(|e| e.text(Field::TripDistanceUnit))
        .filter(|u| u == "ft")
        .unwrap_or_else(|| "m".into());

    let rows = cave_scene::station_rows(doc, true, "");
    let end_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.kinds.contains(&"openEnd"))
        .collect();

    let mut summaries = Vec::new();
    for c in &choices {
        let Some(p) = page(doc, c.id) else { continue };
        let stations: Vec<&str> = p
            .rows
            .iter()
            .filter(|r| r.kind == RowKind::Station && !r.label.is_empty())
            .map(|r| r.label.as_str())
            .collect();
        summaries.push(TripSummary {
            id: c.id,
            index: c.index,
            name: p.name.clone(),
            date: p.date.clone(),
            team: p.team.clone(),
            declination: p.declination,
            unit: p.unit.clone(),
            shots: p.totals.shots,
            length_m: p.totals.length,
            start: stations.first().map(|s| s.to_string()).unwrap_or_default(),
            end: stations.last().map(|s| s.to_string()).unwrap_or_default(),
            ends: end_rows
                .iter()
                .filter(|r| r.trips.last() == Some(&c.index))
                .map(|r| r.station.clone())
                .collect(),
        });
    }

    // Newest trip first, as the old shelf listed them: that is where a new trip usually picks up.
    let mut ends: Vec<OpenEnd> = end_rows
        .iter()
        .map(|r| OpenEnd {
            station: r.station.clone(),
            trip: r.trips.last().copied().unwrap_or(0),
        })
        .collect();
    ends.sort_by(|a, b| b.trip.cmp(&a.trip).then(a.station.cmp(&b.station)));

    let scene = cave_scene::build(doc, true);
    let percents: Vec<f64> = scene.closures.iter().filter_map(|c| c.percent).collect();
    let shots = summaries.iter().map(|t| t.shots).sum();
    CaveSummary {
        name,
        unit,
        length_m: summaries.iter().map(|t| t.length_m).sum(),
        depth_m: scene.bounds.map(|b| b[5] - b[2]),
        stations: scene.stations.len(),
        shots,
        loops: percents.len(),
        worst_percent: percents.into_iter().reduce(f64::max),
        empty: doc.ids_of(Kind::Shot).is_empty(),
        trips: summaries,
        ends,
    }
}
