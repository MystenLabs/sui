// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Testing the integration of the object funds withdraw scheduler with the execution scheduler.

use std::sync::Arc;

use fastcrypto::ed25519::Ed25519KeyPair;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, TypeTag,
    accumulator_root::AccumulatorValue,
    balance::Balance,
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::get_account_key_pair,
    effects::TransactionEffectsAPI,
    executable_transaction::VerifiedExecutableTransaction,
    execution::ExecutionOutput,
    execution_status::{ExecutionFailureStatus, ExecutionStatus},
    gas_coin::GAS,
    object::Object,
};

use crate::{
    accumulators::funds_read::AccountFundsRead,
    authority::{
        AuthorityState, ExecutionEnv, authority_per_epoch_store::AuthorityPerEpochStore,
        shared_object_version_manager::AssignedVersions,
        test_authority_builder::TestAuthorityBuilder,
    },
};

struct TestEnv {
    authority: Arc<AuthorityState>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    sender: SuiAddress,
    keypair: Ed25519KeyPair,
    gas_obj: ObjectID,
    package_id: ObjectID,
    vault_obj: ObjectID,
}

impl TestEnv {
    pub async fn new() -> Self {
        let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        protocol_config.enable_accumulators_for_testing();
        protocol_config.create_root_accumulator_object_for_testing();
        protocol_config.set_enable_object_funds_withdraw_for_testing(true);

        let (sender, keypair) = get_account_key_pair();
        let gas_obj = Object::with_owner_for_testing(sender);

        let authority = TestAuthorityBuilder::new()
            .with_protocol_config(protocol_config)
            .with_starting_objects(std::slice::from_ref(&gas_obj))
            .build()
            .await;
        let epoch_store = authority.epoch_store_for_testing().clone();

        let gas = gas_obj.compute_object_reference();
        let rgp = epoch_store.reference_gas_price();
        let tx = TestTransactionBuilder::new(sender, gas, rgp)
            .publish_examples("object_balance")
            .await
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
            .move_call(package_id, "object_balance", "new_owned", vec![])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);
        let (effects, ..) = authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
            .await
            .unwrap();
        assert!(effects.status().is_ok());
        let vault_obj = effects.created().into_iter().next().unwrap().0;
        Self {
            authority,
            epoch_store,
            sender,
            keypair,
            gas_obj: gas.0,
            package_id,
            vault_obj: vault_obj.0,
        }
    }

    pub async fn oref(&self, object_id: &ObjectID) -> ObjectRef {
        self.authority
            .get_object(object_id)
            .await
            .unwrap()
            .compute_object_reference()
    }

    pub fn rgp(&self) -> u64 {
        self.epoch_store.reference_gas_price()
    }

    pub async fn fund_address(&self, address: SuiAddress, amount: u64) {
        let gas = self.oref(&self.gas_obj).await;
        let tx = TestTransactionBuilder::new(self.sender, gas, self.rgp())
            .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, address)])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &self.keypair);

        let (effects, ..) = self
            .authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &self.epoch_store)
            .await
            .unwrap();
        assert!(effects.status().is_ok());

        self.authority
            .settle_accumulator_for_testing(&[effects])
            .await;
    }

    pub fn get_latest_balance(&self, type_tag: TypeTag) -> u128 {
        let account_id =
            AccumulatorValue::get_field_id(self.vault_obj.into(), &Balance::type_tag(type_tag))
                .unwrap();
        let balance_read = self.authority.get_child_object_resolver();
        balance_read.get_latest_account_amount(&account_id)
    }
}

#[tokio::test]
async fn test_object_withdraw_basic_flow() {
    let env = TestEnv::new().await;

    env.fund_address(env.vault_obj.into(), 1000).await;

    let gas = env.oref(&env.gas_obj).await;
    let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj).await),
            vec![(1000, env.sender)],
        )
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);

    let accumulator_version = env.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).await.1;
    let effects = env
        .authority
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new()
                .with_assigned_versions(AssignedVersions::new(vec![], Some(accumulator_version))),
            &env.epoch_store,
        )
        .await
        .unwrap()
        .0;
    assert!(effects.status().is_ok());
}

#[tokio::test]
async fn test_object_withdraw_fast_path_abort() {
    let env = TestEnv::new().await;

    env.fund_address(env.vault_obj.into(), 1000).await;

    let gas = env.oref(&env.gas_obj).await;
    let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj).await),
            vec![(1000, env.sender)],
        )
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);

    let output = env
        .authority
        // Fastpath execution
        .try_execute_immediately(&cert, ExecutionEnv::new(), &env.epoch_store)
        .await;
    assert!(matches!(output, ExecutionOutput::RetryLater));
}

#[tokio::test]
async fn test_object_withdraw_multiple_withdraws() {
    let env = TestEnv::new().await;

    env.fund_address(env.vault_obj.into(), 1000).await;

    let mut all_effects = Vec::new();
    // Withdraw from the same object account 3 times, each 300.
    // All withdraws should be sufficient.
    for _ in 0..3 {
        let gas = env.oref(&env.gas_obj).await;
        let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
            .transfer_sui_to_address_balance(
                FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj).await),
                vec![(300, env.sender)],
            )
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);

        let accumulator_version = env.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).await.1;
        let effects = env
            .authority
            // Fastpath execution
            .try_execute_immediately(
                &cert,
                ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(
                    vec![],
                    Some(accumulator_version),
                )),
                &env.epoch_store,
            )
            .await
            .unwrap()
            .0;
        assert!(effects.status().is_ok());
        all_effects.push(effects);
    }
    env.authority
        .settle_accumulator_for_testing(&all_effects)
        .await;

    assert_eq!(env.get_latest_balance(GAS::type_tag()), 1000 - 300 * 3);

    // Withdraw from the same object account 3 times, each 40.
    // The first 2 withdraws should be sufficient, the last one should be insufficient.
    // This test exercises the case where we have to track unsettled balance withdraws from the same consensus commit.
    for i in 0..3 {
        let gas = env.oref(&env.gas_obj).await;
        let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
            .transfer_sui_to_address_balance(
                FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj).await),
                vec![(40, env.sender)],
            )
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);
        let digest = *cert.digest();

        let accumulator_version = env.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).await.1;
        let output = env
            .authority
            // Fastpath execution
            .try_execute_immediately(
                &cert,
                ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(
                    vec![],
                    Some(accumulator_version),
                )),
                &env.epoch_store,
            )
            .await;
        let effects = if i < 2 {
            let effects = output.unwrap().0;
            assert!(effects.status().is_ok());
            effects
        } else {
            assert!(matches!(output, ExecutionOutput::RetryLater));
            let effects = env
                .authority
                .notify_read_effects("test", digest)
                .await
                .unwrap();
            assert!(matches!(
                effects.status(),
                ExecutionStatus::Failure {
                    error: ExecutionFailureStatus::InsufficientFundsForWithdraw,
                    ..
                }
            ));
            effects
        };
        all_effects.push(effects);
    }
    env.authority
        .settle_accumulator_for_testing(&all_effects)
        .await;

    assert_eq!(
        env.get_latest_balance(GAS::type_tag()),
        1000 - 300 * 3 - 40 * 2
    );
}

// FIXME: More tests coming.
