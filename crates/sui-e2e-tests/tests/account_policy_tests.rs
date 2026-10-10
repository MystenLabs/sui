// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use move_core_types::ident_str;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{FullObjectRef, ObjectID, ObjectRef, SuiAddress},
    effects::{TransactionEffects, TransactionEffectsAPI},
    execution_status::{
        AccountPolicyViolationKind, ExecutionFailure, ExecutionFailureStatus, ExecutionStatus,
    },
    object::Owner,
    transaction::{CallArg, ObjectArg, SharedObjectMutability, Transaction, TransactionData},
};
use test_cluster::{TestCluster, TestClusterBuilder};

const MIST_PER_SUI: u64 = 1_000_000_000;
/// Per-epoch SUI budget of the test policy.
const SUI_LIMIT: u64 = MIST_PER_SUI;
/// Gas budget of every test transaction; the policy counts it against `SUI_LIMIT`.
const GAS_BUDGET: u64 = 50_000_000;
const SUI_TYPE: &str = "0x2::sui::SUI";

struct Env {
    cluster: TestCluster,
    owner: SuiAddress,
    guardian: SuiAddress,
    /// Listed in the policy as a recipient.
    recipient: SuiAddress,
    /// Not listed anywhere.
    stranger: SuiAddress,
    rgp: u64,
    gas_cap: u64,
    registry: CallArg,
    _guard: sui_protocol_config::OverrideGuard,
}

/// The test package published by the owner before the policy is enabled.
struct TestPackage {
    id: ObjectID,
    treasury_cap: ObjectID,
}

impl TestPackage {
    fn coin_type(&self) -> String {
        format!("{}::policy_coin::POLICY_COIN", self.id)
    }

    fn item_type(&self) -> String {
        format!("{}::items::Item", self.id)
    }

    fn wrapper_type(&self) -> String {
        format!("{}::items::Wrapper", self.id)
    }
}

