//! Saving and replaying a scenario.
//!
//! # A scenario is an invocation plus a Datenstand
//!
//! The obvious design is to serialise the household, the config and the schedule. This does
//! not, and the reason is worth stating: a parallel serialisation drifts. Every field added to
//! `Household` is a field someone must remember to write, read, version and default, and the
//! failure mode is silent — an old file loads, one setting is quietly missing, and the numbers
//! are subtly wrong.
//!
//! So a scenario file records **the arguments** and the **fingerprint of the statutory data**
//! they were computed against. Replaying re-runs the same arguments through the same code
//! path, which is exact by construction: there is no second representation to disagree with
//! the first. A field added to `Household` needs nothing here at all.
//!
//! # What the fingerprint is for
//!
//! Casivell's tables change, and when they do a scenario stops reproducing. Replay recomputes
//! the digest of the enacted data and compares. Same digest, and the numbers are guaranteed
//! identical. Different, and the report says so plainly rather than leaving a household to
//! wonder why its projection moved.
//!
//! The *enacted* data is what is pinned. Projected years follow from it deterministically
//! given the assumptions, and the assumptions are in the arguments — so pinning the enacted
//! base is sufficient, and pinning a projected year would only restate it.
//!
//! # The format
//!
//! Line-based `key = value`, with `#` comments and `[section]` headers. Hand-written because
//! this workspace has no external dependencies and a scenario file is not worth acquiring one
//! for — and because a household should be able to read, diff and edit it in any text editor.

use std::fmt::Write as _;

use casivell_core::TaxYear;
use casivell_lawdata::{Fingerprinted as _, LawYear};

/// The schema version this build writes.
///
/// Read files carrying a *later* version are refused rather than guessed at: a newer Casivell
/// may record something this one would silently ignore, and silently ignoring part of a saved
/// scenario is the failure this whole design exists to avoid.
pub(crate) const SCHEMA: u32 = 1;

/// A saved scenario.
#[derive(Debug)]
pub(crate) struct Scenario {
    /// The sub-command, or `None` for the bare payslip form.
    pub(crate) form: Option<String>,
    /// The arguments, exactly as given.
    pub(crate) args: Vec<String>,
    /// The enacted year whose data was pinned.
    pub(crate) law_year: u16,
    /// The digest of that year's statutory data.
    pub(crate) fingerprint: String,
}

impl Scenario {
    /// Captures the current invocation.
    ///
    /// # Errors
    ///
    /// A message if the enacted data for `law_year` cannot be resolved.
    pub(crate) fn capture(
        form: Option<String>,
        args: Vec<String>,
        law_year: u16,
    ) -> Result<Self, String> {
        Ok(Self {
            form,
            args,
            law_year,
            fingerprint: fingerprint_of(law_year)?,
        })
    }

    /// Renders the file.
    pub(crate) fn to_text(&self) -> String {
        let mut out = String::with_capacity(512);
        let _ = writeln!(out, "# Casivell scenario");
        let _ = writeln!(
            out,
            "# Replay with: casivell replay <this file>. Editable by hand."
        );
        let _ = writeln!(out, "schema = {SCHEMA}\n");

        let _ = writeln!(out, "[invocation]");
        let _ = writeln!(out, "form = {}", self.form.as_deref().unwrap_or("payslip"));
        // One argument per line rather than a single joined string, so a value containing a
        // space survives the round trip without a quoting rule nobody would remember.
        for arg in &self.args {
            let _ = writeln!(out, "arg = {arg}");
        }

        let _ = writeln!(out, "\n[law]");
        let _ = writeln!(out, "year = {}", self.law_year);
        let _ = writeln!(out, "fingerprint = {}", self.fingerprint);
        out
    }

    /// Parses a file.
    ///
    /// # Errors
    ///
    /// A message naming what was wrong: an unreadable schema, a future one, or a missing
    /// field. Never a partial scenario — half a saved invocation would compute something
    /// nobody asked for.
    pub(crate) fn from_text(text: &str) -> Result<Self, String> {
        let (mut schema, mut form, mut year, mut fingerprint) = (None, None, None, None);
        let mut args = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!(
                    "cannot read the line {line:?}; expected key = value."
                ));
            };
            let value = value.trim().to_owned();
            match key.trim() {
                "schema" => schema = Some(value),
                "form" => form = Some(value),
                "arg" => args.push(value),
                "year" => year = Some(value),
                "fingerprint" => fingerprint = Some(value),
                other => return Err(format!("unknown key {other:?} in the scenario file.")),
            }
        }

        let schema: u32 = schema
            .ok_or("the file has no schema line; it may not be a Casivell scenario.")?
            .parse()
            .map_err(|_| "the schema line is not a number.".to_owned())?;
        if schema > SCHEMA {
            return Err(format!(
                "this scenario was written by a newer Casivell (schema {schema}, this build \
                 reads {SCHEMA}). Refusing rather than ignoring what it may contain."
            ));
        }

        let form = form.ok_or("the file has no form line.")?;
        Ok(Self {
            form: (form != "payslip").then_some(form),
            args,
            law_year: year
                .ok_or("the file records no law year.")?
                .parse()
                .map_err(|_| "the law year is not a number.".to_owned())?,
            fingerprint: fingerprint.ok_or("the file records no fingerprint.")?,
        })
    }

    /// Compares the saved digest against the current data.
    ///
    /// `Ok(None)` where they agree. `Ok(Some(message))` where they do not — which is a warning
    /// and not an error, because the household still wants its numbers; it just needs to know
    /// they are not the numbers it saved.
    ///
    /// # Errors
    ///
    /// A message if the year's data cannot be resolved at all.
    pub(crate) fn check_law(&self) -> Result<Option<String>, String> {
        let current = fingerprint_of(self.law_year)?;
        if current == self.fingerprint {
            return Ok(None);
        }
        Ok(Some(format!(
            "The statutory data for {} has changed since this scenario was saved\n  \
             (saved {}, now {}). The figures below are computed from the current data,\n  \
             so they will not match what was saved. This is the law being corrected or\n  \
             updated, not an error.",
            self.law_year, self.fingerprint, current
        )))
    }
}

