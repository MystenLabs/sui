// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{FullObjectRef, ObjectRef, SuiAddress},
    effects::{TransactionEffects, TransactionEffectsAPI},
    execution_status::{
        AccountPolicyViolationKind, ExecutionFailure, ExecutionFailureStatus, ExecutionStatus,
    },
    object::Owner,
    transaction::{
        CallArg, ObjectArg, SharedObjectMutability,
        TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE, TransactionData,
    },
};
use test_cluster::{TestCluster, TestClusterBuilder};

const MIST_PER_SUI: u64 = 1_000_000_000;
const SUI_LIMIT: u64 = MIST_PER_SUI;

struct Env {
    cluster: TestCluster,
    owner: SuiAddress,
    guardian: SuiAddress,
    recipient: SuiAddress,
    rgp: u64,
    gas_cap: u64,
    registry: CallArg,
    _guard: sui_protocol_config::OverrideGuard,
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
            rgp,
            gas_cap: rgp * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
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
        TestTransactionBuilder::new(sender, gas, self.rgp)
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

    async fn enable_policy(&self) {
        let tx = self
            .builder(self.owner)
            .await
            .move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                "account_policy",
                "enable",
                vec![
                    self.registry.clone(),
                    CallArg::Pure(bcs::to_bytes(&self.guardian).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&SUI_LIMIT).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&self.gas_cap).unwrap()),
                ],
            )
            .build();
        self.execute_ok(tx).await;
    }

    /// An owner transaction calling `account_policy::<function>(registry, ctx)`.
    async fn policy_call_tx(&self, function: &'static str) -> TransactionData {
        self.builder(self.owner)
            .await
            .move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                "account_policy",
                function,
                vec![self.registry.clone()],
            )
            .build()
    }

    async fn transfer_sui_tx(&self, amount: u64) -> TransactionData {
        self.builder(self.owner)
            .await
            .transfer_sui(Some(amount), self.recipient)
            .build()
    }

    /// The guardian exempts `tx_data` from the owner's policy.
    async fn approve(&self, tx_data: &TransactionData) {
        let tx = self
            .builder(self.guardian)
            .await
            .move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                "account_policy",
                "approve",
                vec![
                    self.registry.clone(),
                    CallArg::Pure(bcs::to_bytes(&self.owner).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&tx_data.digest().inner().to_vec()).unwrap()),
                ],
            )
            .build();
        self.execute_ok(tx).await;
    }
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
    env.enable_policy().await;

    // Pending policies are not enforced.
    env.execute_ok(env.transfer_sui_tx(2 * SUI_LIMIT).await)
        .await;

    env.cluster.trigger_reconfiguration().await;

    let effects = env.execute(env.transfer_sui_tx(2 * SUI_LIMIT).await).await;
    assert_violation(&effects, AccountPolicyViolationKind::SuiOutflowExceeded);
    env.execute_ok(env.transfer_sui_tx(SUI_LIMIT / 2).await)
        .await;

    let tx = env
        .builder(env.owner)
        .await
        .transfer_sui(Some(1), env.recipient)
        .with_gas_budget(env.gas_cap + 1)
        .build();
    assert_violation(
        &env.execute(tx).await,
        AccountPolicyViolationKind::GasBudgetExceeded,
    );

    // Only the system package may be called, even for a read-only framework function.
    let tx = env
        .builder(env.owner)
        .await
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "account_policy",
            "exists",
            vec![
                env.registry.clone(),
                CallArg::Pure(bcs::to_bytes(&env.owner).unwrap()),
            ],
        )
        .build();
    assert_violation(
        &env.execute(tx).await,
        AccountPolicyViolationKind::PackageNotAllowed,
    );

    // Staking keeps the SUI with the owner, so a stake far above the limit is allowed.
    let validator = env.cluster.swarm.config().validator_configs()[0].sui_address();
    let stake_coin = env.gas_objects(env.owner).await[1];
    let tx = env
        .builder(env.owner)
        .await
        .call_staking(stake_coin, validator)
        .build();
    let effects = env.execute_ok(tx).await;
    let (staked_sui, _) = effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(env.owner))
        .expect("staking creates a StakedSui owned by the staker");

    // But the stake object may not leave the owner.
    let tx = env
        .builder(env.owner)
        .await
        .transfer(FullObjectRef::from_fastpath_ref(staked_sui), env.recipient)
        .build();
    assert_violation(
        &env.execute(tx).await,
        AccountPolicyViolationKind::ObjectTransferNotAllowed,
    );

    // A guardian-approved transaction is exempt from every rule.
    let tx = env.transfer_sui_tx(2 * SUI_LIMIT).await;
    env.approve(&tx).await;
    env.execute_ok(tx).await;
}

#[sim_test]
async fn test_account_policy_cancel_before_activation() {
    let env = Env::new().await;
    env.enable_policy().await;

    env.execute_ok(env.policy_call_tx("cancel").await).await;

    env.cluster.trigger_reconfiguration().await;
    env.execute_ok(env.transfer_sui_tx(2 * SUI_LIMIT).await)
        .await;
}

#[sim_test]
async fn test_account_policy_disable_requires_approval() {
    let env = Env::new().await;
    env.enable_policy().await;
    env.cluster.trigger_reconfiguration().await;

    // With the key alone, disabling aborts in Move and the policy stays in force.
    let effects = env.execute(env.policy_call_tx("disable").await).await;
    assert!(effects.status().is_err());
    assert_violation(
        &env.execute(env.transfer_sui_tx(2 * SUI_LIMIT).await).await,
        AccountPolicyViolationKind::SuiOutflowExceeded,
    );

    let tx = env.policy_call_tx("disable").await;
    env.approve(&tx).await;
    env.execute_ok(tx).await;
    env.execute_ok(env.transfer_sui_tx(2 * SUI_LIMIT).await)
        .await;
}
