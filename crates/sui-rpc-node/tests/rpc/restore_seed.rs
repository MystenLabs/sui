// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Confirms `seed_current_epoch_start` reconstructs the current
//! epoch's `epochs` row straight from a restored object set's
//! `SuiSystemState` — the start record a restore-then-tip flow would
//! otherwise never write (tip indexing resumes past the end-of-epoch
//! checkpoint that carries it). Uses a Simulacrum genesis as the
//! object source since it ships a real system state.

use simulacrum::Simulacrum;
use sui_consistent_store::ChainId;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::Watermark;
use sui_rpc_store::HISTORY_COHORT;
use sui_rpc_store::LIVE_COHORT;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::seed_current_epoch_start;
use sui_rpc_store::seed_history_cohort;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::sui_system_state::get_sui_system_state;

#[test]
fn seed_reconstructs_epoch_row_from_system_state() {
    let sim = Simulacrum::new();
    let dir = tempfile::tempdir().unwrap();
    let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

    // The restore anchor's checkpoint + 1 is the seeded epoch's first
    // checkpoint; use an arbitrary value to assert it round-trips.
    let mut batch = db.batch();
    let epoch = seed_current_epoch_start(&schema, sim.store(), Some(42), &mut batch)
        .expect("seed from the genesis system state");
    batch.commit().unwrap();

    let info = schema
        .get_epoch(epoch)
        .unwrap()
        .expect("the seeded epoch row must exist");
    assert_eq!(info.epoch, epoch);
    assert!(info.system_state.is_some(), "system state must be recorded");
    assert!(info.protocol_version.is_some(), "protocol version seeded");
    assert!(info.reference_gas_price.is_some(), "gas price seeded");
    assert_eq!(info.start_checkpoint, Some(42));

    // The committee is derived from the stored system state, so it
    // resolves only because the seed recorded `system_state_bcs`.
    assert!(
        schema.get_committee(epoch).unwrap().is_some(),
        "committee derivable from the seeded system state",
    );
}

/// The embedded flow: `seed_history_cohort` seeds the history
/// pipelines to the lowest available checkpoint `L` and, from the
/// validator's (here, Simulacrum's) on-chain system state, a
/// *partial* current-epoch row — with no `start_checkpoint`, because
/// the mid-epoch restore can't know it. `get_committee` and Move
/// type-layout resolution still work off the recorded system state.
#[test]
fn seed_history_cohort_seeds_partial_epoch_and_history_watermarks() {
    let sim = Simulacrum::new();
    let dir = tempfile::tempdir().unwrap();
    let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

    let chain_id = ChainId([5u8; 32]);
    // Embedded restore lands mid-epoch at tip `T`; the history cohort
    // is seeded to the lowest available checkpoint `L`.
    let l = Watermark::for_checkpoint(1_000);
    seed_history_cohort(&db, &schema, l, chain_id, Some(sim.store()))
        .expect("seed the embedded history cohort");

    // Every history-cohort pipeline resumes from L and is pinned to
    // the chain; the live cohort (the restore driver's job) is left
    // untouched.
    let framework = FrameworkSchema::new(db.clone());
    for name in HISTORY_COHORT {
        let key = PipelineTaskKey::new(*name);
        assert_eq!(
            framework.watermarks.get(&key).unwrap(),
            Some(l),
            "{name} should resume from L",
        );
        assert_eq!(framework.chain_ids.get(&key).unwrap(), Some(chain_id));
    }
    for name in LIVE_COHORT {
        let key = PipelineTaskKey::new(*name);
        assert!(
            framework.watermarks.get(&key).unwrap().is_none(),
            "{name} must be untouched by the history seed",
        );
    }

    // The partial epoch seed reconstructed the current epoch row from
    // the system state: protocol version, system state, and committee
    // resolve, but `start_checkpoint` is unset.
    let epoch = get_sui_system_state(sim.store()).unwrap().epoch();
    let info = schema
        .get_epoch(epoch)
        .unwrap()
        .expect("the seeded epoch row must exist");
    assert!(info.system_state.is_some(), "system state must be recorded");
    assert!(info.protocol_version.is_some(), "protocol version seeded");
    assert_eq!(
        info.start_checkpoint, None,
        "mid-epoch restore cannot know the epoch's first checkpoint",
    );
    assert!(
        schema.get_committee(epoch).unwrap().is_some(),
        "committee derivable from the seeded system state",
    );
}
