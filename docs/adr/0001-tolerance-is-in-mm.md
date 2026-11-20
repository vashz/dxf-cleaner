# Tolerance and join gap are always in mm

The Tolerance and Join gap used to be raw drawing units, so the mm-sized defaults (0.05) meant 1.27 mm in an inch drawing and 50 mm in a metre one, and Clean Up silently dropped distinct shapes as duplicates. We decided both are always physical distances in mm, converted to drawing units from `$INSUNITS` (a drawing with no or an unknown unit is treated as mm, and staff are told), while the cleaned file keeps the source drawing's units and is never rescaled. Staff and cutting machines think in mm, so one number should mean the same physical distance in every drawing.

## Considered Options

- **Drawing units with a default per unit** (0.05 mm, 0.002 in, …): rejected, because staff can still type an mm value into an inch drawing and get the same silent loss.
- **Keep drawing units, warn when the drawing isn't in mm**: rejected, because a warning doesn't stop the wrong tolerance being used.
- **Convert the whole drawing to mm on output**: rejected, because nesting software expects the file in the units it was drawn in.
