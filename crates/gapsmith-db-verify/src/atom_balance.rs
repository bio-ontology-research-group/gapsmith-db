//! AtomBalance verifier.
//!
//! Per-reaction check using the compound `formula` fields. Semantics per
//! plan.md:
//! - Hydrogen-only imbalances are flagged as `Warning`, not `Error`
//!   (explicit-proton ambiguity in upstream sources).
//! - Reactions where any compound lacks a formula are `Info`-level "skipped".
//! - Any other element mismatch is `Error`.

use gapsmith_db_core::Database;
use serde_json::json;

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;
use crate::formula::{AtomCounts, diff, parse};

pub const NAME: &str = "atom_balance";

pub struct AtomBalance;

impl Verifier for AtomBalance {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        let mut out = Vec::new();
        for r in db.reactions.values() {
            let mut totals = AtomCounts::new();
            let mut skipped_reason: Option<String> = None;

            for s in &r.stoichiometry {
                let Some(c) = db.compounds.get(&s.compound) else {
                    skipped_reason = Some(format!("missing compound {}", s.compound));
                    break;
                };
                let Some(formula) = c.formula.as_deref() else {
                    skipped_reason = Some(format!("compound {} has no formula", c.id));
                    break;
                };
                match parse(formula) {
                    Ok(counts) => {
                        for (k, v) in counts {
                            #[allow(clippy::cast_possible_truncation)]
                            let coef_i = s.coefficient as i64;
                            let scaled = v * coef_i;
                            if scaled != 0 {
                                *totals.entry(k).or_insert(0) += scaled;
                            }
                        }
                        // For non-integer coefficients, emit a warning and skip this reaction.
                        if (s.coefficient - s.coefficient.round()).abs() > 1e-9 {
                            skipped_reason = Some(format!(
                                "non-integer stoichiometric coefficient {} on {}",
                                s.coefficient, s.compound
                            ));
                            break;
                        }
                    }
                    Err(e) => {
                        skipped_reason =
                            Some(format!("compound {} formula unparseable: {e}", c.id));
                        break;
                    }
                }
            }

            if let Some(reason) = skipped_reason {
                out.push(Diagnostic::info(
                    NAME,
                    Target::Reaction(r.id.clone()),
                    "skipped",
                    reason,
                ));
                continue;
            }

            totals.retain(|_, v| *v != 0);
            if totals.is_empty() {
                out.push(Diagnostic::info(
                    NAME,
                    Target::Reaction(r.id.clone()),
                    "balanced",
                    "atom-balanced",
                ));
            } else if hydrogen_only(&totals) {
                out.push(
                    Diagnostic::warn(
                        NAME,
                        Target::Reaction(r.id.clone()),
                        "hydrogen_only_imbalance",
                        "reaction balances except for explicit protons",
                    )
                    .with_details(json!({ "residual": totals })),
                );
            } else {
                out.push(
                    Diagnostic::error(
                        NAME,
                        Target::Reaction(r.id.clone()),
                        "unbalanced",
                        "atom imbalance",
                    )
                    .with_details(json!({ "residual": totals })),
                );
            }
        }
        Ok(out)
    }
}

fn hydrogen_only(counts: &AtomCounts) -> bool {
    counts.keys().all(|k| k == "H")
}

/// Convenience: check a single reaction by ID. Useful from the CLI.
#[must_use]
pub fn balance_of_reaction(
    db: &Database,
    rid: &gapsmith_db_core::ReactionId,
) -> Option<AtomCounts> {
    let r = db.reactions.get(rid)?;
    let mut totals = AtomCounts::new();
    for s in &r.stoichiometry {
        let c = db.compounds.get(&s.compound)?;
        let formula = c.formula.as_deref()?;
        let counts = parse(formula).ok()?;
        for (k, v) in counts {
            #[allow(clippy::cast_possible_truncation)]
            let coef_i = s.coefficient as i64;
            let scaled = v * coef_i;
            if scaled != 0 {
                *totals.entry(k).or_insert(0) += scaled;
            }
        }
    }
    totals.retain(|_, v| *v != 0);
    Some(totals)
}

