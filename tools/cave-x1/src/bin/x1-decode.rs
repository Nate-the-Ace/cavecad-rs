//! Decodes X1 packets given as hex (one per argument, spaces allowed inside quotes), as the debug log shows them.
fn main() {
    for a in std::env::args().skip(1) {
        let b: Vec<u8> = a.split_whitespace().filter_map(|h| u8::from_str_radix(h, 16).ok()).collect();
        match cave_instrument::cavway::decode_packet(&b) {
            Some(p) => println!(
                "{:.3} m az {:.1} inc {:.1} dip {:?} leg {} scan {} errors {:?}",
                p.distance, p.azimuth, p.inclination, p.sensors.first().map(|x| x.dip), p.leg, p.scan,
                p.errors.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>()
            ),
            None => println!("not a packet"),
        }
    }
}
