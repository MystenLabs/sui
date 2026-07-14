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
    effects::{TransactionEffects, TransactionEffectsAPI},
    executable_transaction::VerifiedExecutableTransaction,
    execution::ExecutionOutput,
    execution_status::{ExecutionErrorKind, ExecutionFailure, ExecutionStatus},
    gas_coin::GAS,
    object::Object,
};

use crate::authority::{
    AuthorityState, ExecutionEnv, authority_per_epoch_store::AuthorityPerEpochStore,
    shared_object_version_manager::AssignedVersions, test_authority_builder::TestAuthorityBuilder,
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
        Self::new_impl(true).await
    }

    /// Test environment with the legacy behavior of recording running max withdraws
    /// (instead of net withdraws) as unsettled.
    pub async fn new_with_legacy_unsettled_withdraws() -> Self {
        Self::new_impl(false).await
    }

    async fn new_impl(record_net_unsettled_object_withdraws: bool) -> Self {
        let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        protocol_config.set_record_net_unsettled_object_withdraws_for_testing(
            record_net_unsettled_object_withdraws,
        );
        // These tests exercise the post-execution `ObjectFundsChecker`. Disable the in-execution
        // check so it is not bypassed; the in-execution path has its own e2e/transactional coverage.
        protocol_config.set_check_object_funds_withdraw_in_execution_for_testing(false);

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
            .unwrap();
        assert!(effects.status().is_ok());
        let package_id = effects
            .created()
            .into_iter()
            .find(|(_, owner)| owner.is_immutable())
            .unwrap()
            .0
            .0;
        let gas = effects.gas_object().unwrap().0;

        let tx = TestTransactionBuilder::new(sender, gas, rgp)
            .move_call(package_id, "object_balance", "new_owned", vec![])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);
        let (effects, ..) = authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
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

    /// Creates another vault object account, in addition to the default one.
    pub async fn new_vault(&self) -> ObjectID {
        let gas = self.oref(&self.gas_obj);
        let tx = TestTransactionBuilder::new(self.sender, gas, self.rgp())
            .move_call(self.package_id, "object_balance", "new_owned", vec![])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &self.keypair);
        let (effects, ..) = self
            .authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &self.epoch_store)
            .unwrap();
        assert!(effects.status().is_ok());
        effects.created().into_iter().next().unwrap().0.0
    }

    pub fn oref(&self, object_id: &ObjectID) -> ObjectRef {
        self.authority
            .get_object(object_id)
            .unwrap()
            .compute_object_reference()
    }

    pub fn rgp(&self) -> u64 {
        self.epoch_store.reference_gas_price()
    }

    pub async fn fund_address(&self, address: SuiAddress, amount: u64) {
        let gas = self.oref(&self.gas_obj);
        let tx = TestTransactionBuilder::new(self.sender, gas, self.rgp())
            .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, address)])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &self.keypair);

        let (effects, ..) = self
            .authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &self.epoch_store)
            .unwrap();
        assert!(effects.status().is_ok());

        self.authority
            .settle_accumulator_for_testing(&[effects], None)
            .await;
    }

    pub fn get_latest_balance(&self, type_tag: TypeTag) -> u128 {
        let account_id =
            AccumulatorValue::get_field_id(self.vault_obj.into(), &Balance::type_tag(type_tag))
                .unwrap();
        let balance_read = self.authority.get_account_funds_read();
        balance_read.get_latest_account_amount(&account_id)
    }

    /// Builds a transaction that, for each `(amount, recipient)`, withdraws `amount` from
    /// the vault object account and deposits it to the recipient's address balance.
    pub fn vault_withdraw_tx(
        &self,
        transfers: &[(u64, SuiAddress)],
    ) -> VerifiedExecutableTransaction {
        let gas = self.oref(&self.gas_obj);
        let mut builder = TestTransactionBuilder::new(self.sender, gas, self.rgp());
        for (amount, recipient) in transfers {
            builder = builder.transfer_sui_to_address_balance(
                FundSource::object_fund_owned(self.package_id, self.oref(&self.vault_obj)),
                vec![(*amount, *recipient)],
            );
        }
        VerifiedExecutableTransaction::new_for_testing(builder.build(), &self.keypair)
    }

    fn execution_env(&self) -> ExecutionEnv {
        let accumulator_version = self.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).1;
        ExecutionEnv::new().with_assigned_versions(AssignedVersions::new_for_testing(
            vec![],
            Some(accumulator_version),
        ))
    }

    /// Executes at the current accumulator version, expecting success.
    pub async fn execute_ok(&self, cert: &VerifiedExecutableTransaction) -> TransactionEffects {
        let effects = self
            .authority
            .try_execute_immediately(cert, self.execution_env(), &self.epoch_store)
            .unwrap()
            .0;
        assert!(effects.status().is_ok());
        effects
    }

    /// Executes at the current accumulator version, expecting the object funds check to
    /// reject the transaction with InsufficientFundsForWithdraw.
    pub async fn execute_insufficient(
        &self,
        cert: &VerifiedExecutableTransaction,
    ) -> TransactionEffects {
        let digest = *cert.digest();
        let output =
            self.authority
                .try_execute_immediately(cert, self.execution_env(), &self.epoch_store);
        assert!(matches!(output, ExecutionOutput::RetryLater));
        let effects = self
            .authority
            .notify_read_effects_for_testing("test", digest)
            .await;
        assert!(matches!(
            effects.status(),
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionErrorKind::InsufficientFundsForWithdraw,
                ..
            })
        ));
        effects
    }
}

