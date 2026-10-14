//! Everything here must come out exactly as the web app's own TypeScript did: the golden file is written by
//! `tools/golden/gen_instrument_golden.mjs` running the OLD code (survey-sketch before the port).

use cave_instrument::bric::{self, ShotSpec as BricShot};
use cave_instrument::caveble::{self, Leg};
use cave_instrument::cavway::{self, PacketSpec};
use cave_instrument::identify::identify;
use cave_instrument::legs::{LegAssembler, LegEvent, average, is_close};
use cave_instrument::link::{BricKind, BricLink, LegLink, LinkOut, Sap5Link, X1Link};
use cave_instrument::sim::{
    DeviceKind, FlagPolicy, SimOptions, Simulator, Step, TraverseOpts, devices, shots_for, traverse,
};
use cave_instrument::{Mark, Reading, hex, sap5};
use serde_json::{Value, json};

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/instrument.json")).unwrap()
}

/// Drops null-valued keys: the old code wrote `null` where this one leaves a key out.
fn norm(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), norm(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        other => other.clone(),
    }
}

fn same(a: &Value, b: &Value, at: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!(
                (x - y).abs() <= 1e-9 * (1.0 + x.abs().max(y.abs())),
                "{at}: {x} vs {y}"
            );
        }
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<_> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => same(p, q, &format!("{at}.{k}")),
                    (p, q) => panic!("{at}.{k}: {p:?} vs {q:?}"),
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{at}: length");
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                same(p, q, &format!("{at}[{i}]"));
            }
        }
        _ => assert_eq!(a, b, "{at}"),
    }
}

fn check(got: Value, want: &Value, at: &str) {
    same(&norm(&got), &norm(want), at);
}

fn bytes(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|h| u8::from_str_radix(h, 16).unwrap())
        .collect()
}

fn f(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or(0.0)
}

fn mark(v: &Value) -> Mark {
    serde_json::from_value(v.clone()).unwrap_or(Mark::None)
}

#[test]
fn caveble_frames() {
    let g = golden();
    for c in g["caveble"]["frames"].as_array().unwrap() {
        let l = &c["leg"];
        let leg = Leg {
            seq: f(l, "seq") as u8,
            azimuth: f(l, "azimuth"),
            inclination: f(l, "inclination"),
            roll: f(l, "roll"),
            distance: f(l, "distance"),
        };
        let wire = caveble::encode_leg(&leg);
        assert_eq!(hex(&wire), c["hex"].as_str().unwrap());
        let d = caveble::decode_leg(&wire).unwrap();
        let got = json!({"seq": d.seq, "azimuth": d.azimuth, "inclination": d.inclination, "roll": d.roll, "distance": d.distance});
        check(got, &c["decoded"], "caveble");
    }
    for c in g["caveble"]["bad"].as_array().unwrap() {
        assert!(c["decoded"].is_null());
        assert!(caveble::decode_leg(&bytes(c["hex"].as_str().unwrap())).is_none());
    }
}

fn packet_json(p: &cavway::Packet) -> Value {
    json!({
        "type": p.kind, "flag": p.flag, "leg": p.leg, "calibration": p.calibration, "mark": p.mark,
        "distance": p.distance, "azimuth": p.azimuth, "inclination": p.inclination, "roll": p.roll, "time": p.time,
        "sensors": p.sensors.iter().map(|s| json!({"absG": s.abs_g, "absM": s.abs_m, "dip": s.dip})).collect::<Vec<_>>(),
        "errors": p.errors.iter().map(|e| json!({"kind": e.kind.name(), "a": e.a, "b": e.b})).collect::<Vec<_>>(),
        "quality": p.quality,
    })
}

