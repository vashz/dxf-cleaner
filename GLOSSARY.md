# DXF Cleaner

Turns polluted DXF exports (e.g. from Adobe Illustrator) into outlines of straight lines and true arcs that cutting and nesting software accept.
_Avoid_: DXF Converter (its name before 0.2.0)

## Language

### Outlines

**Clean Up**:
Turning a polluted drawing into the fewest segments that stay within the tolerance, joined into paths.
_Avoid_: Fix, optimise

**Segment**:
One straight line or one true circular arc.
_Avoid_: Piece, edge, element

**Span**:
One stretch of an entity's original geometry described by one formula: a line, an arc, part of an ellipse, one span of a spline, or a run of fit points. Clean Up replaces each span with segments.
_Avoid_: Piece

**Path**:
A continuous run of segments on one layer, each starting exactly where the previous one ends.
_Avoid_: Polyline (that is one DXF way of writing a Path), contour, chain

**Shape**:
One outline in the cleaned output: a path, or a full circle.
_Avoid_: Entity (that is the DXF record it is written as), object

**Closed path**:
A path that ends exactly where it starts. Every cut outline must be a closed path; nesting software rejects open ones.
_Avoid_: Loop

**Open path**:
A path whose ends do not meet. Only acceptable for marking lines.
_Avoid_: Broken path

**Marking line**:
Geometry that is engraved (e.g. laser marking) rather than cut through; it may legitimately be an open path.
_Avoid_: Engraving, etch

### Accuracy

**Tolerance**:
The furthest Clean Up may move any part of the original geometry. It is a physical distance in mm, whatever units the drawing is in. Shapes closer together than the tolerance count as duplicates.
_Avoid_: Precision, accuracy

**Drawing units**:
The units a source drawing is drawn in. The cleaned file keeps them: Clean Up never rescales the geometry. A drawing that names no units is treated as drawn in mm, and staff are told so.
_Avoid_: Scale

**Deviation**:
The distance between the original geometry and the segments that replace it.
_Avoid_: Error

**Join gap**:
The largest distance between two path ends that still counts as touching, so the ends are joined. Like the tolerance, it is a physical distance in mm, and it may be wider than the tolerance.
_Avoid_: Join tolerance, snap distance

**Bridge**:
A straight segment that fills a gap between two path ends that could not meet without moving the original geometry past the tolerance. It is the only part of the output not traced from the original geometry.
_Avoid_: Filler, patch

### Installer

**Setup**:
The one-file program staff run to put DXF Cleaner on their own Windows account. It carries exactly one version of the app.
_Avoid_: Installer package, MSI

**Install**:
Putting the app on an account that does not have it yet.
_Avoid_: Deploy

**Update**:
Running a setup whose version is newer than the installed one; it replaces the app in place.
_Avoid_: Upgrade, patch

**Reinstall**:
Running a setup whose version is the same as the installed one, or when the installed version is unknown.
_Avoid_: Repair

**Downgrade**:
Running a setup whose version is older than the installed one. Setup asks first, and refuses when run quietly.
_Avoid_: Rollback

**Uninstall**:
Removing the app, its shortcuts and its "Apps & features" entry from the account. Staff see it as "Remove DXF Cleaner from this computer?", so *remove* means the same thing.
_Avoid_: Delete
