//! `gapsmith-db verify` subcommand.

use std::path::PathBuf;

use anyhow::Context;
use gapsmith_db_core::serde_io;
use gapsmith_db_verify::{
    AtomBalance, ChargeBalance, DlConsistencyCheck, EcValidity, PathwayFluxTest, PmidExistence,
    ThermodynamicFeasibility, UniProtExistence, Verifier, atom_balance, atp_cycle, charge_balance,
    dl_consistency, ec_validity, pathway_flux, pmid_existence, run_all, run_selected, thermo,
    uniprot_existence,
};
use tracing::info;

use crate::VerifyArgs;

#[allow(clippy::needless_pass_by_value)]
pub fn run(args: VerifyArgs) -> anyhow::Result<()> {
    let db = serde_io::read_binary(&args.db)
        .with_context(|| format!("loading DB from {}", args.db.display()))?;

    let mut verifiers: Vec<Box<dyn Verifier>> = vec![
        Box::new(AtomBalance),
        Box::new(ChargeBalance),
        Box::new(EcValidity::new(args.intenz_dat.clone())),
        Box::new(UniProtExistence::new(args.uniprot_snapshot.clone())),
        Box::new(PmidExistence::offline(args.pmid_cache.clone()).with_online(args.pmid_online)),
        Box::new(ThermodynamicFeasibility::new(args.python_project.clone())),
        Box::new({
            let mut v = gapsmith_db_verify::atp_cycle::AtpCycleTest::new(
                args.python_project.clone(),
                args.universal_model.clone(),
            );
            if let Some(e) = args.atp_epsilon {
                v.epsilon = e;
            }
            v
        }),
        Box::new({
            let mut v = PathwayFluxTest::new(args.python_project.clone());
            v.universal_model.clone_from(&args.universal_model);
            v.medium.clone_from(&args.medium);
            v
        }),
        Box::new(DlConsistencyCheck::new(args.dl_signature_out.clone())),
    ];

    let report = if args.only.is_empty() {
        run_all(&mut verifiers, &db)
    } else {
        let only: Vec<&str> = args.only.iter().map(String::as_str).collect();
        run_selected(&mut verifiers, &db, &only)
    };

    info!(
        total = report.summary.total,
        info = report.summary.info,
        warn = report.summary.warning,
        error = report.summary.error,
        "verify complete"
    );

    if let Some(p) = args.report.as_ref() {
        let text = serde_json::to_string_pretty(&report)?;
        std::fs::write(p, text)?;
        info!(path = %p.display(), "wrote report");
    } else {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }

    if report.has_errors() && !args.allow_errors {
        anyhow::bail!("{} error-level diagnostic(s)", report.summary.error);
    }
    Ok(())
}

// Silence unused-import warnings on verifier-name re-exports in this module.
#[allow(dead_code)]
fn _names() -> [&'static str; 9] {
    [
        atom_balance::NAME,
        charge_balance::NAME,
        ec_validity::NAME,
        uniprot_existence::NAME,
        pmid_existence::NAME,
        thermo::NAME,
        atp_cycle::NAME,
        pathway_flux::NAME,
        dl_consistency::NAME,
    ]
}

#[allow(dead_code)]
fn _db_holder(p: PathBuf) -> PathBuf {
    p
}
