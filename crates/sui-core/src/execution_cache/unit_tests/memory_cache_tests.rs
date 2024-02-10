// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::{rngs::StdRng, SeedableRng};
use std::{fs, sync::Arc};
use sui_types::transaction::TransactionData;
use tempfile::tempdir;

use super::*;
use crate::{
    authority::{authority_store_tables::AuthorityPerpetualTables, AuthorityStore},
    test_utils::init_state_parameters_from_rng,
};

async fn init_authority_store() -> Arc<AuthorityStore> {
    let seed = [1u8; 32];
    let (genesis, _) = init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    let committee = genesis.committee().unwrap();

    // Create a random directory to store the DB
    let dir = tempdir().unwrap();
    let db_path = dir.path();
    fs::create_dir(&db_path).unwrap();

    let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(&db_path, None));
    AuthorityStore::open_with_committee_for_testing(perpetual_tables, &committee, &genesis, 0)
        .await
        .unwrap()
        .into()
}

fn make_transction_outputs() -> Arc<TransactionOutputs> {
    Arc::new(TransactionOutputs {
        transaction: Arc::new(VerifiedTransaction::new(
            TransactionData::new(0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            vec![],
        )),
        effects: TransactionEffects::new(),
        events: TransactionEvents::new(),
        markers: vec![],
        wrapped: vec![],
        deleted: vec![],
        locks_to_delete: vec![],
        new_locks_to_init: vec![],
        written: WrittenObjects::new(),
    })
}

#[tokio::test]
async fn test_object_methods() {
    let authority_store = init_authority_store().await;
}