impl Env {
    async fn new() -> Self {
        // The flag is devnet-only; force it on so the test also runs under the mainnet chain
        // override.
        let guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_enable_account_policy_for_testing(true);
            config
        });
        let cluster = TestClusterBuilder::new().build().await;
        let addresses = cluster.get_addresses();
        let rgp = cluster.get_reference_gas_price().await;
        let registry = cluster
            .get_object_from_fullnode_store(&SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID)
            .await
            .expect("registry is created at genesis");
        let registry = CallArg::Object(ObjectArg::SharedObject {
            id: SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID,
            initial_shared_version: registry.owner().start_version().unwrap(),
            mutability: SharedObjectMutability::Mutable,
        });
        Self {
            cluster,
            owner: addresses[0],
            guardian: addresses[1],
            recipient: addresses[2],
            stranger: addresses[3],
            rgp,
            gas_cap: 2 * GAS_BUDGET,
            registry,
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

    /// Asserts that validators refuse to sign `tx_data` because of the sender's policy.
    async fn execute_rejected(&self, tx_data: TransactionData) {
        let tx = self.cluster.wallet.sign_transaction(&tx_data).await;
        let error = self
            .cluster
            .execute_transaction_return_raw_effects(tx)
            .await
            .expect_err("transaction should be rejected at admission")
            .to_string();
        assert!(
            error.contains("account policy"),
            "unexpected rejection: {error}"
        );
    }

    async fn execute_ok(&self, tx_data: TransactionData) -> TransactionEffects {
        let effects = self.execute(tx_data).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        effects
    }

    /// Executes `tx_data` signed by the owner and co-signed by `co_signer`.
    async fn execute_co_signed(
        &self,
        tx_data: TransactionData,
        co_signer: SuiAddress,
    ) -> TransactionEffects {
        let keystore = &self.cluster.wallet.config.keystore;
        let owner_sig = keystore
            .sign_secure(&self.owner, &tx_data, Intent::sui_transaction())
            .await
            .unwrap();
        let co_sig = keystore
            .sign_secure(&co_signer, &tx_data, Intent::sui_transaction())
            .await
            .unwrap();
        let tx = Transaction::from_data(tx_data, vec![owner_sig, co_sig]);
        self.cluster
            .execute_transaction_return_raw_effects(tx)
            .await
            .unwrap()
            .0
    }

    async fn execute_with_guardian(&self, tx_data: TransactionData) -> TransactionEffects {
        let effects = self.execute_co_signed(tx_data, self.guardian).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        effects
    }

    /// An owner transaction calling `account_policy::<function>(registry, args.., ctx)`.
    async fn policy_call_tx(&self, function: &'static str, args: Vec<CallArg>) -> TransactionData {
        let mut call_args = vec![self.registry.clone()];
        call_args.extend(args);
        self.builder(self.owner)
            .await
            .move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                "account_policy",
                function,
                call_args,
            )
            .build()
    }

    /// Opts the owner in with a SUI limit and one listed recipient, all while pending.
    async fn enable_policy(&self) {
        let tx = self
            .policy_call_tx("enable", vec![pure(&self.guardian), pure(&self.gas_cap)])
            .await;
        self.execute_ok(tx).await;
        let tx = self
            .policy_call_tx(
                "set_coin_limit",
                vec![pure(&SUI_TYPE.to_string()), pure(&SUI_LIMIT)],
            )
            .await;
        self.execute_ok(tx).await;
        let tx = self
            .policy_call_tx("add_recipient", vec![pure(&self.recipient)])
            .await;
        self.execute_ok(tx).await;
    }

    async fn set_package_tx(
        &self,
        package: ObjectID,
        custody: bool,
        custody_types: Vec<String>,
        custody_limit: Option<u64>,
    ) -> TransactionData {
        self.policy_call_tx(
            "set_package",
            vec![
                pure(&package),
                pure(&custody),
                pure(&custody_types),
                pure(&custody_limit),
            ],
        )
        .await
    }

    async fn transfer_sui_tx(&self, amount: u64, to: SuiAddress) -> TransactionData {
        self.builder(self.owner)
            .await
            .transfer_sui(Some(amount), to)
            .build()
    }

    /// Objects used as inputs are re-fetched by ID because a transaction the policy rejects
    /// still bumps the versions of its inputs.
    async fn latest(&self, object: ObjectID) -> ObjectRef {
        self.cluster.get_latest_object_ref(&object).await
    }

    async fn transfer_object_tx(&self, object: ObjectID, to: SuiAddress) -> TransactionData {
        let object = self.latest(object).await;
        self.builder(self.owner)
            .await
            .transfer(FullObjectRef::from_fastpath_ref(object), to)
            .build()
    }

    async fn stake_tx(&self) -> TransactionData {
        let validator = self.cluster.swarm.config().validator_configs()[0].sui_address();
        let stake_coin = self.gas_objects(self.owner).await[1];
        self.builder(self.owner)
            .await
            .call_staking(stake_coin, validator)
            .build()
    }

    async fn publish_test_package(&self) -> TestPackage {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/move_test_code_account_policy");
        let tx = self
            .builder(self.owner)
            .await
            .publish_async(path)
            .await
            .build();
        let effects = self.execute_ok(tx).await;
        let mut package = None;
        let mut treasury_cap = None;
        for (object_ref, _) in effects.created() {
            let object = self
                .cluster
                .get_object_from_fullnode_store(&object_ref.0)
                .await
                .unwrap();
            if object.is_package() {
                package = Some(object_ref.0);
            } else if object
                .type_()
                .is_some_and(|ty| ty.to_string().contains("TreasuryCap"))
            {
                treasury_cap = Some(object_ref.0);
            }
        }
        TestPackage {
            id: package.expect("package published"),
            treasury_cap: treasury_cap.expect("treasury cap created"),
        }
    }

    /// Mints an `Item` to the owner and returns its reference.
    async fn mint_item_tx(&self, package: &TestPackage) -> TransactionData {
        let mut builder = self.builder(self.owner).await;
        let ptb = builder.ptb_builder_mut();
        let item = ptb.programmable_move_call(
            package.id,
            ident_str!("items").to_owned(),
            ident_str!("mint").to_owned(),
            vec![],
            vec![],
        );
        ptb.transfer_arg(self.owner, item);
        builder.build()
    }

    async fn mint_item(&self, package: &TestPackage) -> ObjectID {
        let effects = self.execute_ok(self.mint_item_tx(package).await).await;
        owned_created(&effects, self.owner).0
    }

    /// `items::<function>(item, args..)`.
    async fn item_call_tx(
        &self,
        package: &TestPackage,
        function: &'static str,
        item: ObjectID,
        args: Vec<CallArg>,
    ) -> TransactionData {
        let item = self.latest(item).await;
        let mut call_args = vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(item))];
        call_args.extend(args);
        self.builder(self.owner)
            .await
            .move_call(package.id, "items", function, call_args)
            .build()
    }

    /// Mints `amount` of the test coin to the owner and returns the coin.
    async fn mint_policy_coin(&self, package: &TestPackage, amount: u64) -> ObjectID {
        let treasury_cap = self.latest(package.treasury_cap).await;
        let mut builder = self.builder(self.owner).await;
        let ptb = builder.ptb_builder_mut();
        let cap = ptb.obj(ObjectArg::ImmOrOwnedObject(treasury_cap)).unwrap();
        let amount = ptb.pure(amount).unwrap();
        let coin = ptb.programmable_move_call(
            package.id,
            ident_str!("policy_coin").to_owned(),
            ident_str!("mint").to_owned(),
            vec![],
            vec![cap, amount],
        );
        ptb.transfer_arg(self.owner, coin);
        let effects = self.execute_ok(builder.build()).await;
        owned_created(&effects, self.owner).0
    }
}

