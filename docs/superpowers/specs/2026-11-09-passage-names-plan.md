# Passage names that follow the passage (plan, not built)

Next creative addition after Ink style, the brushes, the survey timeline and depth shading (2026-11-09).

## What the caver does
Draw tab > Annotation > **Passage name**. Type the name ("Main Trunk"), then drag along the passage
on the Map. The words bend along the drag, spaced out letter by letter, sized for the drawing scale,
and stay readable (never upside down: a stroke drawn right-to-left is read left-to-right).

## How it is stored (source vs derived)
- Authored: one feature, kind `passage-name`, its path (fit points, anchored to the survey like other
  map lines so it moves with an adjustment), its text, and a style (size, letter spacing, font).
- Derived: where each letter sits and how it turns, worked out every draw from the path and the text.
  New small Rust function `cave-draw` or a crate `cave-label`: `letters_along(path, text, size, spacing)`
  -> `[(x, y, angle, char)]`, with the flip rule and "fit to path" (spread or squeeze to fill the drag).
  Needs glyph widths: pass measured widths from the browser (canvas measureText) so Rust stays font-free.

## Where it shows
Map, Profile (for pit/shaft names along a band), printed sheets (`drawStrokes` path), DXF export as one
TEXT per letter (rotated) so other programs show it bent.

## Editing
Select to edit words, size, spacing; drag grips to reshape the path (the existing line editor);
Ink style leaves the letters alone.

## Open product questions for Nathan
1. Fit: spread the name to fill the whole drag, or keep normal spacing and centre it on the drag?
2. Look: plain, or capitals with wide spacing (the classic cartographic passage-name look)?
3. Halo: a paper-coloured outline round the letters so they read over walls and fills?

## Depth shading refinement (owed, Nathan 2026-11-09: "needs refined")
Ideas to pick from: smoother gradient (vertex colours rather than flat triangles), contour bands option,
ceiling-height mode, colour ramp option, show on printed sheets, key in chosen units with real depths.
Ask Nathan what felt off before changing it.
