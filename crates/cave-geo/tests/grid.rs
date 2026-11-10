//! Integration tests.
use cave_geo::grid::Grid;
use cave_geo::request::Bounds;

/// A minimal uncompressed stripped float32 TIFF.
fn tiff(w: u16, h: u16, vals: &[f32], le: bool) -> Vec<u8> {
    let u16b = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let u32b = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let mut out = Vec::new();
    out.extend_from_slice(if le { b"II" } else { b"MM" });
    out.extend_from_slice(&u16b(42));
    let data_at = 8u32;
    let data: Vec<u8> = vals
        .iter()
        .flat_map(|v| if le { v.to_le_bytes() } else { v.to_be_bytes() })
        .collect();
    let ifd_at = data_at + data.len() as u32;
    out.extend_from_slice(&u32b(ifd_at));
    out.extend_from_slice(&data);
    let tags: [(u16, u16, u32); 8] = [
        (256, 3, w as u32),
        (257, 3, h as u32),
        (258, 3, 32),
        (259, 3, 1),
        (273, 4, data_at),
        (277, 3, 1),
        (278, 3, h as u32),
        (339, 3, 3),
    ];
    out.extend_from_slice(&u16b(tags.len() as u16));
    for (t, ty, v) in tags {
        out.extend_from_slice(&u16b(t));
        out.extend_from_slice(&u16b(ty));
        out.extend_from_slice(&u32b(1));
        if ty == 3 {
            out.extend_from_slice(&u16b(v as u16));
            out.extend_from_slice(&[0, 0]);
        } else {
            out.extend_from_slice(&u32b(v));
        }
    }
    out.extend_from_slice(&u32b(0));
    out
}

#[test]
fn reads_synthetic_both_orders() {
    let v: Vec<f32> = (0..9).map(|i| 100.0 + i as f32).collect();
    for le in [true, false] {
        let g = Grid::parse_tiff(&tiff(3, 3, &v, le)).unwrap();
        assert_eq!((g.w, g.h), (3, 3));
        assert_eq!(g.values, v);
    }
}

#[test]
fn refuses_what_it_cannot_read() {
    assert!(Grid::parse_tiff(b"not a tiff at all").is_err());
    let mut t = tiff(3, 3, &[0.0; 9], true);
    // compression tag (4th entry) value -> 5 (LZW)
    let ifd = u32::from_le_bytes([t[4], t[5], t[6], t[7]]) as usize;
    t[ifd + 2 + 3 * 12 + 8] = 5;
    let e = Grid::parse_tiff(&t).unwrap_err();
    assert!(e.contains("compressed"), "{e}");
}

#[test]
fn real_3dep_reply() {
    let g = Grid::parse_tiff(include_bytes!("data/bowling-green-200.tif")).unwrap();
    assert_eq!((g.w, g.h), (200, 200));
    let (lo, hi) = g.range().unwrap();
    assert!(lo > 120.0 && hi < 250.0 && lo < hi, "{lo}..{hi}");
    assert!(g.values.iter().all(|v| !Grid::no_data(*v)));
}

#[test]
fn sampling() {
    let g = Grid {
        w: 2,
        h: 2,
        values: vec![0.0, 10.0, 20.0, 30.0],
    };
    assert_eq!(g.sample_at(0.5, 0.5), Some(15.0));
    assert_eq!(g.sample_at(1.0, 0.0), Some(10.0));
    assert_eq!(g.sample_at(-0.1, 0.0), None);
    let holed = Grid {
        w: 2,
        h: 2,
        values: vec![0.0, -1.0e30, 20.0, 30.0],
    };
    assert_eq!(holed.sample_at(0.5, 0.5), None);
}

#[test]
fn cells_and_utm() {
    let b = Bounds {
        xmin: 1000.0,
        ymin: 2000.0,
        xmax: 1200.0,
        ymax: 2100.0,
    };
    let g = Grid {
        w: 200,
        h: 100,
        values: vec![0.0; 20_000],
    };
    assert_eq!(g.cell_utm(&b, 0.0, 0.0), (1000.5, 2099.5));
    let (c, r) = g.utm_cell(&b, 1100.25, 2050.75);
    let (e, n) = g.cell_utm(&b, c, r);
    assert!((e - 1100.25).abs() < 1e-9 && (n - 2050.75).abs() < 1e-9);
}

/// A little-endian TIFF with exactly these tags `(tag, type, value)` and `data_len` bytes of data at 8.
fn raw(tags: &[(u16, u16, u32)], data_len: usize, w: u32, h: u32) -> Vec<u8> {
    let mut out = b"II".to_vec();
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&(8 + data_len as u32).to_le_bytes());
    out.extend(std::iter::repeat_n(0u8, data_len));
    let base: Vec<(u16, u16, u32)> = vec![
        (256, 4, w),
        (257, 4, h),
        (258, 3, 32),
        (259, 3, 1),
        (277, 3, 1),
        (339, 3, 3),
    ];
    let all: Vec<_> = base.iter().chain(tags).collect();
    out.extend_from_slice(&(all.len() as u16).to_le_bytes());
    for &&(t, ty, v) in &all {
        out.extend_from_slice(&t.to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        if ty == 3 {
            out.extend_from_slice(&(v as u16).to_le_bytes());
            out.extend_from_slice(&[0, 0]);
        } else {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

#[test]
fn zero_tile_width_is_an_error() {
    let t = raw(&[(322, 3, 0), (323, 3, 2), (324, 4, 8)], 64, 4, 4);
    assert!(Grid::parse_tiff(&t).is_err());
    let t = raw(&[(322, 3, 2), (323, 3, 0), (324, 4, 8)], 64, 4, 4);
    assert!(Grid::parse_tiff(&t).is_err());
}

#[test]
fn zero_rows_per_strip_is_an_error() {
    let t = raw(&[(273, 4, 8), (278, 3, 0)], 64, 4, 4);
    assert!(Grid::parse_tiff(&t).is_err());
}

#[test]
fn huge_claimed_size_with_tiny_file_is_an_error() {
    let t = raw(&[(273, 4, 8), (278, 3, 100_000)], 16, 100_000, 100_000);
    assert!(Grid::parse_tiff(&t).is_err());
}

#[test]
fn truncated_file_is_an_error() {
    let v: Vec<f32> = (0..9).map(|i| i as f32).collect();
    let t = tiff(3, 3, &v, true);
    // the last 6 bytes are value padding and the unused next-IFD link, so a file cut there is still whole
    for n in 0..t.len() - 6 {
        assert!(Grid::parse_tiff(&t[..n]).is_err(), "len {n}");
    }
}

#[test]
fn wild_offsets_are_an_error() {
    let mut t = tiff(3, 3, &[0.0; 9], true);
    t[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Grid::parse_tiff(&t).is_err());
    let t = raw(&[(273, 4, u32::MAX), (278, 3, 4)], 64, 4, 4);
    assert!(Grid::parse_tiff(&t).is_err());
}

#[test]
fn tile_width_without_tile_length_is_an_error() {
    let t = raw(&[(322, 3, 2), (273, 4, 8), (278, 3, 4)], 64, 4, 4);
    let e = Grid::parse_tiff(&t).unwrap_err();
    assert!(e.contains("tile layout is incomplete"), "{e}");
}

#[test]
fn short_values_do_not_panic() {
    let g = Grid {
        w: 3,
        h: 3,
        values: vec![1.0],
    };
    assert_eq!(g.at(2, 2), None);
    let z = Grid {
        w: 0,
        h: 0,
        values: vec![],
    };
    assert_eq!(z.sample_at(0.0, 0.0), None);
}