#[test]
fn cavway_packets() {
    let g = golden();
    let cases = g["cavway"].as_array().unwrap();
    assert!(cases.len() > 40);
    for (i, c) in cases.iter().enumerate() {
        let wire = bytes(c["hex"].as_str().unwrap());
        if let Some(s) = c.get("spec") {
            let errors = s.get("errors").map(|e| {
                let mut a = [0u8; 9];
                for (k, v) in e.as_array().unwrap().iter().enumerate() {
                    a[k] = v.as_u64().unwrap() as u8;
                }
                a
            });
            let spec = PacketSpec {
                kind: s.get("type").map_or(1, |t| t.as_u64().unwrap() as u8),
                leg: s.get("leg").is_some_and(|v| v.as_bool().unwrap()),
                mark: s.get("mark").map_or(Mark::None, mark),
                distance: f(s, "distance"),
                azimuth: f(s, "azimuth"),
                inclination: f(s, "inclination"),
                roll: f(s, "roll"),
                time: s.get("time").map_or(0, |t| t.as_u64().unwrap() as u32),
                short: s.get("short").is_some_and(|v| v.as_bool().unwrap()),
                errors,
            };
            assert_eq!(
                hex(&cavway::encode_packet(&spec)),
                c["hex"].as_str().unwrap(),
                "encode {i}"
            );
        }
        match cavway::decode_packet(&wire) {
            Some(p) => check(packet_json(&p), &c["decoded"], &format!("decode {i}")),
            None => assert!(
                c["decoded"].is_null(),
                "decode {i}: expected {:?}",
                c["decoded"]
            ),
        }
    }
}

#[test]
fn sap5_packets() {
    let g = golden();
    for (i, c) in g["sap5"].as_array().unwrap().iter().enumerate() {
        let wire = bytes(c["hex"].as_str().unwrap());
        if let Some(s) = c.get("spec") {
            let w = sap5::encode_packet(
                f(s, "distance"),
                f(s, "azimuth"),
                f(s, "inclination"),
                f(s, "roll"),
                f(s, "seq") as u8,
            );
            assert_eq!(hex(&w), c["hex"].as_str().unwrap(), "encode {i}");
        }
        match sap5::decode_packet(&wire, true) {
            Some(p) => check(
                json!({"seq": p.seq, "distance": p.distance, "azimuth": p.azimuth, "inclination": p.inclination, "roll": p.roll}),
                &c["decoded"],
                &format!("decode {i}"),
            ),
            None => assert!(c["decoded"].is_null(), "decode {i}"),
        }
    }
}

fn errs_json(e: Option<[bric::ErrorPair; 2]>) -> Value {
    e.map_or(Value::Null, |e| {
        Value::Array(
            e.iter()
                .map(|p| json!({"code": p.code, "a": p.a, "b": p.b}))
                .collect(),
        )
    })
}

#[test]
fn bric_indications() {
    let g = golden();
    for (i, c) in g["bric"]["cases"].as_array().unwrap().iter().enumerate() {
        let s = &c["spec"];
        let t = s.get("time");
        let tf = |k: &str, d: u64| t.and_then(|t| t.get(k)).map_or(d, |v| v.as_u64().unwrap());
        let shot = BricShot {
            distance: f(s, "distance"),
            azimuth: f(s, "azimuth"),
            inclination: f(s, "inclination"),
            index: f(s, "index") as u32,
            roll: f(s, "roll"),
            dip: s.get("dip").map_or(60.0, |d| d.as_f64().unwrap()),
            kind: f(s, "type") as u8,
            time: bric::Time {
                year: tf("year", 2026) as u16,
                month: tf("month", 10) as u8,
                day: tf("day", 14) as u8,
                hour: tf("hour", 12) as u8,
                minute: tf("minute", 0) as u8,
                second: tf("second", 0) as u8,
                centisecond: tf("centisecond", 0) as u8,
            },
            errors: s.get("errors").map_or(vec![], |e| {
                e.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| {
                        (
                            x[0].as_u64().unwrap() as u8,
                            x[1].as_f64().unwrap(),
                            x[2].as_f64().unwrap(),
                        )
                    })
                    .collect()
            }),
        };
        let (p, m, e) = bric::encode_shot(&shot);
        assert_eq!(hex(&p), c["primary"].as_str().unwrap(), "primary {i}");
        assert_eq!(hex(&m), c["meta"].as_str().unwrap(), "meta {i}");
        assert_eq!(hex(&e), c["errors"].as_str().unwrap(), "errors {i}");
        let pr = bric::decode_primary(&p).unwrap();
        let me = bric::decode_meta(&m).unwrap();
        let er = bric::decode_errors(&e);
        let want = &c["decoded"];
        check(
            json!({"time": {"year": pr.time.year, "month": pr.time.month, "day": pr.time.day, "hour": pr.time.hour, "minute": pr.time.minute, "second": pr.time.second, "centisecond": pr.time.centisecond}, "distance": pr.distance, "azimuth": pr.azimuth, "inclination": pr.inclination}),
            &want["primary"],
            "primary",
        );
        check(
            json!({"index": me.index, "dip": me.dip, "roll": me.roll, "temperature": me.temperature, "samples": me.samples, "type": me.kind}),
            &want["meta"],
            "meta",
        );
        check(errs_json(er), &want["errors"], "errors");
        assert_eq!(
            bric::describe_errors(er.as_ref()).map_or(Value::Null, Value::from),
            want["said"],
            "said {i}"
        );
    }
    for c in g["bric"]["short"].as_array().unwrap() {
        let b = bytes(c["hex"].as_str().unwrap());
        assert_eq!(bric::decode_primary(&b).is_some(), !c["primary"].is_null());
        assert_eq!(bric::decode_meta(&b).is_some(), !c["meta"].is_null());
        check(
            errs_json(bric::decode_errors(&b)),
            &c["errors"],
            "short errors",
        );
    }
}

