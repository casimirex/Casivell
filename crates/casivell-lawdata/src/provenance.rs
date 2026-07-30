//! Where a number came from, and how much it should be trusted.

/// How firmly a statutory figure is established.
///
/// Ordered by decreasing confidence, and [`DataStatus::weakest`] lets a composite
/// result inherit the confidence of its least certain input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataStatus {
    /// In force. Published in the Bundesgesetzblatt or an equivalent binding
    /// instrument. This is the only status that may be presented to a user as
    /// "the law says".
    Enacted,
    /// Passed one chamber, or published as a Referentenentwurf, but not yet in
    /// force. Must be labelled in the UI.
    Draft,
    /// Casivell's own extrapolation, with no legislative basis at all. Every
    /// figure past the last enacted year is necessarily this. Must be labelled,
    /// and the assumption behind it must be inspectable by the user.
    Projected,
}

impl DataStatus {
    /// Returns whichever of the two statuses carries less confidence.
    #[must_use]
    pub const fn weakest(self, other: Self) -> Self {
        // The derived `Ord` runs Enacted < Draft < Projected, so "weakest" is the
        // maximum. Spelled out rather than calling `max` so that reordering the
        // variants cannot silently invert the meaning.
        if (self as u8) >= (other as u8) {
            self
        } else {
            other
        }
    }

    /// Whether a figure may be presented to the user as settled law.
    #[must_use]
    pub const fn is_binding_law(self) -> bool {
        matches!(self, Self::Enacted)
    }
}

/// The citation for a statutory parameter set.
///
/// All fields are `&'static str` so that a [`Provenance`] costs nothing at
/// runtime and can live in a `const` table in a `#![no_std]` crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    /// The controlling provision, as it would be cited in writing.
    ///
    /// For example `"§ 32a Abs. 1 EStG (Fassung ab VZ 2026, Art. 2 SteFeG)"`.
    /// Precise enough that a reader can find the exact text, including which
    /// amending act produced the version used.
    pub legal_basis: &'static str,

    /// A URL where the cited text can be read.
    ///
    /// Preferably a primary source: `gesetze-im-internet.de` for statutes,
    /// the issuing ministry for ordinances. Never a tax blog or a calculator.
    pub source_url: &'static str,

    /// ISO-8601 date on which a human compared this struct against the source.
    ///
    /// Not the date the law was passed — the date the transcription was checked.
    /// It answers "how stale is our copy", which is the question that matters.
    pub verified_on: &'static str,

    /// How much the figures should be trusted. See [`DataStatus`].
    pub status: DataStatus,
}

impl Provenance {
    /// Constructs a citation.
    #[must_use]
    pub const fn new(
        legal_basis: &'static str,
        source_url: &'static str,
        verified_on: &'static str,
        status: DataStatus,
    ) -> Self {
        Self {
            legal_basis,
            source_url,
            verified_on,
            status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DataStatus, Provenance};

    #[test]
    fn weakest_picks_the_less_certain_status() {
        assert_eq!(
            DataStatus::Enacted.weakest(DataStatus::Projected),
            DataStatus::Projected
        );
        assert_eq!(
            DataStatus::Projected.weakest(DataStatus::Enacted),
            DataStatus::Projected
        );
        assert_eq!(
            DataStatus::Draft.weakest(DataStatus::Enacted),
            DataStatus::Draft
        );
        assert_eq!(
            DataStatus::Enacted.weakest(DataStatus::Enacted),
            DataStatus::Enacted
        );
    }

    /// `weakest` must be commutative and idempotent; if it is not, the status a
    /// user sees would depend on the order the engine happened to combine inputs.
    #[test]
    fn weakest_is_commutative_and_idempotent() {
        let all = [
            DataStatus::Enacted,
            DataStatus::Draft,
            DataStatus::Projected,
        ];
        for a in all {
            assert_eq!(a.weakest(a), a);
            for b in all {
                assert_eq!(a.weakest(b), b.weakest(a));
            }
        }
    }

    #[test]
    fn only_enacted_figures_are_binding() {
        assert!(DataStatus::Enacted.is_binding_law());
        assert!(!DataStatus::Draft.is_binding_law());
        assert!(!DataStatus::Projected.is_binding_law());
    }

    /// A citation with an empty legal basis or a non-primary source is the kind of
    /// thing that slips in under deadline. The tariff and social tables are
    /// checked against this in their own modules; this test just pins the shape.
    #[test]
    fn citations_retain_what_they_were_given() {
        let p = Provenance::new(
            "§ 32a Abs. 1 EStG",
            "https://www.gesetze-im-internet.de/estg/__32a.html",
            "2026-07-30",
            DataStatus::Enacted,
        );
        assert_eq!(p.legal_basis, "§ 32a Abs. 1 EStG");
        assert_eq!(p.status, DataStatus::Enacted);
        assert_eq!(p.verified_on, "2026-07-30");
    }
}
