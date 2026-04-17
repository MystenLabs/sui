// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adapter-level integration test for `ForkedTransactionExecutor`: runs an
//! `ExecuteTransactionRequestV3` through the same entry point the gRPC
//! `TransactionExecutionService` uses, and asserts the forked Simulacrum
//! produced matching effects. The gRPC socket is intentionally not exercised
//! here — an end-to-end tonic-client test belongs under the broader Testing
//! TODO item.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use rand::rngs::OsRng;

use simulacrum::Simulacrum;
use simulacrum::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_protocol_config::Chain;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::KeypairTraits;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{GasData, Transaction, TransactionData, TransactionKind};
use sui_types::transaction_driver_types::ExecuteTransactionRequestV3;
use sui_types::transaction_executor::TransactionExecutor;

use crate::context::Context;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::store::DataStore;

#[tokio::test]
async fn executor_execute_transaction_runs_against_forked_simulacrum() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .build();

    let mut data_store = DataStore::new_for_testing(temp.path().to_path_buf());
    let written: BTreeMap<ObjectID, Object> = config
        .genesis
        .objects()
        .iter()
        .map(|o| (o.id(), o.clone()))
        .collect();
    data_store.update_objects(written, vec![]);

    let keystore = KeyStore::from_network_config(&config);
    let sim = Simulacrum::new_from_custom_state(
        keystore,
        config.genesis.checkpoint(),
        config.genesis.sui_system_object(),
        &config,
        data_store.clone(),
        rng,
    );
    let reference_gas_price = sim.reference_gas_price();

    // Pick the first genesis account as the sender.
    let (sender, sender_key) = {
        let (addr, key) = sim
            .keystore()
            .accounts()
            .next()
            .expect("at least one account");
        (*addr, key.copy())
    };

    let gas_object = config
        .genesis
        .objects()
        .iter()
        .find(|obj| obj.owner == Owner::AddressOwner(sender) && obj.is_gas_coin())
        .cloned()
        .expect("sender should have a gas coin in genesis");

    let context = Arc::new(Context::new(sim, data_store, Chain::Unknown));
    let executor = ForkedTransactionExecutor::new(context);

    // Build a transfer-SUI transaction signed by the genesis sender key;
    // the adapter verifies signatures against the sender via Simulacrum's
    // standard execute path.
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(SuiAddress::random_for_testing_only(), Some(1_000));
        builder.finish()
    };
    let tx_data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(pt),
        sender,
        GasData {
            payment: vec![gas_object.compute_object_reference()],
            owner: sender,
            price: reference_gas_price,
            budget: 100_000_000,
        },
    );
    let signed_tx = Transaction::from_data_and_signer(tx_data.clone(), vec![&sender_key]);
    let expected_digest = *signed_tx.digest();

    let request = ExecuteTransactionRequestV3::new_v2(signed_tx);
    let response = executor
        .execute_transaction(request, None)
        .await
        .expect("execute_transaction should succeed");

    assert!(
        response.effects.effects.status().is_ok(),
        "transfer failed: {:?}",
        response.effects.effects.status(),
    );
    assert_eq!(*response.effects.effects.transaction_digest(), expected_digest);
}
