# Gridfinity dimensions used by this plugin

Source: Gridfinity by Zack Freedman; numbers as implemented in
[gridfinity-rebuilt-openscad](https://github.com/kennetek/gridfinity-rebuilt-openscad)
(`src/core/standard.scad`, `src/core/gridfinity-baseplate.scad`, MIT, read 2026-11-13).

## Grid
- Pitch: 42 × 42 mm per cell. Height unit: 7 mm.
- A bin's footprint per cell is 41.5 × 41.5 mm (0.5 mm gap between neighbours); a bin of n cells is 42·n − 0.5 mm.

## Bin foot (one per cell), from the bottom up
- Bottom: 41.5 − 2·2.95 = 35.6 mm square, corner radius 3.75 − 2.95 = 0.8 mm
- 0.8 mm up/out at 45°, then 1.8 mm straight up, then 2.15 mm up/out at 45°
- Top of foot: 41.5 mm square, corner radius 3.75 mm, at z = 4.75 mm
- Base height (feet + bridge tying them together): 7 mm; floor of the bin cavity from there.

## Bin height
- Height in units u: total = 7·u mm to the top of the walls (lip not included).
- Walls 0.95 mm (min). Dividers 1.2 mm. Inner fillet ~2.8 mm.

## Stacking lip (on top of the walls), inner tip at the inner wall line, going outward
- 0.7 mm up/out at 45°, 1.8 mm straight up, 1.9 mm up/out at 45° (total 2.6 wide × 4.4 high)
- A 1.2 mm support below the inner tip plus a 45° slope back to the wall.
- Tip fillet 0.6 mm.

## Holes in the bin feet (4 per cell)
- Centres 8 mm in from the 42 mm cell edge, i.e. ±13 mm from the cell centre (as gridfinity-rebuilt's `d_hole_from_side = 8`).
- Magnet: Ø6.5 mm × 2.4 mm deep (6 × 2 mm magnets). Screw: Ø3 mm (M3), ~6 mm deep.

## Baseplate (one pocket per cell)
- Cell 42 × 42 mm, outer corner radius 4 mm. Minimum height 5 mm.
- Pocket profile from the bottom of the pocket up: 0.7 at 45°, 1.8 straight, 2.15 at 45° (4.65 high),
  pocket top = 42 mm square r 4 mm, pocket bottom = 42 − 2·2.85 = 36.3 mm square r 1.15 mm.
- Optional magnet holes at the same ±13 mm positions, screw holes with countersink.

## Label tab
- Depth 15.85 mm, support angle 36°, extra support height 1.2 mm.
