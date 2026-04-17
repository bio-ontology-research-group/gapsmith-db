//! ChargeBalance verifier. Sum of (coefficient × charge) across each
//! reaction's stoichiometry must be zero. Missing charges are
//! `Info`-level "skipped" (not fail-closed, since charge is genuinely
//! unknown for many curated compounds).

use gapsmith_db_core::Database;
use serde_json::json;

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;

pub const NAME: &str = "charge_balance";

pub struct ChargeBalance;

impl Verifier for ChargeBalance {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        let mut out = Vec::new();
        for r in db.reactions.values() {
            let mut total: i64 = 0;
            let mut skip_reason: Option<String> = None;
            for s in &r.stoichiometry {
                let Some(c) = db.compounds.get(&s.compound) else {
                    skip_reason = Some(format!("missing compound {}", s.compound));
                    break;
                };
                let Some(q) = c.charge else {
                    skip_reason = Some(format!("compound {} has no charge", c.id));
                    break;
                };
                if (s.coefficient - s.coefficient.round()).abs() > 1e-9 {
                    skip_reason = Some(format!(
                        "non-integer coefficient {} on {}",
                        s.coefficient, s.compound
                    ));
                    break;
                }
                #[allow(clippy::cast_possible_truncation)]
                let coef_i = s.coefficient as i64;
                total += i64::from(q) * coef_i;
            }

            if let Some(reason) = skip_reason {
                out.push(Diagnostic::info(
                    NAME,
                    Target::Reaction(r.id.clone()),
                    "skipped",
                    reason,
                ));
                continue;
            }

            if total == 0 {
                out.push(Diagnostic::info(
                    NAME,
                    Target::Reaction(r.id.clone()),
                    "balanced",
                    "charge-balanced",
                ));
            } else {
                out.push(
                    Diagnostic::error(
                        NAME,
                        Target::Reaction(r.id.clone()),
                        "unbalanced",
                        format!("net charge residual {total}"),
                    )
                    .with_details(json!({ "residual": total })),
                );
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gapsmith_db_core::{
        Compartment, Compound, CompoundId, Reaction, ReactionId, Reversibility, StoichiometryEntry,
    };

    fn mk(id: &str, charge: i16) -> Compound {
        let mut c = Compound::new(CompoundId::new(id));
        c.charge = Some(charge);
        c
    }

    #[test]
    fn balanced_if_zero_sum() {
        // Na+ + Cl- <-> NaCl (neutral).
        let mut db = Database::new();
        db.insert_compound(mk("Na_plus", 1));
        db.insert_compound(mk("Cl_minus", -1));
        db.insert_compound(mk("NaCl", 0));
        let mut r = Reaction::new(ReactionId::new("R1"), Reversibility::Reversible);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("Na_plus"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("Cl_minus"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("NaCl"),
            1.0,
            Compartment::Cytosol,
        ));
        db.insert_reaction(r);
        let diags = ChargeBalance.check(&db).unwrap();
        assert_eq!(diags[0].code.0, "balanced");
    }

    #[test]
    fn unbalanced_reports_residual() {
        let mut db = Database::new();
        db.insert_compound(mk("A_plus", 1));
        db.insert_compound(mk("A_neutral", 0));
        let mut r = Reaction::new(ReactionId::new("R_bad"), Reversibility::Reversible);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("A_plus"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("A_neutral"),
            1.0,
            Compartment::Cytosol,
        ));
        db.insert_reaction(r);
        let diags = ChargeBalance.check(&db).unwrap();
        assert_eq!(diags[0].code.0, "unbalanced");
        assert_eq!(diags[0].severity, crate::Severity::Error);
    }
}
