//! Property tests for schema invariants.
//!
//! plan.md specifies:
//!   - every compound has ≥1 identifier
//!   - every reaction's stoichiometry references known compounds
//!   - every EC number parses
//!
//! These run via proptest on randomly generated valid databases.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::str::FromStr;

use gapsmith_db_core::{
    Compartment, Compound, CompoundId, Database, EcNumber, Pathway, PathwayId, Reaction,
    ReactionId, Reversibility, StoichiometryEntry,
};
use proptest::prelude::*;

/// Arbitrary EC number string with 4 dot-separated levels (digits or `-`).
fn arb_ec_string() -> impl Strategy<Value = String> {
    let level_head = 1_u16..=9;
    let level_tail = prop_oneof![
        (0_u16..=99).prop_map(|n| n.to_string()),
        Just("-".to_string()),
    ];
    (
        level_head.prop_map(|n| n.to_string()),
        level_tail.clone(),
        level_tail.clone(),
        level_tail,
    )
        .prop_map(|(a, b, c, d)| format!("{a}.{b}.{c}.{d}"))
}

fn arb_compound_id() -> impl Strategy<Value = CompoundId> {
    (1_u32..=1_000_000).prop_map(|n| CompoundId::new(format!("C{n:07}")))
}

fn arb_compound() -> impl Strategy<Value = Compound> {
    (arb_compound_id(), prop::option::of("[A-Za-z0-9 ]{0,20}")).prop_map(|(id, maybe_name)| {
        let mut c = Compound::new(id);
        if let Some(n) = maybe_name {
            c.names.push(n);
        }
        c
    })
}

/// Build a [`Database`] whose reaction stoichiometry is guaranteed to only
/// reference compounds present in the compound table. This is the
/// precondition we rely on for the `validate()` property below.
fn arb_valid_database() -> impl Strategy<Value = Database> {
    prop::collection::vec(arb_compound(), 1..=8).prop_flat_map(|compounds| {
        let ids: Vec<CompoundId> = compounds.iter().map(|c| c.id.clone()).collect();
        let id_count = ids.len();

        let reactions = prop::collection::vec(
            (0_usize..id_count, 0_usize..id_count, any::<bool>()).prop_map({
                let ids = ids.clone();
                move |(i, j, forward)| {
                    let mut r = Reaction::new(
                        ReactionId::new(format!("R{:05}", i * 997 + j * 31)),
                        if forward {
                            Reversibility::Forward
                        } else {
                            Reversibility::Reversible
                        },
                    );
                    r.stoichiometry.push(StoichiometryEntry::substrate(
                        ids[i].clone(),
                        1.0,
                        Compartment::Cytosol,
                    ));
                    r.stoichiometry.push(StoichiometryEntry::product(
                        ids[j].clone(),
                        1.0,
                        Compartment::Cytosol,
                    ));
                    r
                }
            }),
            0..=6,
        );

        reactions.prop_map(move |rxns| {
            let mut db = Database::new();
            for c in compounds.clone() {
                db.insert_compound(c);
            }
            // Deduplicate reactions by ID (randomly generated indices can collide).
            for r in rxns {
                db.insert_reaction(r);
            }
            // Add one pathway referencing a slice of the reactions for completeness.
            let rxn_ids: Vec<ReactionId> = db.reactions.keys().cloned().collect();
            let mut p = Pathway::new(PathwayId::new("P00001"), "synthetic");
            p.reactions = rxn_ids;
            db.insert_pathway(p);
            db
        })
    })
}

proptest! {
    #[test]
    fn generated_databases_validate(db in arb_valid_database()) {
        db.validate().expect("generated DB should satisfy invariants");
    }

    #[test]
    fn compound_has_at_least_one_identifier(c in arb_compound()) {
        prop_assert!(c.identifier_count() >= 1);
        prop_assert!(!c.id.as_str().is_empty());
    }

    #[test]
    fn every_reaction_stoichiometry_is_resolvable(db in arb_valid_database()) {
        for r in db.reactions.values() {
            for s in &r.stoichiometry {
                prop_assert!(
                    db.compounds.contains_key(&s.compound),
                    "reaction {} references missing compound {}",
                    r.id,
                    s.compound,
                );
            }
        }
    }

    #[test]
    fn ec_numbers_parse_and_roundtrip(s in arb_ec_string()) {
        let parsed: EcNumber = s.parse().expect("arb_ec_string should parse");
        let rendered = parsed.to_string();
        let reparsed: EcNumber = EcNumber::from_str(&rendered)
            .expect("render -> parse must round-trip");
        prop_assert_eq!(parsed, reparsed);
    }
}
