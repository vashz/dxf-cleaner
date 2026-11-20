//! Drawing units: what one unit of a source drawing is in mm, read from its `$INSUNITS`.
//!
//! The Tolerance and Join gap are always mm (see `docs/adr/0001-tolerance-is-in-mm.md`), so
//! Clean Up, the window and `dxf-clean` all convert through this one lookup.

/// The units a source drawing is drawn in. A drawing that names no units, or a unit code
/// this table doesn't know, is treated as drawn in mm, and [`DrawingUnits::note`] says so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawingUnits {
    /// How long one drawing unit is, in mm.
    mm_per_unit: f64,
    /// What staff read after "drawn in", e.g. "inches". `None` when treated as mm.
    name: Option<&'static str>,
    /// The `$INSUNITS` value as read.
    code: Option<i32>,
}

impl DrawingUnits {
    /// The units `$INSUNITS` names (`None` when the drawing has no `$INSUNITS`).
    pub fn from_insunits(code: Option<i32>) -> DrawingUnits {
        let (mm_per_unit, name) = match code.map(table).unwrap_or(None) {
            Some((mm, name)) => (mm, Some(name)),
            None => (1.0, None),
        };
        DrawingUnits { mm_per_unit, name, code }
    }

    /// How long one drawing unit is, in mm.
    pub fn mm_per_unit(&self) -> f64 {
        self.mm_per_unit
    }

    /// A distance in mm, in drawing units.
    pub fn from_mm(&self, mm: f64) -> f64 {
        mm / self.mm_per_unit
    }

    /// A distance in drawing units, in mm.
    pub fn to_mm(&self, d: f64) -> f64 {
        d * self.mm_per_unit
    }

    /// Shown next to the file name, e.g. "drawn in inches".
    pub fn drawn_in(&self) -> String {
        match (self.name, self.code) {
            (Some(name), _) => format!("drawn in {name}"),
            (None, None | Some(0)) => "no units: treated as mm".to_string(),
            (None, Some(_)) => "unknown units: treated as mm".to_string(),
        }
    }

    /// What staff are told when the drawing names no units Clean Up knows, so it was
    /// treated as mm. `None` when the units are known.
    pub fn note(&self) -> Option<String> {
        match (self.name, self.code) {
            (Some(_), _) => None,
            (None, None | Some(0)) => Some("No units in drawing: treated as mm".to_string()),
            (None, Some(code)) => Some(format!("Unknown units in drawing ($INSUNITS {code}): treated as mm")),
        }
    }
}

/// mm per unit and the unit's name for every `$INSUNITS` code with a fixed length.
fn table(code: i32) -> Option<(f64, &'static str)> {
    Some(match code {
        1 => (25.4, "inches"),
        2 => (304.8, "feet"),
        3 => (1_609_344.0, "miles"),
        4 => (1.0, "mm"),
        5 => (10.0, "cm"),
        6 => (1000.0, "metres"),
        7 => (1e6, "km"),
        8 => (25.4e-6, "microinches"),
        9 => (0.0254, "mils"),
        10 => (914.4, "yards"),
        11 => (1e-7, "ångströms"),
        12 => (1e-6, "nanometres"),
        13 => (1e-3, "microns"),
        14 => (100.0, "decimetres"),
        15 => (1e4, "decametres"),
        16 => (1e5, "hectometres"),
        17 => (1e12, "gigametres"),
        18 => (1.495_978_707e14, "astronomical units"),
        19 => (9.460_730_472_580_8e18, "light years"),
        20 => (3.085_677_581_491_367e19, "parsecs"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixed_length_code_converts_to_mm() {
        let mm = |code| DrawingUnits::from_insunits(Some(code)).mm_per_unit();
        // (code, mm per unit), from the published unit definitions
        let table = [
            (1, 25.4),
            (2, 304.8),
            (3, 1_609_344.0),
            (4, 1.0),
            (5, 10.0),
            (6, 1000.0),
            (7, 1e6),
            (8, 25.4e-6),
            (9, 0.0254),
            (10, 914.4),
            (11, 1e-7),
            (12, 1e-6),
            (13, 1e-3),
            (14, 100.0),
            (15, 1e4),
            (16, 1e5),
            (17, 1e12),
            (18, 1.495_978_707e14),
            (19, 9.460_730_472_580_8e18),
            (20, 3.085_677_581_491_367e19),
        ];
        for (code, want) in table {
            let got = mm(code);
            assert!((got - want).abs() <= want * 1e-12, "$INSUNITS {code}: {got} mm, want {want}");
            assert!(DrawingUnits::from_insunits(Some(code)).note().is_none(), "$INSUNITS {code} names its units");
        }
    }

    #[test]
    fn a_drawing_with_no_units_is_treated_as_mm_and_says_so() {
        for code in [None, Some(0)] {
            let u = DrawingUnits::from_insunits(code);
            assert_eq!(u.mm_per_unit(), 1.0, "{code:?}");
            assert_eq!(u.note(), Some("No units in drawing: treated as mm".to_string()), "{code:?}");
        }
    }

    #[test]
    fn an_unknown_code_is_treated_as_mm_and_says_so() {
        for code in [-1, 21, 99] {
            let u = DrawingUnits::from_insunits(Some(code));
            assert_eq!(u.mm_per_unit(), 1.0, "{code}");
            assert_eq!(u.note(), Some(format!("Unknown units in drawing ($INSUNITS {code}): treated as mm")));
        }
    }

    #[test]
    fn named_units_read_as_drawn_in() {
        assert_eq!(DrawingUnits::from_insunits(Some(1)).drawn_in(), "drawn in inches");
        assert_eq!(DrawingUnits::from_insunits(Some(4)).drawn_in(), "drawn in mm");
        assert_eq!(DrawingUnits::from_insunits(Some(6)).drawn_in(), "drawn in metres");
        assert_eq!(DrawingUnits::from_insunits(None).drawn_in(), "no units: treated as mm");
        assert_eq!(DrawingUnits::from_insunits(Some(0)).drawn_in(), "no units: treated as mm");
        assert_eq!(DrawingUnits::from_insunits(Some(21)).drawn_in(), "unknown units: treated as mm");
    }

    #[test]
    fn converts_between_mm_and_drawing_units() {
        let inch = DrawingUnits::from_insunits(Some(1));
        assert!((inch.from_mm(25.4) - 1.0).abs() < 1e-12);
        assert!((inch.to_mm(0.5) - 12.7).abs() < 1e-12);
        let km = DrawingUnits::from_insunits(Some(7));
        assert!((km.from_mm(0.05) - 5e-8).abs() < 1e-20);
    }
}