fn pure<T: serde::Serialize>(value: &T) -> CallArg {
    CallArg::Pure(bcs::to_bytes(value).unwrap())
}

/// The single object `effects` created for `owner`.
fn owned_created(effects: &TransactionEffects, owner: SuiAddress) -> ObjectRef {
    let mut created = effects
        .created()
        .into_iter()
        .filter(|(_, object_owner)| *object_owner == Owner::AddressOwner(owner))
        .map(|(object_ref, _)| object_ref);
    let object_ref = created.next().expect("one object created for owner");
    assert!(created.next().is_none(), "more than one object created");
    object_ref
}

fn assert_violation(effects: &TransactionEffects, expected: AccountPolicyViolationKind) {
    match effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::AccountPolicyViolation { kind },
            ..
        }) if *kind == expected => {}
        other => panic!("expected policy violation {expected:?}, got {other:?}"),
    }
}

#[sim_test]
async fn test_account_policy_enforced_after_activation() {
    let env = Env::new().await;
    let package = env.publish_test_package().await;
    env.enable_policy().await;

    // Pending policies are not enforced.
    env.execute_ok(env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await)
        .await;

    env.cluster.trigger_reconfiguration().await;

    assert_violation(
        &env.execute(env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await)
            .await,
        AccountPolicyViolationKind::CoinOutflowExceeded,
    );
    env.execute_ok(env.transfer_sui_tx(SUI_LIMIT / 2, env.stranger).await)
        .await;

    // Rules visible from the transaction alone are enforced at admission, costing no gas.
    let tx = env
        .builder(env.owner)
        .await
        .transfer_sui(Some(1), env.stranger)
        .with_gas_budget(env.gas_cap + 1)
        .build();
    env.execute_rejected(tx).await;

    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/move_test_code_account_policy");
    let tx = env
        .builder(env.owner)
        .await
        .publish_async(path)
        .await
        .build();
    env.execute_rejected(tx).await;

    // Unlisted packages may not be called, not even a read-only framework function.
    let tx = env
        .builder(env.owner)
        .await
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "account_policy",
            "exists",
            vec![env.registry.clone(), pure(&env.owner)],
        )
        .build();
    env.execute_rejected(tx).await;
    env.execute_rejected(env.mint_item_tx(&package).await).await;

    // Staking keeps the SUI with the owner, so a stake far above the limit is allowed.
    let effects = env.execute_ok(env.stake_tx().await).await;
    let staked_sui = owned_created(&effects, env.owner).0;

    // But the stake object may not go to an unlisted address.
    assert_violation(
        &env.execute(env.transfer_object_tx(staked_sui, env.stranger).await)
            .await,
        AccountPolicyViolationKind::ObjectTransferNotAllowed,
    );

    // A co-signature from anyone but the guardian changes nothing.
    let tx = env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await;
    assert_violation(
        &env.execute_co_signed(tx, env.stranger).await,
        AccountPolicyViolationKind::CoinOutflowExceeded,
    );

    // A transaction co-signed by the guardian is exempt from every rule.
    env.execute_with_guardian(env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await)
        .await;
}

