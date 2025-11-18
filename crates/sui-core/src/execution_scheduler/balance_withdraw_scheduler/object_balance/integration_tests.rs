// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Testing the integration of the object balance withdraw scheduler with the execution scheduler.

use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, crypto::get_account_key_pair, effects::TransactionEffectsAPI,
    executable_transaction::VerifiedExecutableTransaction, object::Object,
};

use crate::authority::{
    ExecutionEnv, shared_object_version_manager::AssignedVersions,
    test_authority_builder::TestAuthorityBuilder,
};

#[sim_test]
async fn test_object_withdraw_basic_flow() {
    let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    protocol_config.set_enable_accumulators_for_testing(true);
    protocol_config.create_root_accumulator_object_for_testing();

    let (sender, keypair) = get_account_key_pair();
    let gas_obj = Object::with_owner_for_testing(sender);

    let authority = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config)
        .with_starting_objects(std::slice::from_ref(&gas_obj))
        .build()
        .await;
    let epoch_store = authority.epoch_store_for_testing();

    let gas = gas_obj.compute_object_reference();
    let rgp = epoch_store.reference_gas_price();
    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .publish_examples("object_balance")
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);
    let (effects, ..) = authority
        .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
        .await
        .unwrap();
    assert!(effects.status().is_ok());
    let package_id = effects
        .created()
        .into_iter()
        .find(|(_, owner)| owner.is_immutable())
        .unwrap()
        .0
        .0;
    let gas = effects.gas_object().0;

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .move_call(package_id, "object_balance", "new", vec![])
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);
    let (effects, ..) = authority
        .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
        .await
        .unwrap();
    assert!(effects.status().is_ok());
    let vault_obj = effects.created().into_iter().next().unwrap().0;
    let gas = effects.gas_object().0;

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, vault_obj.0.into())])
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);

    let (effects, ..) = authority
        .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
        .await
        .unwrap();
    assert!(effects.status().is_ok());
    let gas = effects.gas_object().0;

    authority
        .settle_accumulator_for_testing(1, &[effects])
        .await;
    let accumulator_version = authority
        .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        .await
        .unwrap()
        .version();

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::object_fund(package_id, vault_obj),
            vec![(1000, sender)],
        )
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);

    let effects = authority
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new()
                .with_assigned_versions(AssignedVersions::new(vec![], Some(accumulator_version))),
            &epoch_store,
        )
        .await
        .unwrap()
        .0;
    assert!(effects.status().is_ok());
}

// TODO: More tests coming.