fn outs_json(outs: &[LinkOut]) -> Vec<Value> {
    outs.iter()
        .map(|o| match o {
            LinkOut::Frame { hex, from } => json!({"k": "frame", "hex": hex, "from": from}),
            LinkOut::Reading { reading } => json!({"k": "reading", "reading": reading}),
            LinkOut::Send { bytes } => json!({"k": "send", "hex": hex(bytes)}),
        })
        .collect()
}

#[test]
fn links() {
    let g = golden();
    let frames = |k: &str| -> Vec<Vec<u8>> {
        g[k]["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| bytes(h.as_str().unwrap()))
            .collect()
    };

    let mut link = LegLink::new("sim");
    let mut got = Vec::new();
    for b in frames("sap6link") {
        got.extend(outs_json(&link.on_frame(&b, "leg")));
    }
    check(Value::Array(got), &g["sap6link"]["log"], "sap6");

    let mut link = X1Link::new("sim");
    let mut got = Vec::new();
    let all = frames("x1link");
    for b in &all {
        got.extend(outs_json(&link.on_frame(b, "x1")));
    }
    for i in 0..10 {
        let p = cavway::encode_packet(&PacketSpec {
            distance: 7.0 + f64::from(i),
            azimuth: 1.0,
            inclination: 1.0,
            time: 100 + i as u32,
            ..PacketSpec::default()
        });
        got.extend(outs_json(&link.on_frame(&p, "x1")));
    }
    got.extend(outs_json(&link.on_frame(&all[0], "x1")));
    check(Value::Array(got), &g["x1link"]["log"], "x1");

    let mut link = Sap5Link::new("sim");
    let mut got = Vec::new();
    for b in frames("sap5link") {
        got.extend(outs_json(&link.on_frame(&b, "sap5")));
    }
    check(Value::Array(got), &g["sap5link"]["log"], "sap5");

    let mut link = BricLink::new("sim");
    let mut got = Vec::new();
    for s in g["briclink"]["steps"].as_array().unwrap() {
        if s["op"] == "flush" {
            got.extend(outs_json(&link.flush()));
        } else {
            let kind = BricKind::parse(s["kind"].as_str().unwrap()).unwrap();
            got.extend(outs_json(
                &link.on_frame(kind, &bytes(s["hex"].as_str().unwrap())),
            ));
        }
    }
    check(Value::Array(got), &g["briclink"]["log"], "bric");
}

fn readings(v: &Value) -> Vec<Reading> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r.clone()).unwrap())
        .collect()
}

fn events_json(e: &[LegEvent]) -> Vec<Value> {
    e.iter().map(|e| serde_json::to_value(e).unwrap()).collect()
}

