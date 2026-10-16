//! Edge cases found probing the readers with the quirks real files have, kept as regressions.

const HEAD: &str = "CAVE\r\nSURVEY NAME: A\r\nSURVEY DATE: 7 10 2024  COMMENT:x\r\nSURVEY TEAM:\r\nAnn\r\nDECLINATION: 0.00  FORMAT: DDDDLRUDLADNF\r\n\r\nFROM TO LENGTH BEARING INC LEFT UP DOWN RIGHT\r\n\r\n";

#[test]
fn a_dos_end_of_file_byte_is_not_a_trip() {
    let s = cave_formats::compass::parse(&format!(
        "{HEAD}A1 A2 10.00 90.00 0.00 1 1 1 1\r\n\u{0C}\r\n\u{1A}"
    ));
    assert_eq!(s.trips.len(), 1);
}

#[test]
fn surveys_joined_without_a_form_feed_stay_apart() {
    let b = HEAD
        .replace("NAME: A", "NAME: B")
        .replace("7 10 2024", "8 11 2024");
    let s = cave_formats::compass::parse(&format!(
        "{HEAD}A1 A2 10.00 90.00 0.00 1 1 1 1\r\n{b}B1 B2 10.00 90.00 0.00 1 1 1 1\r\n"
    ));
    assert_eq!(s.trips.len(), 2);
    assert_eq!(s.shots[1].trip, 1);
    assert_eq!(s.trips[1].date, "2024-08-11");
}

#[test]
fn a_date_that_cannot_be_is_no_date() {
    let s = cave_formats::compass::parse(&format!(
        "{}A1 A2 10.00 90.00 0.00 1 1 1 1\r\n\u{0C}",
        HEAD.replace("7 10 2024", "13 45 2024")
    ));
    assert_eq!(s.trips[0].date, "");
}

/// Each leg's (distance, azimuth, inclination), rounded.
fn legs(s: &cave_formats::Survey) -> Vec<(f64, f64, f64)> {
    let r = |v: f64| (v * 10.0).round() / 10.0;
    s.shots
        .iter()
        .map(|x| (r(x.distance), r(x.azimuth), r(x.inclination)))
        .collect()
}

#[test]
fn therion_thread_counter_depth_gauge_and_offsets_are_read() {
    // Therion's own results for these (checked with its CLI): a larger depth reading is higher,
    // offsets point at true north whatever the declination
    let s = cave_formats::therion::parse(
        "survey a\ncentreline\ndeclination 10 degrees\ndata topofil from to fromcount tocount compass clino\n1 2 100 110 90 0\ndata diving from to length compass fromdepth todepth\n2 3 10 90 5 10\ndata diving from to length compass depthchange\n3 4 10 90 -5\ndata cartesian from to easting northing altitude\n4 5 3 4 0\nendcentreline\nendsurvey\n",
    );
    assert_eq!(
        legs(&s),
        [
            (10.0, 100.0, 0.0),
            (10.0, 100.0, 30.0),
            (10.0, 100.0, -30.0),
            (5.0, 36.9, 0.0)
        ]
    );
    assert!(s.findings().is_empty(), "{:?}", s.findings());
}

#[test]
fn survex_counter_and_gauge_units_and_calibration_apply() {
    // Survex's own test files bugdz.svx and infereqtopofil.svx, in short
    let s = cave_formats::survex::parse(
        "*data diving from to tape compass depthchange\n*units depth 0.01 metres\n*calibrate depth 100 -1\n1 2 5.00 090 400\n*data topofil from to fromcount tocount compass clino\n*calibrate count 0 0.1\n2 3 0 10 0 0\n",
    );
    assert_eq!(legs(&s), [(5.0, 90.0, -36.9), (1.0, 0.0, 0.0)]);
}

#[test]
fn readings_this_reader_cannot_follow_are_said_not_dropped() {
    let s = cave_formats::survex::parse(
        "*data cylpolar from to tape compass depthchange\n1 2 10 90 5\n*data diving station depth newline tape compass\n1 0\n10 90\n2 5\n",
    );
    assert!(s.shots.is_empty());
    assert_eq!(s.findings().len(), 2, "{:?}", s.findings());
}

#[test]
fn a_walls_macro_expands_in_a_data_line() {
    let s = cave_formats::walls::parse("#units meters $az=90\nA1 A2 10 $(az) 0\n");
    assert_eq!(s.shots[0].azimuth, 90.0);
}

#[test]
fn a_negative_tape_is_said_not_silently_left_off() {
    let (_, s) = cave_formats::read_any("x.svx", "1 2 -10 90 0\n2 3 5 0 0\n", &|_| None).unwrap();
    assert!(
        s.findings()
            .iter()
            .any(|f| f.code == "tape-unusable" && f.message.contains("1 to 2")),
        "{:?}",
        s.findings()
    );
}

/// Each leg as "from>to distance azimuth".
fn csv_legs(t: &str) -> Vec<String> {
    cave_formats::csv::parse(t)
        .shots
        .iter()
        .map(|x| format!("{}>{} {} {}", x.from, x.to, x.distance, x.azimuth))
        .collect()
}

#[test]
fn spreadsheet_csv_reads_whatever_its_layout() {
    let want = ["A1>A2 10.5 90"];
    // a European spreadsheet: semicolons and a decimal comma
    assert_eq!(
        csv_legs("From;To;Distance;Azimuth;Inclination\nA1;A2;10,5;90;0\n"),
        want
    );
    assert_eq!(
        csv_legs("From\tTo\tDistance\tAzimuth\tInclination\nA1\tA2\t10.5\t90\t0\n"),
        want
    );
    // columns in another order, named another way, with a unit
    assert_eq!(
        csv_legs("To Station,From Station,Bearing,Length (m),Clino\nA2,A1,90,10.5,0\n"),
        want
    );
    // quoted cells keep their commas and lose their quotes
    assert_eq!(
        csv_legs(
            "From,To,Distance,Azimuth,Inclination,Comment\n\"A1\",A2,10.5,90,0,\"big, wet\"\n"
        ),
        want
    );
    let s = cave_formats::csv::parse("From,To,Distance (ft),Azimuth,Inclination\nA1,A2,10,90,0\n");
    assert_eq!(s.distance_unit.as_deref(), Some("ft"));
}

#[test]
fn a_csv_nothing_can_be_read_from_says_so() {
    let s = cave_formats::csv::parse("Station,Something,Other,More\nA1,x,y,z\n");
    assert!(s.shots.is_empty());
    assert!(s.findings().iter().any(|f| f.code == "csv-no-legs"));
}

#[test]
fn a_tab_separated_file_is_read_as_csv() {
    let t = "From\tTo\tDistance\tAzimuth\tInclination\nA1\tA2\t10\t90\t0\n";
    assert_eq!(cave_formats::read_any("x.tsv", t, &|_| None).unwrap().1.shots.len(), 1);
}
