// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use proptest::arbitrary::*;
use sui_core::authority::AuthorityState;
use sui_core::test_utils::send_and_confirm_transaction;
use sui_types::base_types::dbg_addr;
use sui_types::crypto::KeypairTraits;
use sui_types::messages::TransactionData;
use sui_types::messages::TransactionKind;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;
use tokio::runtime::Runtime;
use tracing::debug;
use transaction_fuzzer::run_proptest;
use transaction_fuzzer::GasDataGenConfig;
use transaction_fuzzer::GasDataWithObjects;

/// Send transfer sui txn with provided random gas data and gas objects to an authority.
async fn test_with_random_gas_data(
    gas_data_test: GasDataWithObjects,
    authority_state: Arc<AuthorityState>,
) {
    let gas_data = gas_data_test.gas_data;
    let objects = gas_data_test.objects;
    let sender = gas_data_test.sender_key.public().into();

    // Insert the random gas objects into genesis.
    authority_state.insert_genesis_objects(&objects).await;
        builder.finish()
    };
    let kind = TransactionKind::ProgrammableTransaction(pt);
    let tx_data = TransactionData::new_with_gas_data(kind, sender, gas_data);
    let tx = to_sender_signed_transaction(tx_data, &gas_data_test.sender_key);

    let result = send_and_confirm_transaction(&authority_state, None, tx).await;
    debug!("result: {:?}", result);
}

#[test]
fn test_gas_data_owned_or_immut() {
    let strategy = any_with::<GasDataWithObjects>(GasDataGenConfig::owned_by_sender_or_immut());
    run_proptest(2000, strategy, |gas_data_test, authority_state| {
        let rt = Runtime::new().unwrap();
        let future = test_with_random_gas_data(gas_data_test, authority_state);
        rt.block_on(future);
    });
}

#[test]
fn test_gas_data_any_owner() {
    let strategy = any_with::<GasDataWithObjects>(GasDataGenConfig::any_owner());
    run_proptest(2000, strategy, |gas_data_test, authority_state| {
        let rt = Runtime::new().unwrap();
        let future = test_with_random_gas_data(gas_data_test, authority_state);
        rt.block_on(future);
    });
}

// proptest! {
//     // Stops after 20 test cases.
//     #![proptest_config(ProptestConfig::with_cases(20))]
//     #[test]
//     #[cfg_attr(msim, ignore)]
//     fn test_gas_data(gas_data_test in any_with::<GasDataWithObjects>(GasDataGenConfig::owned_by_sender_or_immut())) {
//         let rt = Runtime::new().unwrap();

//         let future = test_with_random_gas_data(gas_data_test);
//         rt.block_on(future);

//     }

//     #[test]
//     #[cfg_attr(msim, ignore)]
//     fn test_gas_data_any_owner(gas_data_test in any_with::<GasDataWithObjects>(GasDataGenConfig::any_owner())) {
//         let rt = Runtime::new().unwrap();

//         let future = test_with_random_gas_data(gas_data_test);
//         rt.block_on(future);

//     }
// }
