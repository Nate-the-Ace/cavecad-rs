//! The port against the old app, call for call. `golden/cases.json` is what the old JavaScript
//! readers and writers answered (tools/golden/gen_formats_golden.mjs): every call its unit tests
//! made, and every test survey file read and written back out in all five formats.

use cave_formats::{Format, Survey, detect, therion};
use serde_json::Value;
use std::collections::HashMap;

fn cases() -> Value {
    let path = format!("{}/tests/golden/cases.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// What a missing key in the old output is the same as.
fn is_default(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Number(_) => false,
    }
}

/// `expected` and `actual` agree (numbers to 1e-9); a key the old code never set must be at its
/// default here. Returns where they first differ.
fn diff(expected: &Value, actual: &Value, at: &str) -> Option<String> {
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            ((a - b).abs() > 1e-9 * a.abs().max(1.0))
                .then(|| format!("{at}: expected {a}, got {b}"))
        }
        (Value::Object(e), Value::Object(a)) => {
            for (k, ev) in e {
                let av = a.get(k).unwrap_or(&Value::Null);
                if let Some(d) = diff(ev, av, &format!("{at}.{k}")) {
                    return Some(d);
                }
            }
            // `aliases` (other names of joined stations) is new here: the old app had no such list
            a.iter()
                .find(|(k, v)| !e.contains_key(*k) && !is_default(v) && *k != "aliases")
                .map(|(k, v)| format!("{at}.{k}: not in the old output, got {v}"))
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Some(format!("{at}: expected {} items, got {}", e.len(), a.len()));
            }
            e.iter()
                .zip(a)
                .enumerate()
                .find_map(|(i, (x, y))| diff(x, y, &format!("{at}[{i}]")))
        }
        (e, a) if e == a => None,
        (Value::Null, a) if is_default(a) => None,
        (e, a) => Some(format!("{at}: expected {e}, got {a}")),
    }
}

fn first_text_diff(expected: &str, actual: &str) -> String {
    let (e, a): (Vec<&str>, Vec<&str>) =
        (expected.split('\n').collect(), actual.split('\n').collect());
    for i in 0..e.len().max(a.len()) {
        if e.get(i) != a.get(i) {
            return format!(
                "line {}: expected {:?}, got {:?}",
                i + 1,
                e.get(i),
                a.get(i)
            );
        }
    }
    "identical".into()
}

struct Report {
    failures: Vec<String>,
    passed: usize,
}

impl Report {
    fn check(&mut self, what: String, problem: Option<String>) {
        match problem {
            Some(p) => self.failures.push(format!("{what}: {p}")),
            None => self.passed += 1,
        }
    }

    fn finish(self) {
        if !self.failures.is_empty() {
            for f in self.failures.iter().take(25) {
                eprintln!("FAIL {f}");
            }
            panic!(
                "{} of {} golden checks failed",
                self.failures.len(),
                self.failures.len() + self.passed
            );
        }
        eprintln!("{} golden checks passed", self.passed);
    }
}

fn parse_check(format: Format, input: &str, expected: &Value) -> Option<String> {
    let got = serde_json::to_value(format.parse(input)).unwrap();
    diff(expected, &got, "survey")
}

fn write_check(format: Format, survey: &Survey, expected: &str) -> Option<String> {
    let got = format.write(&mut survey.clone());
    (got != expected).then(|| first_text_diff(expected, &got))
}