#[tokio::test]
async fn test_object_withdraw_basic_flow() {
    let env = TestEnv::new().await;

    env.fund_address(env.vault_obj.into(), 1000).await;

    let gas = env.oref(&env.gas_obj);
    let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj)),
            vec![(1000, env.sender)],
        )
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);

    let accumulator_version = env.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).1;
    let effects = env
        .authority
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new().with_assigned_versions(AssignedVersions::new_for_testing(
                vec![],
                Some(accumulator_version),
            )),
            &env.epoch_store,
        )
        .unwrap()
        .0;
    assert!(effects.status().is_ok());
}

#[tokio::test]
async fn test_object_withdraw_multiple_withdraws() {
    let env = TestEnv::new().await;

    env.fund_address(env.vault_obj.into(), 1000).await;

    let mut all_effects = Vec::new();
    // Withdraw from the same object account 3 times, each 300.
    // All withdraws should be sufficient.
    for _ in 0..3 {
        let gas = env.oref(&env.gas_obj);
        let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
            .transfer_sui_to_address_balance(
                FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj)),
                vec![(300, env.sender)],
            )
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);

        let accumulator_version = env.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).1;
        let effects = env
            .authority
            // Fastpath execution
            .try_execute_immediately(
                &cert,
                ExecutionEnv::new().with_assigned_versions(AssignedVersions::new_for_testing(
                    vec![],
                    Some(accumulator_version),
                )),
                &env.epoch_store,
            )
            .unwrap()
            .0;
        assert!(effects.status().is_ok());
        all_effects.push(effects);
    }
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;

    assert_eq!(env.get_latest_balance(GAS::type_tag()), 1000 - 300 * 3);

    all_effects.clear();

    // Withdraw from the same object account 3 times, each 40.
    // The first 2 withdraws should be sufficient, the last one should be insufficient.
    // This test exercises the case where we have to track unsettled balance withdraws from the same consensus commit.
    for i in 0..3 {
        let gas = env.oref(&env.gas_obj);
        let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
            .transfer_sui_to_address_balance(
                FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj)),
                vec![(40, env.sender)],
            )
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);
        let digest = *cert.digest();

        let accumulator_version = env.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).1;
        let output = env
            .authority
            // Fastpath execution
            .try_execute_immediately(
                &cert,
                ExecutionEnv::new().with_assigned_versions(AssignedVersions::new_for_testing(
                    vec![],
                    Some(accumulator_version),
                )),
                &env.epoch_store,
            );
        let effects = if i < 2 {
            let effects = output.unwrap().0;
            assert!(effects.status().is_ok());
            effects
        } else {
            assert!(matches!(output, ExecutionOutput::RetryLater));
            let effects = env
                .authority
                .notify_read_effects_for_testing("test", digest)
                .await;
            assert!(matches!(
                effects.status(),
                ExecutionStatus::Failure(ExecutionFailure {
                    error: ExecutionErrorKind::InsufficientFundsForWithdraw,
                    ..
                })
            ));
            effects
        };
        all_effects.push(effects);
    }
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;

    assert_eq!(
        env.get_latest_balance(GAS::type_tag()),
        1000 - 300 * 3 - 40 * 2
    );
}

