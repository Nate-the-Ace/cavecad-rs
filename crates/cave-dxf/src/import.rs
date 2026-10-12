//! Reading a DXF back: the embedded cave is restored exactly; the drawing is only counted.

use crate::FORMAT_VERSION;
use crate::tags::{Tag, TagError, checksum, geometry_hash, parse_tags, sections};
use cave_doc::{ActorId, Document, JsonError};
use std::collections::HashMap;

/// Why a file could not be read. None of these leaves a half-loaded cave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// Not a DXF (no readable group codes, or no sections).
    NotDxf,
    /// Binary DXF; only the text form is supported.
    Binary,
    /// The cave inside is incomplete, altered or damaged; the reason is in plain words.
    Corrupt(String),
    /// Written by a newer version of the app than this one.
    NewerVersion {
        /// Version found.
        found: u32,
        /// Newest this build reads.
        supported: u32,
    },
}

/// Something worth telling the person, that did not stop the file loading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    /// A DXF with no cave inside (drawn in another program): nothing was loaded.
    NoCaveData,
    /// The drawing no longer matches what was written (the file was saved by another program, or
    /// edited). The cave inside was used; changes made to the drawing geometry are not applied.
    SavedElsewhere,
}

/// What reading produced.
#[derive(Debug)]
pub struct Imported {
    /// The restored cave, or `None` for a foreign DXF.
    pub document: Option<Document>,
    /// Notes for the person.
    pub warnings: Vec<Warning>,
    /// Drawing entities in the file (lines, points, text, polylines).
    pub entities: usize,
}

type Sections = HashMap<String, Vec<Tag>>;

struct Dictionary {
    entries: Vec<(String, String)>,
}

#[derive(Default)]
struct Objects {
    dictionaries: Vec<Dictionary>,
    xrecords: HashMap<String, Vec<String>>,
}

fn read_objects(body: &[Tag]) -> Objects {
    let mut out = Objects::default();
    let mut i = 0;
    while i < body.len() {
        if body[i].code != 0 {
            i += 1;
            continue;
        }
        let kind = body[i].value.trim().to_string();
        let mut j = i + 1;
        while j < body.len() && body[j].code != 0 {
            j += 1;
        }
        let object = &body[i + 1..j];
        match kind.as_str() {
            "DICTIONARY" => {
                let mut entries = Vec::new();
                let mut name: Option<String> = None;
                for t in object {
                    match t.code {
                        3 => name = Some(t.value.clone()),
                        350 | 360 => {
                            if let Some(n) = name.take() {
                                entries.push((n, t.value.trim().to_string()));
                            }
                        }
                        _ => {}
                    }
                }
                out.dictionaries.push(Dictionary { entries });
            }
            "XRECORD" => {
                let handle = object
                    .iter()
                    .find(|t| t.code == 5)
                    .map(|t| t.value.trim().to_string());
                if let Some(h) = handle {
                    out.xrecords.insert(
                        h,
                        object
                            .iter()
                            .filter(|t| t.code == 1)
                            .map(|t| t.value.clone())
                            .collect(),
                    );
                }
            }
            _ => {}
        }
        i = j;
    }
    out
}

fn entity_count(body: &[Tag]) -> usize {
    body.iter()
        .filter(|t| {
            t.code == 0 && !matches!(t.value.trim(), "VERTEX" | "SEQEND" | "ATTRIB" | "ENDSEC")
        })
        .count()
}