#[test]
fn the_old_unit_tests_calls() {
    let all = cases();
    let mut r = Report {
        failures: Vec::new(),
        passed: 0,
    };
    for (i, c) in all["unit"].as_array().unwrap().iter().enumerate() {
        let op = c["op"].as_str().unwrap();
        let input = &c["input"];
        match op {
            "detect" => {
                let got = detect(c["name"].as_str().unwrap_or(""), input.as_str().unwrap())
                    .map(|f| f.id());
                let want = c["output"].as_str();
                r.check(
                    format!("unit #{i} detect {:?}", c["name"]),
                    (got != want).then(|| format!("expected {want:?}, got {got:?}")),
                );
            }
            "parse" => {
                let f = Format::from_id(c["format"].as_str().unwrap()).unwrap();
                r.check(
                    format!("unit #{i} {} parse", f.id()),
                    parse_check(f, input.as_str().unwrap(), &c["output"]),
                );
            }
            "write" => {
                let f = Format::from_id(c["format"].as_str().unwrap()).unwrap();
                let survey: Survey = serde_json::from_value(input.clone()).unwrap();
                r.check(
                    format!("unit #{i} {} write", f.id()),
                    write_check(f, &survey, c["output"].as_str().unwrap()),
                );
            }
            _ => unreachable!(),
        }
    }
    r.finish();
}

#[test]
fn the_test_survey_files_read_and_write_back() {
    let all = cases();
    let mut r = Report {
        failures: Vec::new(),
        passed: 0,
    };
    for c in all["files"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let input = c["input"].as_str().unwrap();
        let want = Format::from_id(c["detect"].as_str().unwrap()).unwrap();
        let got = detect(name, input);
        r.check(
            format!("{name} detect"),
            (got != Some(want)).then(|| format!("expected {want:?}, got {got:?}")),
        );
        r.check(
            format!("{name} parse"),
            parse_check(want, input, &c["survey"]),
        );
        let survey = want.parse(input);
        for (id, text) in c["writes"].as_object().unwrap() {
            let f = Format::from_id(id).unwrap();
            r.check(
                format!("{name} written as {id}"),
                write_check(f, &survey, text.as_str().unwrap()),
            );
        }
    }
    r.finish();
}

#[test]
fn a_therion_project_split_across_files_is_read_whole() {
    let all = cases();
    let c = &all["therionProject"];
    let files: HashMap<String, String> = c["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    // the same path tidying the generator's reader does
    let read = |p: &str| {
        let mut parts: Vec<&str> = Vec::new();
        for x in p.split('/') {
            match x {
                ".." => {
                    parts.pop();
                }
                "." | "" => {}
                x => parts.push(x),
            }
        }
        files.get(&parts.join("/")).cloned()
    };
    let path = c["path"].as_str().unwrap();
    let survey = therion::parse_with_inputs(&files[path], path, &read);
    let got = serde_json::to_value(survey).unwrap();
    if let Some(d) = diff(&c["survey"], &got, "survey") {
        panic!("{d}");
    }
}

/// Each sketch in a .th2 answer key (tools/golden/gen_th2_golden.mjs) read the way the old app
/// read it. Returns the files that disagree, with where.
fn th2_check(key: &str) -> (usize, Vec<String>) {
    let cases: Value = serde_json::from_str(&std::fs::read_to_string(key).unwrap()).unwrap();
    let cases = cases.as_array().unwrap();
    let mut bad = Vec::new();
    for case in cases {
        let actual =
            serde_json::to_value(cave_formats::th2::parse(case["input"].as_str().unwrap()))
                .unwrap();
        if let Some(d) = diff(&case["sketch"], &actual, "") {
            bad.push(format!("{}: {d}", case["file"].as_str().unwrap()));
        }
    }
    (cases.len(), bad)
}

#[test]
fn the_test_sketch_reads_as_the_old_app_read_it() {
    let (n, bad) = th2_check(&format!(
        "{}/tests/golden/th2.json",
        env!("CARGO_MANIFEST_DIR")
    ));
    assert!(n > 0 && bad.is_empty(), "{bad:#?}");
}

/// Real sketches, answer key made locally (never committed): `cargo test -- --ignored real_sketches`.
#[test]
#[ignore]
fn real_sketches_read_as_the_old_app_read_them() {
    let key = format!(
        "{}/Documents/cave-test-data/golden-th2.json",
        std::env::var("HOME").unwrap()
    );
    let (n, bad) = th2_check(&key);
    eprintln!("{} of {n} sketches agree", n - bad.len());
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(20)]);
}
