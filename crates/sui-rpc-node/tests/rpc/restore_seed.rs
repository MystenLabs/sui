// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Confirms `seed_current_epoch_start` reconstructs the current
//! epoch's `epochs` row straight from a restored object set's
//! `SuiSystemState` — the start record a restore-then-tip flow would
//! otherwise never write (tip indexing resumes past the end-of-epoch
//! checkpoint that carries it). Uses a Simulacrum genesis as the
//! object source since it ships a real system state.

use simulacrum::Simulacrum;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::seed_current_epoch_start;

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