/// Use the verifier's verdict to mutate `Database::reactions[id].status`
/// in place. Useful after ingestion.
pub fn apply_status(db: &mut Database) {
    use gapsmith_db_core::MassBalanceStatus;
    let mut updates: Vec<(gapsmith_db_core::ReactionId, MassBalanceStatus)> = Vec::new();
    for r in db.reactions.values() {
        let Some(totals) = diff_for(db, &r.id) else {
            continue;
        };
        let status = if totals.is_empty() {
            MassBalanceStatus::Balanced
        } else if hydrogen_only(&totals) {
            MassBalanceStatus::HydrogenOnly
        } else {
            MassBalanceStatus::Unbalanced
        };
        updates.push((r.id.clone(), status));
    }
    for (rid, status) in updates {
        if let Some(r) = db.reactions.get_mut(&rid) {
            r.status = status;
        }
    }
}

fn diff_for(db: &Database, rid: &gapsmith_db_core::ReactionId) -> Option<AtomCounts> {
    let r = db.reactions.get(rid)?;
    let mut totals = AtomCounts::new();
    for s in &r.stoichiometry {
        let c = db.compounds.get(&s.compound)?;
        let formula = c.formula.as_deref()?;
        let counts = parse(formula).ok()?;
        for (k, v) in counts {
            #[allow(clippy::cast_possible_truncation)]
            let coef_i = s.coefficient as i64;
            let scaled = v * coef_i;
            if scaled != 0 {
                *totals.entry(k).or_insert(0) += scaled;
            }
        }
    }
    Some(diff(&totals, &AtomCounts::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gapsmith_db_core::{
        Compartment, Compound, CompoundId, Reaction, ReactionId, Reversibility, StoichiometryEntry,
    };

    fn mk(id: &str, formula: &str) -> Compound {
        let mut c = Compound::new(CompoundId::new(id));
        c.formula = Some(formula.to_string());
        c
    }

    fn db() -> Database {
        let mut db = Database::new();
        db.insert_compound(mk("C_H2O", "H2O"));
        db.insert_compound(mk("C_H", "H"));
        db.insert_compound(mk("C_O", "O"));
        db.insert_compound(mk("C_NO_FORMULA", ""));
        db
    }

    #[test]
    fn balanced_reaction() {
        let mut d = db();
        let mut r = Reaction::new(ReactionId::new("R_balanced"), Reversibility::Reversible);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("C_H"),
            2.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("C_O"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("C_H2O"),
            1.0,
            Compartment::Cytosol,
        ));
        d.insert_reaction(r);
        let diags = AtomBalance.check(&d).unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.0, "balanced");
    }

    #[test]
    fn hydrogen_only_imbalance_is_warning() {
        let mut d = db();
        let mut r = Reaction::new(ReactionId::new("R_honly"), Reversibility::Reversible);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("C_H2O"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("C_O"),
            1.0,
            Compartment::Cytosol,
        ));
        // Missing 2 H — this is the "explicit proton ambiguity" case.
        d.insert_reaction(r);
        let diags = AtomBalance.check(&d).unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, crate::Severity::Warning);
        assert_eq!(diags[0].code.0, "hydrogen_only_imbalance");
    }

    #[test]
    fn unbalanced_is_error() {
        let mut d = db();
        let mut r = Reaction::new(ReactionId::new("R_bad"), Reversibility::Reversible);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("C_H2O"),
            1.0,
            Compartment::Cytosol,
        ));
        // no product; glaring O and H imbalance
        d.insert_reaction(r);
        let diags = AtomBalance.check(&d).unwrap();
        assert_eq!(diags[0].severity, crate::Severity::Error);
        assert_eq!(diags[0].code.0, "unbalanced");
    }

    #[test]
    fn missing_formula_is_skipped_info() {
        let mut d = db();
        let mut r = Reaction::new(ReactionId::new("R_skip"), Reversibility::Reversible);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("C_NO_FORMULA"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("C_H2O"),
            1.0,
            Compartment::Cytosol,
        ));
        d.insert_reaction(r);
        let diags = AtomBalance.check(&d).unwrap();
        assert_eq!(diags[0].severity, crate::Severity::Info);
        assert_eq!(diags[0].code.0, "skipped");
    }
}
