// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID, SUI_GUARDRAILS_REGISTRY_OBJECT_ID, TypeTag,
    base_types::{ObjectRef, SuiAddress},
    collection_types::VecMap,
    effects::{TransactionEffects, TransactionEffectsAPI},
    execution_status::{
        ExecutionFailure, ExecutionFailureStatus, ExecutionStatus, GuardrailsViolationKind,
    },
    gas_coin::GAS,
    guardrails::{Policy, policy_object_id},
    transaction::{
        CallArg, GasData, ObjectArg, SharedObjectMutability, Transaction, TransactionData,
        TransactionDataAPI,
    },
};
use test_cluster::{TestCluster, TestClusterBuilder};

const MIST_PER_SUI: u64 = 1_000_000_000;
/// Per-epoch SUI limit of the test policy.
const SUI_LIMIT: u64 = MIST_PER_SUI;
const GAS_BUDGET: u64 = 50_000_000;
const RECOVERY_DELAY_EPOCHS: u64 = 14;

struct Env {
    cluster: TestCluster,
    owner: SuiAddress,
    guardian: SuiAddress,
    /// Receives what the owner sends.
    stranger: SuiAddress,
    /// Pays gas for others without being anyone's guardian.
    sponsor: SuiAddress,
    rgp: u64,
    _guard: sui_protocol_config::OverrideGuard,
}

fn pure<T: serde::Serialize>(value: &T) -> CallArg {
    CallArg::Pure(bcs::to_bytes(value).unwrap())
}

fn sui() -> TypeTag {
    GAS::type_tag()
}

/// Returns the SUI entry of a policy's limits or spend, keyed as `std::type_name` prints the type.
fn sui_entry(map: &VecMap<String, u64>) -> u64 {
    map.get(&sui().to_canonical_string(/* with_prefix */ false))
        .copied()
        .unwrap_or(0)
}

/// Asserts that execution failed the transaction because `account`'s budget of `coin_type` had
/// no room.
fn assert_over_budget(effects: &TransactionEffects, account: SuiAddress, coin_type: &TypeTag) {
    let ExecutionStatus::Failure(ExecutionFailure {
        error:
            ExecutionFailureStatus::GuardrailsViolation {
                account: violated,
                kind:
                    GuardrailsViolationKind::BudgetExhausted {
                        coin_type: violated_type,
                    },
            },
        ..
    }) = effects.status()
    else {
        panic!(
            "expected a guardrails violation, got {:?}",
            effects.status()
        );
    };
    assert_eq!(*violated, account);
    assert_eq!(**violated_type, *coin_type);
}

/// Asserts that the transaction aborted in `sui::guardrails::<function>`.
fn assert_aborted_in(effects: &TransactionEffects, function: &str) {
    let ExecutionStatus::Failure(ExecutionFailure {
        error: ExecutionFailureStatus::MoveAbort(location, _),
        ..
    }) = effects.status()
    else {
        panic!("expected an abort, got {:?}", effects.status());
    };
    assert_eq!(location.module.name().as_str(), "guardrails");
    assert_eq!(location.function_name.as_deref(), Some(function));
}