fn meta_value<'a>(meta: &'a [String], key: &str) -> Option<&'a str> {
    meta.iter()
        .find_map(|s| s.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// Reads a DXF. Never panics; a damaged or unfamiliar file is an error or an empty result, never a
/// partly loaded cave.
pub fn import(text: &str, actor: ActorId) -> Result<Imported, ImportError> {
    let tags = parse_tags(text).map_err(|e| match e {
        TagError::Binary => ImportError::Binary,
        TagError::BadCode(_) => ImportError::NotDxf,
    })?;
    let secs = sections(&tags);
    if secs.is_empty() {
        return Err(ImportError::NotDxf);
    }
    let entities = secs.get("ENTITIES").map_or(0, |b| entity_count(b));
    let objects = secs
        .get("OBJECTS")
        .map(|b| read_objects(b))
        .unwrap_or_default();

    // The primary copy lives in our dictionary (the one with a META entry). If it is missing or
    // damaged, the copy on the hidden carrier points is tried: other programs tend to keep extended
    // data when they re-save a drawing, but drop dictionaries they do not know.
    let primary = match objects
        .dictionaries
        .iter()
        .find(|d| d.entries.iter().any(|(n, _)| n == "META"))
    {
        Some(dict) => read_dictionary(dict, &objects, &secs, actor).map(Some),
        None => Ok(None),
    };
    match primary {
        Ok(Some((document, warnings))) => Ok(Imported {
            document: Some(document),
            warnings,
            entities,
        }),
        other => {
            let carried = secs.get("ENTITIES").and_then(|b| read_carriers(b, actor));
            match (carried, other) {
                (Some(Ok(document)), _) => Ok(Imported {
                    document: Some(document),
                    warnings: vec![Warning::SavedElsewhere],
                    entities,
                }),
                (Some(Err(e)), _) => Err(e),
                (None, Err(e)) => Err(e),
                (None, _) => Ok(Imported {
                    document: None,
                    warnings: vec![Warning::NoCaveData],
                    entities,
                }),
            }
        }
    }
}

/// Reads the primary copy from our dictionary, verifying length, piece count and checksum.
fn read_dictionary(
    dict: &Dictionary,
    objects: &Objects,
    secs: &Sections,
    actor: ActorId,
) -> Result<(Document, Vec<Warning>), ImportError> {
    let corrupt = |why: &str| ImportError::Corrupt(why.to_string());
    let strings = |name: &str| -> Option<&Vec<String>> {
        let handle = &dict.entries.iter().find(|(n, _)| n == name)?.1;
        objects.xrecords.get(handle)
    };
    let meta = strings("META").ok_or_else(|| corrupt("the cave's header record is missing"))?;
    if meta.first().map(String::as_str) != Some("CAVECAD-DOC") {
        return Err(corrupt("the cave's header record is not ours"));
    }
    let number = |key: &str| -> Result<usize, ImportError> {
        meta_value(meta, key)
            .and_then(|v| v.trim().parse().ok())
            .ok_or_else(|| ImportError::Corrupt(format!("the header has no usable {key}")))
    };
    let version = number("version")? as u32;
    if version > FORMAT_VERSION {
        return Err(ImportError::NewerVersion {
            found: version,
            supported: FORMAT_VERSION,
        });
    }
    let (bytes, chunks, records) = (number("bytes")?, number("chunks")?, number("records")?);
    let hash = meta_value(meta, "hash").ok_or_else(|| corrupt("the header has no checksum"))?;

    let mut payload = String::with_capacity(bytes.min(1 << 28));
    let mut chunk_count = 0;
    for i in 0..records {
        let part = strings(&format!("DOC_{i:04}")).ok_or_else(|| {
            ImportError::Corrupt(format!(
                "part {i} of the cave is missing (the file was cut short or edited)"
            ))
        })?;
        chunk_count += part.len();
        for s in part {
            payload.push_str(s);
        }
    }
    if chunk_count != chunks {
        return Err(corrupt("the cave has the wrong number of pieces"));
    }
    if payload.len() != bytes {
        return Err(corrupt("the cave is the wrong length"));
    }
    if checksum(&payload) != hash {
        return Err(corrupt(
            "the cave's checksum does not match: the file was damaged or altered",
        ));
    }
    let document = Document::from_json(&payload, actor).map_err(json_error)?;

    let mut warnings = Vec::new();
    let written = meta_value(meta, "geometry");
    let now = secs.get("ENTITIES").map(|b| geometry_hash(b));
    if written.is_some() && now.as_deref() != written {
        warnings.push(Warning::SavedElsewhere);
    }
    Ok((document, warnings))
}

fn json_error(e: JsonError) -> ImportError {
    match e {
        JsonError::NewerSchema { found, supported } => {
            ImportError::NewerVersion { found, supported }
        }
        JsonError::Parse(m) => ImportError::Corrupt(format!("the cave could not be read: {m}")),
    }
}

/// The header words stored on every carrier point; anything else on it is a chunk of the cave.
const CARRIER_KEYS: [&str; 6] = ["version", "part", "parts", "bytes", "chunks", "hash"];

struct CarrierPart {
    index: usize,
    chunks: Vec<String>,
    keys: Vec<(String, String)>,
}

/// The second copy: hidden points whose extended data (application `CAVECAD`) carries the cave.
/// `None` when the file has no such points.
fn read_carriers(body: &[Tag], actor: ActorId) -> Option<Result<Document, ImportError>> {
    let mut parts: Vec<CarrierPart> = Vec::new();
    let mut i = 0;
    while i < body.len() {
        if body[i].code != 0 {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < body.len() && body[j].code != 0 {
            j += 1;
        }
        let object = &body[i + 1..j];
        if let Some(x) = object
            .iter()
            .position(|t| t.code == 1001 && t.value.trim() == "CAVECAD")
        {
            let strings: Vec<&str> = object[x + 1..]
                .iter()
                .take_while(|t| t.code == 1000)
                .map(|t| t.value.as_str())
                .collect();
            if strings.first() == Some(&"CAVECAD-DOC-PART") {
                let mut part = CarrierPart {
                    index: usize::MAX,
                    chunks: Vec::new(),
                    keys: Vec::new(),
                };
                for s in &strings[1..] {
                    match s.split_once('=') {
                        Some((k, v)) if CARRIER_KEYS.contains(&k) => {
                            part.keys.push((k.to_string(), v.trim().to_string()))
                        }
                        _ => part.chunks.push((*s).to_string()),
                    }
                }
                if let Some(n) = part
                    .keys
                    .iter()
                    .find(|(k, _)| k == "part")
                    .and_then(|(_, v)| v.parse().ok())
                {
                    part.index = n;
                    parts.push(part);
                }
            }
        }
        i = j;
    }
    if parts.is_empty() {
        return None;
    }
    parts.sort_by_key(|p| p.index);
    let corrupt = |why: &str| Some(Err(ImportError::Corrupt(why.to_string())));
    let get = |k: &str| {
        parts[0]
            .keys
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    let (Some(total), Some(bytes), Some(hash)) = (
        get("parts").and_then(|v| v.parse::<usize>().ok()),
        get("bytes").and_then(|v| v.parse::<usize>().ok()),
        get("hash"),
    ) else {
        return corrupt("the backup copy of the cave has no usable header");
    };
    if let Some(v) = get("version").and_then(|v| v.parse::<u32>().ok())
        && v > FORMAT_VERSION
    {
        return Some(Err(ImportError::NewerVersion {
            found: v,
            supported: FORMAT_VERSION,
        }));
    }
    if parts.len() != total || parts.iter().enumerate().any(|(n, p)| p.index != n) {
        return corrupt("the backup copy of the cave is missing pieces");
    }
    let payload: String = parts
        .iter()
        .flat_map(|p| p.chunks.iter())
        .map(String::as_str)
        .collect();
    if payload.len() != bytes || checksum(&payload) != hash {
        return corrupt("the backup copy of the cave failed its checksum");
    }
    Some(Document::from_json(&payload, actor).map_err(json_error))
}
