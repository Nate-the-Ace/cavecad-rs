//! The DXF safety checks: what must always be true of a saved file.

use cave_doc::{Action, ActorId, Document, Field, Kind, Value};
use cave_dxf::{ExportOptions, ImportError, Imported, MAX_VALUE_LINE, Warning, export, import};
use cave_id::SeqIds;

const CAVES: [&str; 4] = [
    "PitfallCave.json",
    "FingerprintCave.json",
    "PlumblineIT",
    "StairstepCave.json",
];

fn load(name: &str) -> Document {
    let name = if name == "PlumblineIT" {
        "PlumblinePit.json"
    } else {
        name
    };
    let path = format!(
        "{}/../cave-survey/tests/golden/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap()
}

/// A cave with the awkward things in it: sketches with pen pressure, a removed shot, names with
/// accents and symbols, very long and space-padded text.
fn awkward() -> Document {
    let mut doc = load("PitfallCave.json");
    let frames = cave_scene::profile_frames(&doc, "A", true).unwrap();
    let (_, actions) = cave_draw::profile_sketch(
        &doc,
        &mut SeqIds::new(77),
        "wall",
        "A",
        &[[1.0, -1.0], [3.0, -2.0], [6.0, -2.5]],
        Some(vec![0.4, 1.2, 1.6]),
        &frames,
    )
    .unwrap();
    doc.edit(actions).unwrap();
    let shot = doc.ids_of(Kind::Shot)[3];
    doc.edit(vec![Action::Remove {
        id: shot,
        kind: Kind::Shot,
    }])
    .unwrap();
    let station = doc.ids_of(Kind::Station)[0];
    doc.edit(vec![Action::Set {
        id: station,
        field: Field::StationLabel,
        value: Value::Text("Cueva \u{d1}and\u{fa} \u{2603}  \u{1F573} ".into()),
    }])
    .unwrap();
    let trip = doc.ids_of(Kind::Trip)[0];
    doc.edit(vec![Action::Set {
        id: trip,
        field: Field::TripTeam,
        value: Value::Text("x ".repeat(3000)),
    }])
    .unwrap();
    doc
}

#[test]
fn every_test_cave_round_trips_exactly() {
    for name in CAVES {
        let doc = load(name);
        let text = export(&doc, &ExportOptions::default());
        let back = import(&text, ActorId(2)).unwrap();
        assert_eq!(
            back.document.as_ref(),
            Some(&doc),
            "{name}: the restored cave equals the saved one"
        );
        assert!(back.warnings.is_empty(), "{name}: {:?}", back.warnings);
        assert!(
            back.entities > 50,
            "{name}: the drawing is in the file ({} entities)",
            back.entities
        );
    }
}

#[test]
fn awkward_content_round_trips_exactly() {
    let doc = awkward();
    let back = import(&export(&doc, &ExportOptions::default()), ActorId(2)).unwrap();
    assert_eq!(
        back.document.as_ref(),
        Some(&doc),
        "sketches, removed shots, unicode and padded text survive"
    );
}

#[test]
fn adjusted_or_raw_drawing_does_not_change_the_cave_inside() {
    let doc = load("StairstepCave.json");
    let a = import(&export(&doc, &ExportOptions { adjusted: true }), ActorId(2))
        .unwrap()
        .document;
    let b = import(
        &export(&doc, &ExportOptions { adjusted: false }),
        ActorId(2),
    )
    .unwrap()
    .document;
    assert_eq!(a, b);
    assert_eq!(a.as_ref(), Some(&doc));
}

#[test]
fn the_file_obeys_the_writing_rules() {
    for doc in [load("PitfallCave.json"), awkward()] {
        let text = export(&doc, &ExportOptions::default());
        assert!(
            text.is_ascii(),
            "pure ASCII: no code page can damage a name"
        );
        assert!(text.ends_with("EOF\n"));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len() % 2, 0, "group code / value pairs");
        let mut longest = 0;
        for pair in lines.chunks(2) {
            assert!(
                pair[0].trim().parse::<i32>().is_ok(),
                "group code {:?} is a number",
                pair[0]
            );
            longest = longest.max(pair[1].len());
        }
        assert!(
            longest <= MAX_VALUE_LINE,
            "the longest value line is {longest} bytes (an over-long line once made the old reader drop the rest of a file)"
        );
        assert!(
            longest > 100,
            "the embedded cave really is chunked, not absent"
        );
    }
}