impl Env {
    async fn new() -> Self {
        // The flag is on for devnet only; force it so the test also runs under other chain
        // overrides.
        let guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_enable_guardrails_for_testing(true);
            config
        });
        let cluster = TestClusterBuilder::new().build().await;
        let addresses = cluster.get_addresses();
        let rgp = cluster.get_reference_gas_price().await;
        Self {
            cluster,
            owner: addresses[0],
            guardian: addresses[1],
            stranger: addresses[2],
            sponsor: addresses[3],
            rgp,
            _guard: guard,
        }
    }

    async fn gas_objects(&self, address: SuiAddress) -> Vec<ObjectRef> {
        self.cluster
            .wallet
            .get_gas_objects_owned_by_address(address, None)
            .await
            .unwrap()
    }

    async fn builder(&self, sender: SuiAddress) -> TestTransactionBuilder {
        let gas = self.gas_objects(sender).await[0];
        TestTransactionBuilder::new(sender, gas, self.rgp).with_gas_budget(GAS_BUDGET)
    }

    async fn execute(&self, tx_data: TransactionData) -> TransactionEffects {
        let tx = self.cluster.wallet.sign_transaction(&tx_data).await;
        self.cluster
            .execute_transaction_return_raw_effects(tx)
            .await
            .unwrap()
            .0
    }

    async fn execute_ok(&self, tx_data: TransactionData) -> TransactionEffects {
        let effects = self.execute(tx_data).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        effects
    }

    /// Asserts that execution fails `tx_data` because the owner's SUI budget has no room.
    async fn execute_over_budget(&self, tx_data: TransactionData) {
        let effects = self.execute(tx_data).await;
        assert_over_budget(&effects, self.owner, &sui());
    }

    /// Asserts that validators refuse to sign `tx_data` because of a guardrails policy.
    async fn execute_rejected(&self, tx: Transaction) {
        let error = self
            .cluster
            .execute_transaction_return_raw_effects(tx)
            .await
            .expect_err("transaction should be rejected at signing")
            .to_string();
        assert!(
            error.contains("guardrails"),
            "unexpected rejection: {error}"
        );
    }

    /// `tx_data` with `sponsor` paying the gas, signed by its sender and the sponsor.
    async fn sponsored(&self, tx_data: TransactionData, sponsor: SuiAddress) -> Transaction {
        let gas_data = GasData {
            payment: vec![self.gas_objects(sponsor).await[0]],
            owner: sponsor,
            price: self.rgp,
            budget: GAS_BUDGET,
        };
        let sender = tx_data.sender();
        let tx_data = TransactionData::new_with_gas_data(tx_data.into_kind(), sender, gas_data);
        let keystore = &self.cluster.wallet.config.keystore;
        let mut signatures = vec![];
        for signer in [sender, sponsor] {
            signatures.push(
                keystore
                    .sign_secure(&signer, &tx_data, Intent::sui_transaction())
                    .await
                    .unwrap(),
            );
        }
        Transaction::from_data(tx_data, signatures)
    }

    async fn execute_sponsored(
        &self,
        tx_data: TransactionData,
        sponsor: SuiAddress,
    ) -> TransactionEffects {
        let tx = self.sponsored(tx_data, sponsor).await;
        self.cluster
            .execute_transaction_return_raw_effects(tx)
            .await
            .unwrap()
            .0
    }

    async fn execute_with_guardian(&self, tx_data: TransactionData) -> TransactionEffects {
        let effects = self.execute_sponsored(tx_data, self.guardian).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        effects
    }

    async fn policy(&self, owner: SuiAddress) -> Policy {
        let object = self
            .cluster
            .get_object_from_fullnode_store(&policy_object_id(owner))
            .await
            .expect("policy exists");
        Policy::from_object(&object).expect("object is a policy")
    }

    /// What the owner's policy has recorded for SUI in the current epoch.
    async fn sui_spent(&self) -> u64 {
        let epoch = self
            .cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().epoch_store_for_testing().epoch());
        let policy = self.policy(self.owner).await;
        if policy.spent_epoch != epoch {
            return 0;
        }
        sui_entry(&policy.spent)
    }

    /// An owner transaction calling `guardrails::set_limit<coin_type>(policy, limit, ctx)`.
    async fn set_limit_tx(&self, coin_type: TypeTag, limit: u64) -> TransactionData {
        let policy = self.policy_arg(self.owner).await;
        self.builder(self.owner)
            .await
            .move_call_with_type_args(
                SUI_FRAMEWORK_PACKAGE_ID,
                "guardrails",
                "set_limit",
                vec![coin_type],
                vec![policy, pure(&limit)],
            )
            .build()
    }

    async fn policy_arg(&self, owner: SuiAddress) -> CallArg {
        let id = policy_object_id(owner);
        let policy = self
            .cluster
            .get_object_from_fullnode_store(&id)
            .await
            .expect("policy exists");
        CallArg::Object(ObjectArg::SharedObject {
            id,
            initial_shared_version: policy.owner().start_version().unwrap(),
            mutability: SharedObjectMutability::Mutable,
        })
    }

    /// An owner transaction calling `guardrails::<function>(policy, args.., ctx)`.
    async fn policy_call_tx(&self, function: &'static str, args: Vec<CallArg>) -> TransactionData {
        let mut call_args = vec![self.policy_arg(self.owner).await];
        call_args.extend(args);
        self.builder(self.owner)
            .await
            .move_call(SUI_FRAMEWORK_PACKAGE_ID, "guardrails", function, call_args)
            .build()
    }

    /// Creates the owner's policy with a SUI limit. The policy is pending until the next epoch.
    async fn enable_policy(&self) {
        self.enable_policy_of(self.owner, self.guardian).await;
    }

    async fn enable_policy_of(&self, owner: SuiAddress, guardian: SuiAddress) {
        let registry = self
            .cluster
            .get_object_from_fullnode_store(&SUI_GUARDRAILS_REGISTRY_OBJECT_ID)
            .await
            .expect("registry is created at genesis");
        let registry = CallArg::Object(ObjectArg::SharedObject {
            id: SUI_GUARDRAILS_REGISTRY_OBJECT_ID,
            initial_shared_version: registry.owner().start_version().unwrap(),
            mutability: SharedObjectMutability::Mutable,
        });
        let tx = self
            .builder(owner)
            .await
            .move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                "guardrails",
                "enable",
                vec![
                    registry,
                    pure(&guardian),
                    pure(&SUI_LIMIT),
                    pure(&RECOVERY_DELAY_EPOCHS),
                ],
            )
            .build();
        self.execute_ok(tx).await;
    }

    /// Sends `amount` out of a coin of the owner that is not the gas coin, so that the SUI is the
    /// owner's when someone else pays the gas.
    async fn transfer_owner_coin_tx(&self, amount: u64) -> TransactionData {
        let coin = self.gas_objects(self.owner).await[1];
        self.builder(self.owner)
            .await
            .transfer_sui_to_address_balance(FundSource::coin(coin), vec![(amount, self.stranger)])
            .build()
    }

    async fn transfer_sui_tx(&self, amount: u64, to: SuiAddress) -> TransactionData {
        self.builder(self.owner)
            .await
            .transfer_sui(Some(amount), to)
            .build()
    }
}