#[tokio::test]
async fn test_object_withdraw_and_deposit_same_transaction() {
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new().await;
    let vault: SuiAddress = env.vault_obj.into();
    env.fund_address(vault, 2).await;
    let mut all_effects = Vec::new();

    // Withdraw 3 and deposit 3 back to the same object account. Even though this nets
    // out to 0, the running max withdraw of 3 exceeds the balance of 2, so it fails.
    let tx = env.vault_withdraw_tx(&[(3, vault)]);
    all_effects.push(env.execute_insufficient(&tx).await);

    // Withdraw 2 and deposit 2 back, twice within the same transaction. The running
    // net withdraw never exceeds 2, so this succeeds.
    let tx = env.vault_withdraw_tx(&[(2, vault), (2, vault)]);
    all_effects.push(env.execute_ok(&tx).await);

    // The previous transaction's withdraws netted out to 0, so the full balance of 2
    // is still available at the same version.
    let tx = env.vault_withdraw_tx(&[(1, vault)]);
    all_effects.push(env.execute_ok(&tx).await);

    // Withdraw the full balance of 2 without depositing back.
    let tx = env.vault_withdraw_tx(&[(2, env.sender)]);
    all_effects.push(env.execute_ok(&tx).await);

    // The balance is now fully reserved; even a withdraw of 1 that deposits back
    // must fail.
    let tx = env.vault_withdraw_tx(&[(1, vault)]);
    all_effects.push(env.execute_insufficient(&tx).await);

    // Settlement applies the net amounts: only the full-balance withdraw of 2
    // actually deducted funds.
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;
    assert_eq!(env.get_latest_balance(GAS::type_tag()), 0);
}

#[tokio::test]
async fn test_object_net_deposit_same_transaction() {
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new().await;
    let vault: SuiAddress = env.vault_obj.into();
    env.fund_address(vault, 2).await;
    let mut all_effects = Vec::new();

    // In one transaction, withdraw 2 from the vault and deposit it back, plus deposit
    // 3 more from a coin. The vault's folded accumulator event is a net deposit, which
    // is recorded as 0 unsettled withdraw, while the withdraw of 2 is still checked
    // against the running max.
    let gas = env.oref(&env.gas_obj);
    let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj)),
            vec![(2, vault)],
        )
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(3, vault)])
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);
    all_effects.push(env.execute_ok(&cert).await);

    // The net deposit consumed no unsettled balance: the full balance of 2 is still
    // available at the same version.
    let tx = env.vault_withdraw_tx(&[(2, env.sender)]);
    all_effects.push(env.execute_ok(&tx).await);

    // But the unsettled deposit of 3 is not credited before settlement.
    let tx = env.vault_withdraw_tx(&[(1, env.sender)]);
    all_effects.push(env.execute_insufficient(&tx).await);

    // After settlement the net deposit materializes: 2 + 3 - 2 = 3.
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;
    assert_eq!(env.get_latest_balance(GAS::type_tag()), 3);
}

#[tokio::test]
async fn test_object_zero_amount_withdraw() {
    // A zero-amount object-fund withdraw emits a single Split(0) accumulator event.
    // It survives effects folding as a Split (the fold's Merge tie-break only applies
    // to accounts with multiple writes), but creates no running max entry. It must be
    // skipped when recording unsettled withdraws instead of tripping the recording
    // invariant check.
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new().await;
    let vault: SuiAddress = env.vault_obj.into();
    let zero_vault = env.new_vault().await;
    env.fund_address(vault, 2).await;

    // One transaction: withdraw 0 from zero_vault and 2 from the funded vault. The
    // positive withdraw makes the running max map non-empty, so the zero withdraw
    // reaches the unsettled recording path.
    let gas = env.oref(&env.gas_obj);
    let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&zero_vault)),
            vec![(0, env.sender)],
        )
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj)),
            vec![(2, env.sender)],
        )
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);
    let effects = env.execute_ok(&cert).await;

    env.authority
        .settle_accumulator_for_testing(&[effects], None)
        .await;
    assert_eq!(env.get_latest_balance(GAS::type_tag()), 0);
}

#[tokio::test]
async fn test_object_withdraw_and_deposit_same_transaction_legacy() {
    // With record_net_unsettled_object_withdraws disabled, the running max withdraw
    // (rather than the net withdraw) is recorded as unsettled, so a transaction
    // whose withdraws net out to 0 still blocks subsequent withdraws at the same
    // accumulator version.
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new_with_legacy_unsettled_withdraws().await;
    let vault: SuiAddress = env.vault_obj.into();
    env.fund_address(vault, 2).await;

    // Withdraw 2 and deposit 2 back to the same object account.
    let tx = env.vault_withdraw_tx(&[(2, vault)]);
    env.execute_ok(&tx).await;

    // Even though the previous transaction netted out to 0, its running max withdraw
    // of 2 is recorded as unsettled, so no balance remains available.
    let tx = env.vault_withdraw_tx(&[(1, vault)]);
    env.execute_insufficient(&tx).await;
}