#[test]
fn handles_are_unique() {
    let text = export(&load("PitfallCave.json"), &ExportOptions::default());
    let lines: Vec<&str> = text.lines().collect();
    let mut seen = std::collections::HashSet::new();
    for pair in lines.chunks(2) {
        if pair[0].trim() == "5" {
            assert!(
                seen.insert(pair[1].to_string()),
                "handle {} used twice",
                pair[1]
            );
        }
    }
    assert!(seen.len() > 100);
}

fn replace_in(text: &str, from: &str, to: &str) -> String {
    text.replacen(from, to, 1)
}

#[test]
fn an_altered_primary_copy_is_caught_and_the_backup_is_used() {
    let doc = load("FingerprintCave.json");
    let text = export(&doc, &ExportOptions::default());
    // Flip one character inside a chunk of the primary copy (the first long line in OBJECTS).
    let objects = text.find("OBJECTS").unwrap();
    let mut offset = objects;
    let mut target = None;
    for line in text[objects..].split_inclusive('\n') {
        if line.len() > 150 {
            target = Some(offset + 20);
            break;
        }
        offset += line.len();
    }
    let at = target.expect("the embedded cave has long chunk lines");
    let mut damaged = text.clone().into_bytes();
    damaged[at] = if damaged[at] == b'x' { b'y' } else { b'x' };
    let damaged = String::from_utf8(damaged).unwrap();
    assert_ne!(damaged, text);
    let back = import(&damaged, ActorId(2)).unwrap();
    assert_eq!(
        back.document.as_ref(),
        Some(&doc),
        "the checksum caught the damage; the intact backup restored the exact cave"
    );
    assert_eq!(back.warnings, vec![Warning::SavedElsewhere]);
}

#[test]
fn a_truncated_file_never_loads_partly() {
    let doc = load("PitfallCave.json");
    let text = export(&doc, &ExportOptions::default());
    let objects_at = text.find("OBJECTS").unwrap();
    for cut in [
        10,
        500,
        objects_at / 2,
        objects_at - 5,
        objects_at + 200,
        objects_at + 2000,
        text.len() / 2,
        text.len() - 400,
        text.len() - 5,
    ] {
        let cut = (0..=cut).rev().find(|&c| text.is_char_boundary(c)).unwrap();
        if let Ok(Imported {
            document: Some(d), ..
        }) = import(&text[..cut], ActorId(2))
        {
            assert_eq!(
                d, doc,
                "cut at {cut}: a cave that loads from a cut file is the whole, checked cave"
            );
        }
    }
}

#[test]
fn a_file_with_no_cave_inside_is_reported_not_guessed() {
    let foreign = "  0\nSECTION\n  2\nENTITIES\n  0\nLINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 30\n0.0\n 11\n5.0\n 21\n0.0\n 31\n0.0\n  0\nPOINT\n  8\n0\n 10\n1.0\n 20\n1.0\n 30\n0.0\n  0\nENDSEC\n  0\nEOF\n";
    let r = import(foreign, ActorId(1)).unwrap();
    assert!(r.document.is_none());
    assert_eq!(r.entities, 2);
    assert_eq!(r.warnings, vec![Warning::NoCaveData]);
}

#[test]
fn a_long_value_line_in_a_foreign_file_does_not_throw_the_reader_off() {
    // The old app's reader lost step after a line this long and dropped everything after it.
    let long = "AreaFillSig=WATER|49634436|0.067|1||504485,344740;".repeat(40);
    let foreign = format!(
        "  0\nSECTION\n  2\nENTITIES\n  0\nLINE\n  8\n0\n1001\nQCAD\n1000\n{long}\n  0\nPOINT\n  8\n0\n 10\n42.0\n 20\n17.0\n 30\n0.0\n  0\nENDSEC\n  0\nEOF\n"
    );
    assert!(long.len() > 1500);
    assert_eq!(
        import(&foreign, ActorId(1)).unwrap().entities,
        2,
        "the point after the long line is still found"
    );
}

