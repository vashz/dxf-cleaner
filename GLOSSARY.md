# DXF Converter

Turns polluted DXF exports (e.g. from Adobe Illustrator) into outlines of straight lines and true arcs that cutting and nesting software accept.

## Language

### Outlines

**Clean Up**:
Turning a polluted drawing into the fewest segments that stay within the tolerance, joined into paths.
_Avoid_: Fix, optimise

**Segment**:
One straight line or one true circular arc.
_Avoid_: Piece, edge, element

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
The furthest Clean Up may move any part of the original geometry, in drawing units (usually mm).
_Avoid_: Precision, accuracy

**Deviation**:
The distance between the original geometry and the segments that replace it.
_Avoid_: Error

**Join gap**:
The largest distance between two path ends that still counts as touching, so the ends are joined. It may be wider than the tolerance.
_Avoid_: Join tolerance, snap distance

**Bridge**:
A straight segment that fills a gap between two path ends that could not meet without moving the original geometry past the tolerance. It is the only part of the output not traced from the original geometry.
_Avoid_: Filler, patch

### Installer

**Setup**:
The one-file program staff run to put DXF Converter on their own Windows account. It carries exactly one version of the app.
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
Removing the app, its shortcuts and its "Apps & features" entry from the account.
_Avoid_: Delete, remove
