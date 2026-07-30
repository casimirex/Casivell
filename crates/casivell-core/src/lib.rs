//! Exact-arithmetic primitives for the Casivell simulation engine.
//!
//! # Why this crate exists
//!
//! Every monetary value in Casivell is an integer number of cents, and every
//! rate is an integer number of parts-per-million. There is no `f64` anywhere in
//! the calculation path, and `clippy::float_arithmetic` is denied workspace-wide
//! to keep it that way.
//!
//! This is not fastidiousness. German tax law prescribes exact rounding at
//! specific points (for example [`§ 32a Abs. 1 EStG`] requires the taxable
//! income *and* the resulting tax to be truncated to whole euros), and the
//! product promises *"gleiche Eingaben, gleiches Ergebnis, immer"* — identical
//! inputs yield an identical result, on every device, forever. Binary floating
//! point cannot represent `0.01`, does not associate, and is permitted to differ
//! across compilation targets when fused-multiply-add is available. A cent of
//! drift compounded over a 480-month projection is a wrong answer, and an answer
//! that differs between a user's phone and their laptop is an unfixable support
//! ticket.
//!
//! # Engineering standard
//!
//! This crate is `#![no_std]` and `#![forbid(unsafe_code)]`. That is a
//! mechanically enforced version of two of the JPL Power-of-10 rules: with no
//! `std` there is no allocator, so the engine cannot heap-allocate during a
//! simulation (R3), and there are no raw pointers to misuse (R9). All fallible
//! arithmetic returns [`Result`]; none of it panics. See
//! `docs/CODING_STANDARD.md`.
//!
//! [`§ 32a Abs. 1 EStG`]: https://www.gesetze-im-internet.de/estg/__32a.html

#![no_std]
#![forbid(unsafe_code)]
// Library code may not panic — `unwrap`, `expect`, `panic` and bare arithmetic are
// denied workspace-wide. Test code is exempted here, once per crate, because in a
// test a failed constructor on a hard-coded literal *is* the failure being
// reported, and threading `Result` through every assertion would bury the property
// under plumbing. The exemption is `cfg(test)`, so nothing shipped is covered by it.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing,
    )
)]

pub mod money;
pub mod rate;
pub mod rounding;

pub use money::{Money, MoneyError};
pub use rate::Rate;
pub use rounding::{Rounding, div_ceil, div_floor, div_round_half_up, div_trunc};

/// The calendar year a statutory parameter set applies to.
///
/// A newtype rather than a bare `u16` because "year" is threaded through every
/// law lookup, and swapping it with any other small integer is a class of bug
/// worth making impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaxYear(u16);

impl TaxYear {
    /// Earliest year for which Casivell holds a verified parameter set.
    pub const MIN: Self = Self(2025);

    /// Latest year for which Casivell holds a verified parameter set.
    ///
    /// Projections beyond this year must be explicitly flagged to the user as
    /// extrapolated under an assumption, never presented as law.
    pub const MAX: Self = Self(2026);

    /// Constructs a year, rejecting values outside the verified range.
    ///
    /// # Errors
    ///
    /// Returns [`MoneyError::YearOutOfRange`] when no verified parameter set
    /// exists, so that a caller cannot silently receive figures we cannot cite.
    pub const fn new(year: u16) -> Result<Self, MoneyError> {
        if year < Self::MIN.0 || year > Self::MAX.0 {
            return Err(MoneyError::YearOutOfRange { year });
        }
        Ok(Self(year))
    }

    /// Returns the year as a plain integer.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}