#[test]
fn nonsense_is_refused() {
    assert_eq!(import("", ActorId(1)).unwrap_err(), ImportError::NotDxf);
    assert_eq!(
        import("hello world\nnot a dxf\n", ActorId(1)).unwrap_err(),
        ImportError::NotDxf
    );
    assert_eq!(
        import("AutoCAD Binary DXF\r\n\x1a\0", ActorId(1)).unwrap_err(),
        ImportError::Binary
    );
}

#[test]
fn a_file_from_a_newer_version_is_refused() {
    let text = export(&load("FingerprintCave.json"), &ExportOptions::default());
    let newer = text.replace("version=1", "version=9");
    assert_eq!(
        import(&newer, ActorId(2)).unwrap_err(),
        ImportError::NewerVersion {
            found: 9,
            supported: 1
        }
    );
}

#[test]
fn a_file_saved_by_another_program_still_loads_with_a_warning() {
    let doc = load("FingerprintCave.json");
    let text = export(&doc, &ExportOptions::default());
    // Another program moves a point: the drawing no longer matches what was written.
    let entities_at = text.find("ENTITIES").unwrap();
    let (head, tail) = text.split_at(entities_at);
    let moved = format!("{head}{}", replace_in(tail, "\n 10\n", "\n 10\n9"));
    assert_ne!(moved, text);
    let back = import(&moved, ActorId(2)).unwrap();
    assert_eq!(
        back.document.as_ref(),
        Some(&doc),
        "the cave inside is what loads"
    );
    assert_eq!(back.warnings, vec![Warning::SavedElsewhere]);
}

#[test]
fn windows_line_endings_are_fine() {
    let doc = load("FingerprintCave.json");
    let crlf = export(&doc, &ExportOptions::default()).replace('\n', "\r\n");
    assert_eq!(
        import(&crlf, ActorId(2)).unwrap().document.as_ref(),
        Some(&doc)
    );
}

/// Cheap deterministic generator for the fuzz loop.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn the_reader_never_panics_on_damaged_input() {
    let text = export(&load("FingerprintCave.json"), &ExportOptions::default());
    let bytes = text.as_bytes();
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for round in 0..400 {
        let mut b = bytes.to_vec();
        match round % 4 {
            0 => b.truncate((rng.next() as usize) % b.len()),
            1 => {
                for _ in 0..(1 + rng.next() % 20) {
                    let i = (rng.next() as usize) % b.len();
                    b[i] = (rng.next() % 256) as u8;
                }
            }
            2 => {
                let i = (rng.next() as usize) % b.len();
                let j = i + (rng.next() as usize) % (b.len() - i).min(2000);
                b.drain(i..j);
            }
            _ => {
                let i = (rng.next() as usize) % b.len();
                let junk: Vec<u8> = (0..(rng.next() % 300))
                    .map(|_| (rng.next() % 256) as u8)
                    .collect();
                b.splice(i..i, junk);
            }
        }
        let damaged = String::from_utf8_lossy(&b).into_owned();
        // Anything but a panic is acceptable; a loaded cave must be the whole, checked cave.
        if let Ok(Imported {
            document: Some(d), ..
        }) = import(&damaged, ActorId(2))
        {
            assert_eq!(
                d,
                load("FingerprintCave.json"),
                "round {round}: a cave that loads is the exact original"
            );
        }
    }
}

/// Removes the OBJECTS section, as a program that does not keep dictionaries would on re-saving.
fn without_objects(text: &str) -> String {
    let start = text.find("  0\nSECTION\n  2\nOBJECTS\n").unwrap();
    let end = text[start..].find("  0\nENDSEC\n").unwrap() + start + "  0\nENDSEC\n".len();
    format!("{}{}", &text[..start], &text[end..])
}

#[test]
fn the_cave_survives_a_program_that_drops_the_dictionary() {
    let doc = awkward();
    let resaved = without_objects(&export(&doc, &ExportOptions::default()));
    assert!(!resaved.contains("XRECORD"));
    let back = import(&resaved, ActorId(2)).unwrap();
    assert_eq!(
        back.document.as_ref(),
        Some(&doc),
        "the backup copy on the hidden points restores the whole cave"
    );
    assert_eq!(back.warnings, vec![Warning::SavedElsewhere]);
}

