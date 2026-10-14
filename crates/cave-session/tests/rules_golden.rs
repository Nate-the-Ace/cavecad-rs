//! The session rules must decide exactly as the web app's TypeScript did: the golden file is written by
//! `tools/golden/gen_session_golden.mjs` running the OLD `survey.ts` (survey-sketch before the port).

use cave_id::Id;
use cave_instrument::legs::LegEvent;
use cave_instrument::{Reading, Side};
use cave_session::rules::*;
use serde_json::{Value, json};
use std::collections::HashSet;

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/session.json")).unwrap()
}

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

fn reading(v: &Value) -> Reading {
    serde_json::from_value(v.clone()).unwrap()
}

fn state(v: &Value) -> SessionState {
    serde_json::from_value(v.clone()).unwrap()
}

fn taken(v: &Value) -> HashSet<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn plan_json(p: &Plan) -> Value {
    match p {
        Plan::Leg { from, to, reading } => {
            json!({"kind": "leg", "from": from, "to": to, "reading": reading})
        }
        Plan::Splay { from, reading } => json!({"kind": "splay", "from": from, "reading": reading}),
        Plan::Lrud {
            station,
            side,
            reading,
        } => json!({"kind": "lrud", "station": station, "side": side, "reading": reading}),
        Plan::Back { shot, reading } => json!({"kind": "back", "shot": shot, "reading": reading}),
    }
}

fn decision_json(d: &Decision) -> Value {
    json!({"plan": d.plan.as_ref().map(plan_json), "reject": d.reject, "warnings": d.warnings})
}

#[test]
fn readings_are_checked_alike() {
    for c in golden()["check"].as_array().unwrap() {
        let r = reading(&c["reading"]);
        let got = match check_reading(&r, c["splay"].as_bool().unwrap()) {
            Ok(w) => json!({"ok": true, "warnings": w}),
            Err(why) => json!({"ok": false, "why": why}),
        };
        check(got, &c["result"], &c["reading"].to_string());
    }
}

#[test]
fn backsights_and_walls_are_judged_alike() {
    let g = golden();
    for c in g["back"].as_array().unwrap() {
        let (a, b) = (reading(&c["fore"]), reading(&c["back"]));
        check(
            json!(backsight_warnings(&a, &b)),
            &c["warnings"],
            "backsight warnings",
        );
        assert_eq!(
            looks_like_back(&a, &b),
            c["looks"].as_bool().unwrap(),
            "looks like back {} {}",
            c["fore"],
            c["back"]
        );
    }
    for c in g["side"].as_array().unwrap() {
        let side = side_of(&reading(&c["reading"]), c["heading"].as_f64());
        check(
            json!(side),
            &c["side"],
            &format!("side {} {}", c["reading"], c["heading"]),
        );
    }
}

#[test]
fn every_reading_is_decided_alike() {
    let g = golden();
    let cases = g["decide"].as_array().unwrap();
    assert!(cases.len() > 1000);
    for c in cases {
        let got = decision_json(&decide(&state(&c["state"]), &reading(&c["reading"])));
        check(
            got,
            &c["result"],
            &format!("{} / {}", c["state"], c["reading"]),
        );
    }
}

#[test]
fn the_surveyor_moves_alike() {
    let g = golden();
    for c in g["moves"].as_array().unwrap() {
        let s = state(&c["state"]);
        let t = taken(&c["taken"]);
        let at = c["state"].to_string();
        check(
            serde_json::to_value(advance(&s, &t)).unwrap(),
            &c["advance"],
            &format!("advance {at}"),
        );
        check(
            serde_json::to_value(skip_back(&s, &t)).unwrap(),
            &c["skip"],
            &format!("skip {at}"),
        );
        let modes = [Mode::Auto, Mode::Leg, Mode::Splay, Mode::Lrud]
            .map(|m| serde_json::to_value(with_mode(&s, m, &t)).unwrap());
        check(json!(modes), &c["modes"], &format!("modes {at}"));
        assert_eq!(next_step(&s), c["step"].as_str().unwrap(), "step {at}");
        assert_eq!(standing(&s), c["standing"].as_str().unwrap());
        for a in c["applied"].as_array().unwrap() {
            let p = &a["plan"];
            let r = reading(&p["reading"]);
            let plan = match p["kind"].as_str().unwrap() {
                "leg" => Plan::Leg {
                    from: p["from"].as_str().unwrap().into(),
                    to: p["to"].as_str().unwrap().into(),
                    reading: r,
                },
                "splay" => Plan::Splay {
                    from: p["from"].as_str().unwrap().into(),
                    reading: r,
                },
                "lrud" => Plan::Lrud {
                    station: p["station"].as_str().unwrap().into(),
                    side: serde_json::from_value::<Side>(p["side"].clone()).unwrap(),
                    reading: r,
                },
                _ => Plan::Back {
                    shot: p["shot"].as_str().unwrap().parse::<Id>().unwrap(),
                    reading: r,
                },
            };
            let shot = Some("0000000000000000000000000".to_string() + "7")
                .map(|t| t.parse::<Id>().unwrap());
            check(
                serde_json::to_value(applied(&s, &plan, shot, &t)).unwrap(),
                &a["state"],
                &format!("applied {} {at}", p["kind"]),
            );
        }
    }
    for c in g["start"].as_array().unwrap() {
        let keep = (!c["keep"].is_null()).then(|| {
            (
                serde_json::from_value(c["keep"]["mode"].clone()).unwrap(),
                c["keep"]["backsights"].as_bool().unwrap(),
            )
        });
        check(
            serde_json::to_value(start(
                c["from"].as_str().unwrap(),
                &taken(&c["taken"]),
                keep,
            ))
            .unwrap(),
            &c["state"],
            "start",
        );
    }
}

#[test]
fn every_instrument_event_is_decided_alike() {
    let g = golden();
    let cases = g["events"].as_array().unwrap();
    assert!(cases.len() > 1000);
    for c in cases {
        let ev: LegEvent = serde_json::from_value(c["ev"].clone()).unwrap();
        let (base, decision) = decide_event(&state(&c["state"]), &ev, &taken(&c["taken"]));
        let got = json!({"base": base, "decision": decision_json(&decision)});
        check(got, &c["result"], &format!("{} / {}", c["state"], c["ev"]));
    }
}