/// A pending policy is not enforced. Once active, transfers count against the epoch's SUI limit
/// and gas does not, a transfer that does not fit fails and records nothing, and the budget
/// starts over in the next epoch.
#[sim_test]
async fn test_budget_spans_the_epoch() {
    let env = Env::new().await;
    env.enable_policy().await;

    // Pending: more than the limit leaves the account.
    let tx = env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await;
    env.execute_ok(tx).await;
    assert_eq!(env.sui_spent().await, 0);

    env.cluster.trigger_reconfiguration().await;

    let amount = SUI_LIMIT * 2 / 5;
    let tx = env.transfer_sui_tx(amount, env.stranger).await;
    let effects = env.execute_ok(tx).await;
    let policy_id = policy_object_id(env.owner);
    assert!(
        effects
            .mutated()
            .iter()
            .any(|(object, _)| object.0 == policy_id),
        "the policy is written by a transaction that never declared it"
    );
    assert_eq!(env.sui_spent().await, amount);

    let tx = env.transfer_sui_tx(amount, env.stranger).await;
    env.execute_ok(tx).await;
    assert_eq!(env.sui_spent().await, 2 * amount);

    let tx = env.transfer_sui_tx(amount, env.stranger).await;
    env.execute_over_budget(tx).await;
    assert_eq!(env.sui_spent().await, 2 * amount);

    env.cluster.trigger_reconfiguration().await;
    let tx = env.transfer_sui_tx(amount, env.stranger).await;
    env.execute_ok(tx).await;
    assert_eq!(env.sui_spent().await, amount);
}

/// What the policy counts is the net decrease of the owner's coins and address balance. Moving
/// SUI between the two or splitting a coin is not outflow. SUI that goes to someone else, from
/// a coin or from the address balance, is. So is SUI that goes into another object the owner
/// keeps, such as a stake.
#[sim_test]
async fn test_what_counts_as_outflow() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    let own_transfer = |source: FundSource, gas: ObjectRef, amount: u64, to: SuiAddress| {
        TestTransactionBuilder::new(env.owner, gas, env.rgp)
            .with_gas_budget(GAS_BUDGET)
            .transfer_sui_to_address_balance(source, vec![(amount, to)])
            .build()
    };

    // Far more than the limit, from a coin into the owner's own address balance.
    let gas = env.gas_objects(env.owner).await[0];
    env.execute_ok(own_transfer(
        FundSource::coin(gas),
        gas,
        3 * SUI_LIMIT,
        env.owner,
    ))
    .await;
    assert_eq!(env.sui_spent().await, 0);

    // Splitting a coin and keeping the pieces.
    let coin = env.gas_objects(env.owner).await[1];
    let tx = env
        .builder(env.owner)
        .await
        .split_coin(coin, vec![3 * SUI_LIMIT])
        .build();
    env.execute_ok(tx).await;
    assert_eq!(env.sui_spent().await, 0);

    // From the owner's address balance to someone else.
    let amount = SUI_LIMIT / 2;
    let gas = env.gas_objects(env.owner).await[0];
    env.execute_ok(own_transfer(
        FundSource::address_fund(),
        gas,
        amount,
        env.stranger,
    ))
    .await;
    assert_eq!(env.sui_spent().await, amount);

    // From a coin to someone else's address balance. This uses up the limit exactly.
    let gas = env.gas_objects(env.owner).await[0];
    env.execute_ok(own_transfer(
        FundSource::coin(gas),
        gas,
        amount,
        env.stranger,
    ))
    .await;
    assert_eq!(env.sui_spent().await, SUI_LIMIT);

    let gas = env.gas_objects(env.owner).await[0];
    let tx = own_transfer(FundSource::coin(gas), gas, 1, env.stranger);
    env.execute_over_budget(tx).await;

    // A stake stays with the owner, but not as a coin.
    env.cluster.trigger_reconfiguration().await;
    let validator = env.cluster.swarm.config().validator_configs()[0].sui_address();
    let stake_coin = env.gas_objects(env.owner).await[1];
    let tx = env
        .builder(env.owner)
        .await
        .call_staking(stake_coin, validator)
        .build();
    env.execute_over_budget(tx).await;
}