#[test]
fn a_damaged_primary_copy_falls_back_to_the_backup() {
    let doc = load("FingerprintCave.json");
    let text = export(&doc, &ExportOptions::default());
    let objects = text.find("OBJECTS").unwrap();
    let mut off = objects;
    let mut at = None;
    for line in text[objects..].split_inclusive('\n') {
        if line.len() > 150 {
            at = Some(off + 30);
            break;
        }
        off += line.len();
    }
    let mut b = text.clone().into_bytes();
    let at = at.unwrap();
    b[at] = if b[at] == b'x' { b'y' } else { b'x' };
    let back = import(&String::from_utf8(b).unwrap(), ActorId(2)).unwrap();
    assert_eq!(back.document.as_ref(), Some(&doc));
}

#[test]
fn if_both_copies_are_damaged_it_is_an_error() {
    let text = export(&load("FingerprintCave.json"), &ExportOptions::default());
    // Damage every long chunk line, in both copies.
    let broken: String = text
        .split_inclusive('\n')
        .map(|l| {
            if l.len() > 150 {
                l.replacen('a', "b", 1).replacen('e', "f", 1)
            } else {
                l.to_string()
            }
        })
        .collect();
    assert_ne!(broken, text);
    assert!(
        matches!(import(&broken, ActorId(2)), Err(ImportError::Corrupt(_))),
        "never a half-loaded cave"
    );
}

#[test]
fn the_backup_points_sit_on_a_frozen_layer() {
    let text = export(&load("FingerprintCave.json"), &ExportOptions::default());
    let at = text
        .find("CAVECAD-DATA\n  70\n1\n")
        .or_else(|| text.find("CAVECAD-DATA\n 70\n1\n"));
    assert!(
        at.is_some(),
        "the data layer is frozen so the carrier points are not drawn"
    );
}

/// The extended data of every entity that has some under application `app`: a list of its strings.
fn xdata_of(text: &str, app: &str) -> Vec<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if lines[i].trim() == "1001" && lines[i + 1] == app {
            let mut strings = Vec::new();
            let mut j = i + 2;
            while j + 1 < lines.len() && lines[j].trim() == "1000" {
                strings.push(lines[j + 1].to_string());
                j += 2;
            }
            out.push(strings);
            i = j;
        } else {
            i += 2;
        }
    }
    out
}

#[test]
fn drawn_things_carry_their_data_as_cave_survey_xdata() {
    let doc = load("PitfallCave.json");
    let text = export(&doc, &ExportOptions::default());
    assert!(
        text.contains("\nCaveSurvey\n"),
        "the application is registered and used"
    );
    let tags = xdata_of(&text, "CaveSurvey");
    let stations: Vec<&Vec<String>> = tags
        .iter()
        .filter(|t| t.iter().any(|s| s.starts_with("Station=")))
        .collect();
    let legs: Vec<&Vec<String>> = tags
        .iter()
        .filter(|t| t.iter().any(|s| s.starts_with("From=")) && !t.iter().any(|s| s == "Splay=1"))
        .collect();
    let splays: Vec<&Vec<String>> = tags
        .iter()
        .filter(|t| t.iter().any(|s| s == "Splay=1"))
        .collect();
    assert_eq!(
        stations.len(),
        cave_scene::build(&doc, true).stations.len(),
        "every drawn station is tagged (an excluded station is not drawn; its data is safe in the embedded cave)"
    );
    assert!(
        stations
            .iter()
            .all(|t| t.iter().any(|s| s.starts_with("Seq=")))
    );
    assert!(legs.len() > 50 && !splays.is_empty());
    let first = legs[0];
    for key in [
        "From=",
        "To=",
        "Trip=",
        "ShotSeq=",
        "Distance=",
        "Azimuth=",
        "Inclination=",
    ] {
        assert!(
            first.iter().any(|s| s.starts_with(key)),
            "a leg carries {key}: {first:?}"
        );
    }
    assert!(
        tags.iter().flatten().all(|s| s.len() <= 255),
        "within the extended-data string limit"
    );
}

