//! Reads a real TopoDroid archive (unzipped) and prints what came out. Local data only:
//! `TOPODROID_DIR=<folder holding manifest, survey.sql, *.tdr> cargo test -p cave-formats --test topodroid_real -- --ignored --nocapture`

use std::collections::BTreeMap;

#[test]
#[ignore]
fn reads_a_real_archive() {
    let Ok(dir) = std::env::var("TOPODROID_DIR") else {
        return;
    };
    let files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().to_string(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    let a = cave_formats::topodroid::read_archive(&files).unwrap();
    let s = &a.survey;
    let legs = s.shots.iter().filter(|x| !x.splay).count();
    println!(
        "cave {} date {} decl {} legs {} splays {} fixed {:?} frame {:?}",
        s.cave_name,
        s.date,
        s.declination,
        legs,
        s.shots.len() - legs,
        s.fixed.iter().collect::<Vec<_>>(),
        s.fixed_frame
    );
    for f in s.findings() {
        println!("finding: {}", f.message);
    }
    for p in &a.plots {
        let Some(sk) = &p.sketch else {
            println!("plot {} ({}): no file", p.name, p.projection);
            continue;
        };
        let sc = &sk.scraps[0];
        let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
        for l in &sc.lines {
            *kinds.entry(format!("L {}", l.kind)).or_default() += 1;
        }
        for x in &sc.points {
            *kinds.entry(format!("P {}", x.kind)).or_default() += 1;
        }
        for x in &sc.areas {
            *kinds.entry(format!("A {}", x.kind)).or_default() += 1;
        }
        if std::env::var("TIES").is_ok() {
            let xs: Vec<f64> = sc
                .lines
                .iter()
                .flat_map(|l| l.segs.iter().map(|s| s.to[0]))
                .collect();
            let ys: Vec<f64> = sc
                .lines
                .iter()
                .flat_map(|l| l.segs.iter().map(|s| s.to[1]))
                .collect();
            let mn = |v: &Vec<f64>| v.iter().cloned().fold(f64::INFINITY, f64::min);
            let mx = |v: &Vec<f64>| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            println!(
                "  ties {:?} x {:.0}..{:.0} y {:.0}..{:.0}",
                sc.stations
                    .iter()
                    .take(6)
                    .map(|t| (t.name.clone(), t.x.round(), t.y.round()))
                    .collect::<Vec<_>>(),
                mn(&xs),
                mx(&xs),
                mn(&ys),
                mx(&ys)
            );
        }
        println!(
            "plot {} ({}): ties {} {:?} findings {:?}",
            p.name,
            p.projection,
            sc.stations.len(),
            kinds,
            sk.findings.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }
}

/// Every leg against TopoDroid's own Therion export of the same survey (rounded as it writes them).
/// `TOPODROID_DIR` as above, `TOPODROID_TH=<that export's .th>`.
#[test]
#[ignore]
fn legs_agree_with_topodroids_own_therion_export() {
    let (Ok(dir), Ok(th)) = (
        std::env::var("TOPODROID_DIR"),
        std::env::var("TOPODROID_TH"),
    ) else {
        return;
    };
    let files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().to_string(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    let ours = cave_formats::topodroid::read_archive(&files)
        .unwrap()
        .survey;
    let text = std::fs::read_to_string(&th).unwrap();
    let theirs = cave_formats::therion::parse(&text);
    let key = |s: &cave_formats::Shot| {
        (
            s.from.rsplit('.').next().unwrap().to_string(),
            s.to.rsplit('.').next().unwrap().to_string(),
        )
    };
    let (mut same, mut differ, mut missing) = (0, 0, 0);
    for t in theirs.shots.iter().filter(|s| !s.splay && !s.to.is_empty()) {
        let Some(o) = ours.shots.iter().find(|o| !o.splay && key(o) == key(t)) else {
            missing += 1;
            continue;
        };
        let az = ((o.azimuth - o.declination.unwrap_or(0.0) + ours.declination - t.azimuth
            + 540.0)
            % 360.0
            - 180.0)
            .abs();
        if (o.distance - t.distance).abs() <= 0.0051
            && az <= 0.051
            && (o.inclination - t.inclination).abs() <= 0.051
        {
            same += 1;
        } else {
            differ += 1;
            println!(
                "differ {:?}: ours {:.3} {:.2} {:.2} theirs {:.2} {:.1} {:.1}",
                key(t),
                o.distance,
                o.azimuth,
                o.inclination,
                t.distance,
                t.azimuth,
                t.inclination
            );
        }
    }
    let splays = |s: &cave_formats::Survey| s.shots.iter().filter(|x| x.splay).count();
    println!(
        "legs same {same} differ {differ} not in ours {missing}; splays ours {} theirs {}",
        splays(&ours),
        splays(&theirs)
    );
}
