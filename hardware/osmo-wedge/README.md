# OM7 phone wedge

Tips the iPhone (in its case) 45° about its long axis inside the DJI Osmo Mobile 7, so the gimbal's roll motor
steers the camera up and down instead of only spinning the picture. Mounted one way it reaches the ceiling;
turned upside down in the clamp it reaches the floor. Why: `docs/superpowers/specs/2026-11-20-gimbal-capture-design.md`
and the coverage model shown to Nathan on 2026-11-20.

- `osmo-wedge-45.stl`: ready to slice, for a case 86.4 × 170.2 × 15.2 mm (3.4 × 6.7 × 0.6 in).
- `make_wedge.py`: the generator (Python 3, standard library only). Change the case or the angle:

      python3 make_wedge.py --case-w 86.4 --case-l 170.2 --case-t 15.2 --angle 45 --out osmo-wedge-45.stl

## Parts

- **Tongue** (78 × 9 mm): the part the OM7 clamp grips, inside its 67–84 mm width and 6.9–10 mm thickness range.
  Grip it near the top, at 89 mm up (the phone's centre), for balance.
- **Plate:** a solid 74 mm wide plate tipped 45°; the phone's screen rests on it, the phone's bottom edge stands
  on the base, and its top (camera bump, LiDAR) stands free above the plate.
- **Band guides:** two pairs of 45° ridges on each long edge of the plate make channels for two rubber bands.
  Run the bands across the back of the case, below the camera bump.

## Printing (no supports, no bridges)

Print it exactly as the STL is oriented, standing on its base. The body is one cross-section extruded straight
up (vertical walls only), the base and lip are the first 4 mm on the bed, and the ridges are 45°: nothing overhangs
more than 45°. PETG (or PLA), 0.2 mm layers, 4 walls, 30 % infill, a brim helps it stand. About 79 × 94 × 124 mm.

The plate covers the middle of the screen: tap Start before strapping the phone in, or use the gimbal pass's
start delay once the app has one.

## Before trusting it on the gimbal

- **Balance:** the phone now sits about 55 mm in front of and off to one side of the clamp, so the gimbal's motors
  hold a lever they don't hold today. Power the OM7 with the wedge and phone fitted and watch for strain or drift.
  If it struggles, a smaller angle (`--angle 30`) moves the phone closer.
- **Roll window:** the app's sweep keeps roll inside the measured safe window (75–205° joint); past it the clamp
  flips violently (FINDINGS). The wedge doesn't change that rule.
- The gimbal software needs a "wedge" mount setting that adds roll steps to the sweep; not built yet.