#[test]
fn a_leg_carries_exactly_what_was_surveyed() {
    let text = r#"{"unit":"ft","name":"t","shots":[
        {"from":"A1","to":"A2","distance":30.5,"azimuth":42,"inclination":-2.5,"left":4,"right":5,"leftOpen":true,"up":10,"backAzimuth":222}]}"#;
    let doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
    let tags = xdata_of(&export(&doc, &ExportOptions::default()), "CaveSurvey");
    let leg = tags
        .iter()
        .find(|t| t.iter().any(|s| s == "From=A1"))
        .expect("the leg is tagged");
    for want in [
        "To=A2",
        "Trip=0",
        "Distance=30.5",
        "Azimuth=42",
        "Inclination=-2.5",
        "Left=4/P",
        "Right=5",
        "Up=10",
        "BackAzimuth=222",
    ] {
        assert!(leg.iter().any(|s| s == want), "{want} missing from {leg:?}");
    }
    assert!(
        !leg.iter().any(|s| s.starts_with("Down=")),
        "an unmeasured side writes nothing"
    );
}

#[test]
fn xdata_text_is_ascii_and_single_line() {
    let mut doc = load("FingerprintCave.json");
    let station = doc.ids_of(Kind::Station)[0];
    doc.edit(vec![Action::Set {
        id: station,
        field: Field::StationLabel,
        value: Value::Text("Ca\u{f1}\u{f3}n\\A\nB".into()),
    }])
    .unwrap();
    let text = export(&doc, &ExportOptions::default());
    assert!(text.is_ascii());
    let tags = xdata_of(&text, "CaveSurvey");
    let t = tags
        .iter()
        .find(|t| t.iter().any(|s| s.starts_with("Station=Ca")))
        .unwrap();
    assert!(
        t.iter().any(|s| s == "Station=Ca\\U+00F1\\U+00F3n\\\\A B"),
        "{t:?}"
    );
}

/// The `(colour, linetype)` the LAYER table gives `layer`.
fn layer_look(text: &str, layer: &str) -> (String, String) {
    let tags = cave_dxf::parse_tags(text).expect("parses");
    let at = tags
        .iter()
        .position(|t| t.code == 2 && t.value == layer)
        .and_then(|i| {
            // The record that names the layer sits in the LAYER table.
            (0..=i)
                .rev()
                .find(|&k| tags[k].code == 0)
                .filter(|&k| tags[k].value == "LAYER")
                .map(|_| i)
        })
        .unwrap_or_else(|| panic!("layer {layer} is not in the LAYER table"));
    let value_of = |code: i32| {
        tags[at..]
            .iter()
            .take_while(|t| t.code != 0)
            .find(|t| t.code == code)
            .map(|t| t.value.trim().to_string())
            .unwrap()
    };
    (value_of(62), value_of(6))
}

#[test]
fn layers_get_their_colour_and_linetype_from_the_type_palette() {
    let text = export(&load("PitfallCave.json"), &ExportOptions::default());
    for key in ["wall", "centerline", "splay", "closure", "tie"] {
        let st = cave_style::style(key);
        if !text.contains(st.layer) {
            continue; // this cave has none of that type
        }
        let (color, lt) = layer_look(&text, st.layer);
        assert_eq!(color, cave_style::dxf_color(st.ink).to_string(), "{key}");
        assert_eq!(lt, st.linetype.dxf_name(), "{key}");
    }
    assert!(text.contains("CAVE-WALLS"), "Pitfall has walls");
}

/// A cave just started (one empty trip, nothing else) saves and opens again.
#[test]
fn a_brand_new_cave_round_trips() {
    use cave_id::SeqIds;
    let doc = cave_notebook::new_cave(
        cave_doc::ActorId(1),
        &mut SeqIds::new(1),
        "New Cave",
        "First trip",
        "m",
    )
    .unwrap();
    let text = cave_dxf::export(&doc, &cave_dxf::ExportOptions::default());
    let back = cave_dxf::import(&text, cave_doc::ActorId(2))
        .unwrap()
        .document
        .unwrap();
    assert_eq!(cave_notebook::trips(&back).len(), 1);
    assert_eq!(cave_notebook::trips(&back)[0].name, "First trip");
}