/// A transaction the guardian sponsors is exempt and leaves the recorded spend alone. Changing
/// an active policy takes the guardian; the key alone can only request a delayed disable, which
/// the guardian can undo.
#[sim_test]
async fn test_guardian_sponsorship() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    let tx = env.transfer_owner_coin_tx(5 * SUI_LIMIT).await;
    env.execute_with_guardian(tx).await;
    assert_eq!(env.sui_spent().await, 0);

    // The same transfer sponsored by anyone else is checked against the owner's policy.
    let tx = env.transfer_owner_coin_tx(5 * SUI_LIMIT).await;
    let effects = env.execute_sponsored(tx, env.sponsor).await;
    assert_over_budget(&effects, env.owner, &sui());
    assert_eq!(env.sui_spent().await, 0);

    // The key alone cannot change the active policy.
    let effects = env
        .execute(env.set_limit_tx(sui(), 10 * SUI_LIMIT).await)
        .await;
    assert_aborted_in(&effects, "assert_authorized");
    let effects = env
        .execute(env.policy_call_tx("disable", vec![]).await)
        .await;
    assert_aborted_in(&effects, "assert_authorized");

    // With the guardian the limit changes at once.
    env.execute_with_guardian(env.set_limit_tx(sui(), 10 * SUI_LIMIT).await)
        .await;
    assert_eq!(
        sui_entry(&env.policy(env.owner).await.limits),
        10 * SUI_LIMIT
    );
    let tx = env.transfer_sui_tx(5 * SUI_LIMIT, env.stranger).await;
    env.execute_ok(tx).await;

    // The key alone can start the recovery delay, and only the guardian can stop it. The policy
    // holds while the delay runs.
    let epoch = env.policy(env.owner).await.spent_epoch;
    env.execute_ok(env.policy_call_tx("request_disable", vec![]).await)
        .await;
    assert_eq!(
        env.policy(env.owner).await.active_until,
        epoch + RECOVERY_DELAY_EPOCHS
    );
    let tx = env.transfer_sui_tx(6 * SUI_LIMIT, env.stranger).await;
    env.execute_over_budget(tx).await;
    let effects = env
        .execute(env.policy_call_tx("cancel_disable", vec![]).await)
        .await;
    assert_aborted_in(&effects, "cancel_disable");
    env.execute_with_guardian(env.policy_call_tx("cancel_disable", vec![]).await)
        .await;
    assert_eq!(env.policy(env.owner).await.active_until, u64::MAX);

    // The guardian ends the policy at the next epoch, and it holds until then.
    env.execute_with_guardian(env.policy_call_tx("disable", vec![]).await)
        .await;
    let tx = env.transfer_sui_tx(6 * SUI_LIMIT, env.stranger).await;
    env.execute_over_budget(tx).await;
    env.cluster.trigger_reconfiguration().await;
    let tx = env.transfer_sui_tx(20 * SUI_LIMIT, env.stranger).await;
    env.execute_ok(tx).await;
}