/// The digest of an enacted year's data.
fn fingerprint_of(year: u16) -> Result<String, String> {
    let tax_year =
        TaxYear::new(year).map_err(|_| format!("{year} is not a representable year."))?;
    let law = LawYear::for_year(tax_year).map_err(|_| {
        format!(
            "{year} has no enacted statutory data, so there is nothing to pin a scenario to. \
             Scenarios pin an enacted year; projections from it follow from the assumptions in \
             the arguments."
        )
    })?;
    Ok(law.fingerprint().to_string())
}

#[cfg(test)]
mod tests {
    use super::{SCHEMA, Scenario};

    fn saved() -> Scenario {
        Scenario::capture(
            Some("project".to_owned()),
            vec![
                "--gross".to_owned(),
                "6000".to_owned(),
                "--class".to_owned(),
                "1".to_owned(),
            ],
            2026,
        )
        .expect("captures")
    }

    /// A scenario must survive the round trip exactly. Anything less and a replay computes
    /// something other than what was saved, which is the one thing this file must not do.
    #[test]
    fn a_scenario_round_trips() {
        let original = saved();
        let parsed = Scenario::from_text(&original.to_text()).expect("parses");

        assert_eq!(parsed.form, original.form);
        assert_eq!(parsed.args, original.args);
        assert_eq!(parsed.law_year, original.law_year);
        assert_eq!(parsed.fingerprint, original.fingerprint);
    }

    /// The bare payslip form has no sub-command, and must round trip as such rather than as
    /// the literal word.
    #[test]
    fn the_payslip_form_round_trips_as_no_form() {
        let scenario = Scenario::capture(None, vec!["--gross".to_owned()], 2026).unwrap();
        let parsed = Scenario::from_text(&scenario.to_text()).unwrap();
        assert_eq!(parsed.form, None);
    }

    /// An argument containing a space must survive, which is why arguments are one per line.
    #[test]
    fn an_argument_containing_a_space_survives() {
        let scenario = Scenario::capture(
            Some("assess".to_owned()),
            vec!["--note".to_owned(), "two words".to_owned()],
            2026,
        )
        .unwrap();
        let parsed = Scenario::from_text(&scenario.to_text()).unwrap();
        assert_eq!(parsed.args, vec!["--note", "two words"]);
    }

    /// Unchanged data must compare clean.
    #[test]
    fn a_scenario_saved_now_still_matches_now() {
        assert_eq!(saved().check_law().expect("resolves"), None);
    }

    /// A moved digest must be reported, in words a household can act on.
    #[test]
    fn a_changed_digest_is_reported_rather_than_ignored() {
        let mut stale = saved();
        stale.fingerprint = "0000000000000000".to_owned();

        let warning = stale
            .check_law()
            .expect("resolves")
            .expect("a changed digest must be reported");
        assert!(warning.contains("changed"));
        assert!(warning.contains("0000000000000000"), "names what was saved");
        assert!(warning.contains("not an error"), "and says what it means");
    }

    /// A scenario from a newer Casivell is refused rather than half-read.
    #[test]
    fn a_future_schema_is_refused() {
        let text = format!(
            "schema = {}\nform = project\nyear = 2026\nfingerprint = x",
            SCHEMA + 1
        );
        let error = Scenario::from_text(&text).expect_err("must refuse");
        assert!(error.contains("newer Casivell"));
        assert!(error.contains("Refusing"));
    }

    /// Missing fields are named rather than defaulted, because a defaulted scenario computes
    /// something nobody asked for.
    #[test]
    fn missing_fields_are_named() {
        for (text, expected) in [
            ("form = project\n", "schema"),
            ("schema = 1\n", "form"),
            ("schema = 1\nform = project\n", "law year"),
            ("schema = 1\nform = project\nyear = 2026\n", "fingerprint"),
        ] {
            let error = Scenario::from_text(text).expect_err("must refuse");
            assert!(
                error.contains(expected),
                "the error {error:?} should name {expected:?}"
            );
        }
    }

    /// An unknown key is refused rather than skipped: it may be the very setting that makes
    /// the saved figures what they are.
    #[test]
    fn an_unknown_key_is_refused() {
        let text = "schema = 1\nform = project\nyear = 2026\nfingerprint = x\nsurprise = 1\n";
        assert!(Scenario::from_text(text).is_err());
    }

    /// Comments, blank lines and section headers are ignored, so the file stays readable.
    #[test]
    fn comments_and_sections_are_ignored() {
        let text = "# a comment\n\n[invocation]\nschema = 1\nform = project\n\n[law]\n\
                    year = 2026\nfingerprint = x\n";
        assert!(Scenario::from_text(text).is_ok());
    }

    /// A year with no enacted data cannot anchor a scenario, and the message says why.
    #[test]
    fn a_projected_year_cannot_anchor_a_scenario() {
        let error = Scenario::capture(None, Vec::new(), 2040).expect_err("must refuse");
        assert!(error.contains("no enacted statutory data"));
    }
}