#[test]
fn legs() {
    let g = golden();
    for (name, c) in g["legs"].as_object().unwrap() {
        let flush_at: Vec<usize> = c["flushAt"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_u64().unwrap() as usize)
            .collect();
        let mut a = LegAssembler::new();
        let mut got = Vec::new();
        for (i, r) in readings(&c["readings"]).into_iter().enumerate() {
            got.extend(events_json(&a.push(r)));
            if flush_at.contains(&i) {
                got.push(json!({"flush": events_json(&a.flush())}));
            }
        }
        got.push(json!({"flush": events_json(&a.flush())}));
        assert_eq!(a.waiting() as u64, c["waiting"].as_u64().unwrap());
        check(Value::Array(got), &c["events"], name);
    }
    for c in g["close"].as_array().unwrap() {
        let (a, b): (Reading, Reading) = (
            serde_json::from_value(c["a"].clone()).unwrap(),
            serde_json::from_value(c["b"].clone()).unwrap(),
        );
        assert_eq!(is_close(&a, &b), c["close"].as_bool().unwrap());
    }
    for c in g["average"].as_array().unwrap() {
        let (r, s) = average(&readings(&c["shots"]));
        check(
            json!({"reading": r, "spread": s}),
            &json!({"reading": c["reading"], "spread": c["spread"]}),
            "average",
        );
    }
}

#[test]
fn identify_by_name_and_service() {
    let g = golden();
    for c in g["identify"].as_array().unwrap() {
        let services: Vec<String> = c["services"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().into())
            .collect();
        let got = identify(c["name"].as_str().unwrap(), &services)
            .map_or(Value::Null, |i| serde_json::to_value(i).unwrap());
        check(got, &c["result"], c["name"].as_str().unwrap());
    }
}

fn steps(v: &Value) -> Vec<Step> {
    serde_json::from_value(v.clone()).unwrap()
}

#[test]
fn traverse_and_shots_match() {
    let g = golden();
    for c in g["traverse"].as_array().unwrap() {
        let opts: TraverseOpts = serde_json::from_value(c["opts"].clone()).unwrap();
        check(
            serde_json::to_value(traverse(&opts)).unwrap(),
            &c["steps"],
            &format!("traverse {}", c["opts"]),
        );
    }
    for c in g["shotsFor"].as_array().unwrap() {
        let policy: FlagPolicy = serde_json::from_value(c["policy"].clone()).unwrap();
        let shots = shots_for(
            &steps(&c["steps"]),
            policy,
            c["seed"].as_u64().unwrap() as u32,
        );
        check(
            serde_json::to_value(shots).unwrap(),
            &c["shots"],
            "shotsFor",
        );
    }
}

#[test]
fn devices_and_quirks_match() {
    let g = golden();
    check(
        serde_json::to_value(devices()).unwrap(),
        &g["devices"],
        "devices",
    );
}

#[test]
fn every_simulated_device_matches() {
    let g = golden();
    let survey = steps(&g["shotsFor"][0]["steps"]);
    for run in g["sim"].as_array().unwrap() {
        let kind: DeviceKind = serde_json::from_value(run["kind"].clone()).unwrap();
        let quirk = run["quirk"].as_str().unwrap();
        let single = quirk == "single";
        let quirks = if quirk == "none" || single {
            vec![]
        } else {
            vec![quirk.to_string()]
        };
        let mut sim = Simulator::new(SimOptions {
            kind,
            steps: survey.clone(),
            policy: FlagPolicy::Third,
            quirks,
        });
        sim.connect();
        let mut got = Vec::new();
        let mut fired = Vec::new();
        if single {
            for _ in 0..12 {
                got.extend(outs_json(&sim.take_shot()));
            }
            fired.push(12);
        } else {
            for _ in 0..6 {
                let (n, outs) = sim.take_station(run["wantBack"].as_bool().unwrap(), true);
                fired.push(n);
                got.extend(outs_json(&outs));
            }
        }
        got.extend(outs_json(&sim.disconnect()));
        let at = format!("{} / {quirk} / back {}", run["kind"], run["wantBack"]);
        check(json!(fired), &run["fired"], &format!("{at}: fired"));
        check(Value::Array(got), &run["log"], &format!("{at}: log"));
        let received: Vec<String> = sim.received().iter().map(|b| hex(b)).collect();
        check(json!(received), &run["received"], &format!("{at}: acks"));
    }
}