/// A coin type other than SUI cannot leave the account until the policy has a limit for it, and
/// then only up to that limit.
#[sim_test]
async fn test_limit_of_another_coin_type() {
    let env = Env::new().await;
    // The package mints 10000 of each of its coin types into the owner's address balance.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "data", "coins"]);
    let publish = env
        .builder(env.owner)
        .await
        .with_gas_budget(100 * GAS_BUDGET)
        .publish_async(path)
        .await
        .build();
    let package_id = env
        .cluster
        .sign_and_execute_transaction(&publish)
        .await
        .get_new_package_obj()
        .unwrap()
        .0;
    let coin_a: TypeTag = format!("{package_id}::coin_a::COIN_A").parse().unwrap();
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    let send_coin_a = |gas: ObjectRef, amount: u64| {
        TestTransactionBuilder::new(env.owner, gas, env.rgp)
            .with_gas_budget(GAS_BUDGET)
            .transfer_funds_to_address_balance(
                FundSource::address_fund(),
                vec![(amount, env.stranger)],
                coin_a.clone(),
            )
            .build()
    };
    let gas = env.gas_objects(env.owner).await[0];
    let effects = env.execute(send_coin_a(gas, 1)).await;
    assert_over_budget(&effects, env.owner, &coin_a);

    env.execute_with_guardian(env.set_limit_tx(coin_a.clone(), 1000).await)
        .await;
    let gas = env.gas_objects(env.owner).await[0];
    env.execute_ok(send_coin_a(gas, 1000)).await;
    let gas = env.gas_objects(env.owner).await[0];
    let effects = env.execute(send_coin_a(gas, 1)).await;
    assert_over_budget(&effects, env.owner, &coin_a);
    assert_eq!(env.sui_spent().await, 0);
}

/// The policy binds only what the owner sends, and a transaction reaches its gas owner's SUI
/// beyond gas only through the gas coin argument. So the owner can pay the gas of someone else's
/// transaction that does not use the gas coin, and validators refuse to sign one that does.
#[sim_test]
async fn test_owner_as_gas_owner() {
    let env = Env::new().await;
    env.enable_policy().await;

    let takes_gas_coin = |gas: ObjectRef| {
        TestTransactionBuilder::new(env.stranger, gas, env.rgp)
            .with_gas_budget(GAS_BUDGET)
            .transfer_sui(Some(SUI_LIMIT), env.stranger)
            .build()
    };

    // Pending: the stranger takes SUI out of the gas coin the owner pays with.
    let gas = env.gas_objects(env.stranger).await[0];
    let effects = env.execute_sponsored(takes_gas_coin(gas), env.owner).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());

    env.cluster.trigger_reconfiguration().await;
    let gas = env.gas_objects(env.stranger).await[0];
    let tx = env.sponsored(takes_gas_coin(gas), env.owner).await;
    env.execute_rejected(tx).await;

    // The stranger sends from a coin of its own, and the owner only pays the gas.
    let [gas, coin] = env.gas_objects(env.stranger).await[..2] else {
        unreachable!()
    };
    let tx = TestTransactionBuilder::new(env.stranger, gas, env.rgp)
        .with_gas_budget(GAS_BUDGET)
        .transfer_sui_to_address_balance(FundSource::coin(coin), vec![(SUI_LIMIT, env.owner)])
        .build();
    let effects = env.execute_sponsored(tx, env.owner).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_eq!(env.sui_spent().await, 0);
}

/// A guardian with an active policy of its own can still approve the owner's transactions, so a
/// stolen owner key cannot outwait the guardian with `request_disable`.
#[sim_test]
async fn test_guardian_with_its_own_policy() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.enable_policy_of(env.guardian, env.sponsor).await;
    env.cluster.trigger_reconfiguration().await;

    env.execute_ok(env.policy_call_tx("request_disable", vec![]).await)
        .await;
    env.execute_with_guardian(env.policy_call_tx("cancel_disable", vec![]).await)
        .await;
    assert_eq!(env.policy(env.owner).await.active_until, u64::MAX);

    let tx = env.transfer_owner_coin_tx(5 * SUI_LIMIT).await;
    env.execute_with_guardian(tx).await;
}

/// A policy cancelled before it becomes active never restricts the account.
#[sim_test]
async fn test_cancel_before_activation() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.execute_ok(env.policy_call_tx("cancel", vec![]).await)
        .await;
    env.cluster.trigger_reconfiguration().await;
    let tx = env.transfer_sui_tx(5 * SUI_LIMIT, env.stranger).await;
    let effects = env.execute_ok(tx).await;
    let policy_id = policy_object_id(env.owner);
    assert!(
        !effects
            .mutated()
            .iter()
            .any(|(object, _)| object.0 == policy_id),
        "an inactive policy is not an input"
    );
}

/// Every transaction of the owner depends on the policy object, so validators refuse to sign a
/// transaction of anyone else that declares it.
#[sim_test]
async fn test_only_owner_declares_policy() {
    let env = Env::new().await;
    env.enable_policy().await;
    let tx = env
        .builder(env.stranger)
        .await
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "guardrails",
            "request_disable",
            vec![env.policy_arg(env.owner).await],
        )
        .build();
    let tx = env.cluster.wallet.sign_transaction(&tx).await;
    env.execute_rejected(tx).await;
}