#[sim_test]
async fn test_account_policy_listed_recipients() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    // Coins and objects may go to a listed recipient without limit.
    env.execute_ok(env.transfer_sui_tx(5 * SUI_LIMIT, env.recipient).await)
        .await;
    let effects = env.execute_ok(env.stake_tx().await).await;
    let staked_sui = owned_created(&effects, env.owner).0;
    env.execute_ok(env.transfer_object_tx(staked_sui, env.recipient).await)
        .await;

    // Removing the recipient needs the guardian once the policy is active.
    env.execute_rejected(
        env.policy_call_tx("remove_recipient", vec![pure(&env.recipient)])
            .await,
    )
    .await;
    let tx = env
        .policy_call_tx("remove_recipient", vec![pure(&env.recipient)])
        .await;
    env.execute_with_guardian(tx).await;
    assert_violation(
        &env.execute(env.transfer_sui_tx(5 * SUI_LIMIT, env.recipient).await)
            .await,
        AccountPolicyViolationKind::CoinOutflowExceeded,
    );
}

#[sim_test]
async fn test_account_policy_coin_type_limits() {
    let env = Env::new().await;
    let package = env.publish_test_package().await;
    let coin = env.mint_policy_coin(&package, 1_000).await;
    let small_coin = env.mint_policy_coin(&package, 10).await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    // A coin type without a limit may not flow out at all.
    assert_violation(
        &env.execute(env.transfer_object_tx(small_coin, env.stranger).await)
            .await,
        AccountPolicyViolationKind::CoinOutflowExceeded,
    );

    let tx = env
        .policy_call_tx(
            "set_coin_limit",
            vec![pure(&package.coin_type()), pure(&100u64)],
        )
        .await;
    env.execute_with_guardian(tx).await;
    env.execute_ok(env.transfer_object_tx(small_coin, env.stranger).await)
        .await;
    assert_violation(
        &env.execute(env.transfer_object_tx(coin, env.stranger).await)
            .await,
        AccountPolicyViolationKind::CoinOutflowExceeded,
    );
}

#[sim_test]
async fn test_account_policy_package_custody() {
    let env = Env::new().await;
    let package = env.publish_test_package().await;
    let item = env.mint_item(&package).await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    // Allowed to be called, but not trusted with the owner's objects.
    env.execute_with_guardian(env.set_package_tx(package.id, false, vec![], None).await)
        .await;
    let fresh_item = env.mint_item(&package).await;
    assert_violation(
        &env.execute(env.item_call_tx(&package, "wrap", item, vec![]).await)
            .await,
        AccountPolicyViolationKind::ObjectTransferNotAllowed,
    );
    assert_violation(
        &env.execute(env.item_call_tx(&package, "burn", item, vec![]).await)
            .await,
        AccountPolicyViolationKind::ObjectTransferNotAllowed,
    );
    assert_violation(
        &env.execute(
            env.item_call_tx(&package, "send", item, vec![pure(&env.stranger)])
                .await,
        )
        .await,
        AccountPolicyViolationKind::ObjectTransferNotAllowed,
    );

    // Custody limited to a type the items are not.
    env.execute_with_guardian(
        env.set_package_tx(package.id, true, vec![package.wrapper_type()], None)
            .await,
    )
    .await;
    assert_violation(
        &env.execute(env.item_call_tx(&package, "wrap", item, vec![]).await)
            .await,
        AccountPolicyViolationKind::ObjectTransferNotAllowed,
    );

    // Custody of items: wrapping, burning and sending all pass.
    env.execute_with_guardian(
        env.set_package_tx(package.id, true, vec![package.item_type()], None)
            .await,
    )
    .await;
    env.execute_ok(env.item_call_tx(&package, "wrap", item, vec![]).await)
        .await;
    env.execute_ok(
        env.item_call_tx(&package, "send", fresh_item, vec![pure(&env.stranger)])
            .await,
    )
    .await;
    let another_item = env.mint_item(&package).await;
    env.execute_ok(
        env.item_call_tx(&package, "burn", another_item, vec![])
            .await,
    )
    .await;

    // Items may still not leave through a plain transfer.
    let last_item = env.mint_item(&package).await;
    assert_violation(
        &env.execute(env.transfer_object_tx(last_item, env.stranger).await)
            .await,
        AccountPolicyViolationKind::ObjectTransferNotAllowed,
    );
}

