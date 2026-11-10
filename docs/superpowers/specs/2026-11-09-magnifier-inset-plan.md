# Magnifier inset (plan, not built)

Nathan picked this 2026-11-09 after Ink style, the brushes, the survey timeline and depth shading.

## What the caver does
Annotation tab > **Magnifier**. Drag a circle (or box) round a crowded spot on the Map. A blown-up
copy of that spot appears beside it, joined by a leader line. Drag the copy to place it; its grip sets
how much it magnifies (2x, 3x, 4x, or a typed scale such as 1:100). Select it later to move, resize,
re-aim or delete it. It shows on screen and on printed sheets.

## Stored vs derived (house rule)
- Authored: one feature, kind `inset`: the circle (centre + radius, plan metres, anchored to the survey
  like map lines so it follows an adjustment), where the copy sits (offset from the circle), the
  magnification, the shape (circle or box), an optional title ("A12 junction") and the plan level it
  looks at (the level rule of overlap-levels: map lines stay on their level).
- Derived: everything inside the copy. Nothing is duplicated into the cave; the copy is the live map
  redrawn through a second transform, so it is always current (survey, walls, linework, fills, brushes,
  symbols, depth shading, Ink style).

## How it draws (one renderer, two places)
- Factor the Map overlay's 2D drawing (`strokeLines`, survey walls/legs/stations, fills, depth shade)
  so it can draw into any canvas with any plan->pixel transform, clipped to a path. Today `cave-view.ts`
  draws the survey with three.js and the drawn lines on a 2D overlay; the inset needs the survey in 2D
  too (walls, legs, stations, labels). `sheet-draw.ts` already draws the whole plan in 2D for sheets:
  reuse that path for the inset on both the Map and sheets.
- Map: after the main overlay, for each inset: save, clip to the copy's circle/box, set transform
  (centre of circle -> centre of copy, scale x magnification), draw the plan, restore; then the frame,
  the source circle, the leader (shortest line between the two outlines, never through the copy), the
  title and a small scale bar for the inset's own scale.
- Sheets: same call inside `drawSheet`, in paper units, so the printed inset is true to scale.
- Text and symbols inside the copy: sized for the inset's scale (bigger), but line weights stay the
  pen weights (a blown-up map should not have fat walls). Ink style wobble sized in the copy's own paper mm.
- DXF: export the copy as real geometry (lines scaled and moved, clipped) on its own layer, plus frame,
  circle and leader. Rust function: `cave-draw::inset_geometry(view, inset) -> lines` (clip polylines to
  circle/box), tested.

## Rust pieces
- `cave-doc`: `inset` feature fields; `cave-draw`: add/edit actions (undoable, scope `map`).
- `cave-geom`: clip polyline to circle and to box (with tests: crossing, tangent, fully inside/outside).
- `cave-wasm`: add_inset / edit_inset / inset list in map_lines (or own call).

## Checks
Pitfall (maze junction near B15-B18), Plumbline Pit (stacked pitches: inset must look at one level),
print a sheet with an inset at 1:500 and 3x, DXF opens in another program with the inset in place.

## Open product questions for Nathan
1. Shape: circle only, or circle and box?
2. Magnification: simple 2x/3x/4x buttons, or a proper scale (1:100) printed under the inset?
3. Leader: one straight line, or two lines from the circle's edges to the copy's edges (the
   "cone" look)?
4. Should an inset also be allowed in the Profile (a blown-up pitch series)? (Plan: Map + sheets first.)

## Size
Medium-large: the 2D survey renderer factoring is most of it. Rough order: geometry + doc (1 session),
Map drawing + tool (1), sheets + DXF + polish (1).