#[sim_test]
async fn test_account_policy_cancel_before_activation() {
    let env = Env::new().await;
    env.enable_policy().await;

    env.execute_ok(env.policy_call_tx("cancel", vec![]).await)
        .await;

    env.cluster.trigger_reconfiguration().await;
    env.execute_ok(env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await)
        .await;
}

#[sim_test]
async fn test_account_policy_disable_requires_guardian_co_signature() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    // With the key alone, disabling is a framework call the policy does not allow.
    env.execute_rejected(env.policy_call_tx("disable", vec![]).await)
        .await;
    assert_violation(
        &env.execute(env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await)
            .await,
        AccountPolicyViolationKind::CoinOutflowExceeded,
    );

    env.execute_with_guardian(env.policy_call_tx("disable", vec![]).await)
        .await;
    env.execute_ok(env.transfer_sui_tx(2 * SUI_LIMIT, env.stranger).await)
        .await;
}

#[sim_test]
async fn test_account_policy_budget_spans_the_epoch() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    // Two transfers each under the limit together exceed it; a failed attempt still costs gas,
    // and the budget resets with the epoch.
    env.execute_ok(env.transfer_sui_tx(SUI_LIMIT * 6 / 10, env.stranger).await)
        .await;
    assert_violation(
        &env.execute(env.transfer_sui_tx(SUI_LIMIT * 6 / 10, env.stranger).await)
            .await,
        AccountPolicyViolationKind::CoinOutflowExceeded,
    );
    env.execute_ok(env.transfer_sui_tx(SUI_LIMIT / 10, env.stranger).await)
        .await;

    // Once the gas budget alone no longer fits, validators refuse to sign at all.
    env.execute_ok(env.transfer_sui_tx(SUI_LIMIT * 2 / 10, env.stranger).await)
        .await;
    let tx = env
        .builder(env.owner)
        .await
        .transfer_sui(Some(1), env.stranger)
        .with_gas_budget(env.gas_cap)
        .build();
    env.execute_rejected(tx).await;

    env.cluster.trigger_reconfiguration().await;
    env.execute_ok(env.transfer_sui_tx(SUI_LIMIT * 6 / 10, env.stranger).await)
        .await;
}

#[sim_test]
async fn test_account_policy_custody_limit_spans_the_epoch() {
    let env = Env::new().await;
    let package = env.publish_test_package().await;
    let first = env.mint_item(&package).await;
    let second = env.mint_item(&package).await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    env.execute_with_guardian(env.set_package_tx(package.id, true, vec![], Some(1)).await)
        .await;
    env.execute_ok(env.item_call_tx(&package, "burn", first, vec![]).await)
        .await;
    assert_violation(
        &env.execute(env.item_call_tx(&package, "burn", second, vec![]).await)
            .await,
        AccountPolicyViolationKind::CustodyLimitExceeded,
    );

    env.cluster.trigger_reconfiguration().await;
    env.execute_ok(env.item_call_tx(&package, "burn", second, vec![]).await)
        .await;
}
