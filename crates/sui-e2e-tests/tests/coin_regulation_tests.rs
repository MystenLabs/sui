// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::language_storage::{StructTag, TypeTag};
use prometheus::Registry;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_macros::sim_test;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use sui_rpc::proto::sui::rpc::v2::{GetObjectRequest, ListOwnedObjectsRequest};
use sui_types::accumulator_root::EmptyUnsettledObjectFunds;
use sui_types::balance::Balance;
use sui_types::base_types::{FullObjectRef, ObjectID, ObjectRef, SuiAddress, SystemObjectVersions};
use sui_types::coin_regulation::coin_type_name;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{
    AccumulatorOperation, AccumulatorValue, InputConsensusObject, TransactionEffects,
    TransactionEffectsAPI,
};
use sui_types::error::{SuiError, SuiErrorKind, UserInputError};
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::execution_status::{ExecutionErrorKind, ExecutionStatus};
use sui_types::metrics::ExecutionMetrics;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use sui_types::transaction::{
    Argument, CallArg, CheckedInputObjects, FundsWithdrawalArg, InputObjectKind, InputObjects,
    ObjectArg, ObjectReadResult, ObjectReadResultKind, SharedObjectMutability, TransactionData,
    TransactionDataAPI,
};
use sui_types::transaction_executor::TransactionChecks;
use sui_types::{
    SUI_COIN_REGISTRY_OBJECT_ID, SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_ADDRESS,
    SUI_FRAMEWORK_PACKAGE_ID, SUI_FREEZE_LIST_OBJECT_ID,
};
use test_cluster::TestClusterBuilder;
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder, get_balance};

/// The FreezeList must be created by the end-of-epoch transaction when a chain upgrades to
/// protocol version 145, since only freshly-created networks get it at genesis.
#[sim_test]
async fn test_create_freeze_list_object_at_upgrade() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(144.into())
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(144, 145))
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    for h in &handles {
        h.with(|node| {
            assert!(
                node.state()
                    .get_object_cache_reader()
                    .get_latest_object_ref_or_tombstone(SUI_FREEZE_LIST_OBJECT_ID)
                    .is_none()
            );
        });
    }

    test_cluster.wait_for_protocol_version(145.into()).await;
    // The upgrade completes in epoch 1, whose end-of-epoch transaction creates the FreezeList.
    test_cluster.wait_for_epoch_all_nodes(2).await;

    for h in &handles {
        h.with(|node| {
            let state = node.state();
            let freeze_list = state
                .get_object_cache_reader()
                .get_object(&SUI_FREEZE_LIST_OBJECT_ID)
                .expect("freeze list object should exist");
            let Owner::Shared {
                initial_shared_version,
            } = freeze_list.owner
            else {
                panic!("the freeze list must be shared");
            };
            assert_eq!(
                state
                    .epoch_store_for_testing()
                    .epoch_start_config()
                    .freeze_list_obj_initial_shared_version(),
                Some(initial_shared_version),
            );
        });
    }
}

/// A frozen sender and a paused coin are rejected when the validator signs, before consensus.
#[sim_test]
async fn test_signing_rejects_frozen_sender_and_paused_coin() {
    let mut env = RegulatedCoinEnv::new().await;
    env.migrate(vec![]).await;
    let issuer = env.issuer;
    let user = env.env.get_sender(1);
    env.transfer_coin(issuer, env.coins[0], user).await;

    env.regulate("freeze_address", Some(user)).await;
    let tx = env.transfer_coin_tx(user, env.coins[0], issuer).await;
    let err = env.env.exec_tx_directly(tx).await.unwrap_err();
    assert_eq!(
        user_input_error(err),
        UserInputError::AddressDeniedForCoin {
            address: user,
            coin_type: env.coin_type_name(),
        }
    );
    env.env.transfer_sui(user, issuer, 1).await;

    env.regulate("enable_global_pause", None).await;
    let tx = env.transfer_coin_tx(issuer, env.coins[1], user).await;
    let err = env.env.exec_tx_directly(tx).await.unwrap_err();
    assert_eq!(
        user_input_error(err),
        UserInputError::CoinTypeGlobalPause {
            coin_type: env.coin_type_name(),
        }
    );

    env.regulate("disable_global_pause", None).await;
    env.regulate("unfreeze_address", Some(user)).await;
    env.transfer_coin(user, env.coins[0], issuer).await;
}

/// One soft bundle puts a transfer, a freeze of its recipient, and a second transfer into one
/// consensus commit. The transfers read the FreezeList implicitly at the versions before and
/// after the freeze's explicit mutable use, and every node reproduces the validators' effects.
#[sim_test]
async fn test_freeze_visible_to_later_transaction_in_same_commit() {
    let mut env = RegulatedCoinEnv::new().await;
    env.migrate(vec![]).await;
    let issuer = env.issuer;
    let sender = env.env.get_sender(1);
    let recipient = env.env.get_sender(2);
    env.transfer_coin(issuer, env.coins[0], sender).await;
    env.transfer_coin(issuer, env.coins[1], sender).await;

    let sender_gas = env.env.get_gas_for_sender(sender);
    let before = env
        .transfer_coin_tx_with_gas(sender, env.coins[0], recipient, sender_gas[0])
        .await;
    let freeze = env.regulation_tx("freeze_address", Some(recipient)).await;
    let after = env
        .transfer_coin_tx_with_gas(sender, env.coins[1], recipient, sender_gas[1])
        .await;
    let results = env
        .env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[before, freeze, after])
        .await
        .unwrap();
    let [(before, before_fx), (freeze, freeze_fx), (after, after_fx)] = results.as_slice() else {
        panic!("expected three results, got {results:?}");
    };

    assert!(before_fx.status().is_ok(), "{:?}", before_fx.status());
    assert!(freeze_fx.status().is_ok(), "{:?}", freeze_fx.status());
    assert_eq!(
        execution_error(after_fx),
        ExecutionErrorKind::AddressDeniedForCoin {
            address: recipient,
            coin_type: env.coin_type_name(),
        },
    );

    let Some(InputConsensusObject::ReadOnly((_, before_version, _))) =
        freeze_list_access(before_fx)
    else {
        panic!("{before}: expected an implicit FreezeList read");
    };
    let Some(InputConsensusObject::Mutate((_, freeze_input_version, _))) =
        freeze_list_access(freeze_fx)
    else {
        panic!("{freeze}: expected a mutable FreezeList input");
    };
    let Some(InputConsensusObject::ReadOnly((_, after_version, _))) = freeze_list_access(after_fx)
    else {
        panic!("{after}: expected an implicit FreezeList read");
    };
    assert_eq!(before_version, freeze_input_version);
    assert_eq!(after_version, freeze_fx.lamport_version());

    let digests = [*before, *freeze, *after];
    env.env
        .cluster
        .wait_for_tx_settlement_all_nodes(&digests)
        .await;
    assert_in_one_commit(&env.env.cluster.fullnode_handle.sui_node.state(), &digests);
    assert_effects_match_on_all_nodes(&env.env, &[before_fx, freeze_fx, after_fx]);
}

/// Executions pinned to an earlier FreezeList version never observe later freezes or
/// unfreezes, both when re-executed against a store that already holds the later state and
/// when a new fullnode replays every checkpoint.
#[sim_test]
async fn test_historical_reads_ignore_later_changes() {
    let mut env = RegulatedCoinEnv::new().await;
    env.migrate(vec![]).await;
    let issuer = env.issuer;
    let sender = env.env.get_sender(1);
    let recipient = env.env.get_sender(2);
    env.transfer_coin(issuer, env.coins[0], sender).await;
    env.transfer_coin(issuer, env.coins[1], sender).await;

    let (passed, passed_fx) = env
        .execute(env.transfer_coin_tx(sender, env.coins[0], recipient).await)
        .await;
    assert!(passed_fx.status().is_ok(), "{:?}", passed_fx.status());
    env.regulate("freeze_address", Some(recipient)).await;
    let (failed, failed_fx) = env
        .execute(env.transfer_coin_tx(sender, env.coins[1], recipient).await)
        .await;
    assert_eq!(
        execution_error(&failed_fx),
        ExecutionErrorKind::AddressDeniedForCoin {
            address: recipient,
            coin_type: env.coin_type_name(),
        }
    );

    // Invert the state both transactions saw: the sender becomes frozen, the recipient not.
    env.regulate("unfreeze_address", Some(recipient)).await;
    env.regulate("freeze_address", Some(sender)).await;

    let fullnode = env.env.cluster.fullnode_handle.sui_node.state();
    for (digest, effects) in [(passed, &passed_fx), (failed, &failed_fx)] {
        assert_eq!(&reexecute(&fullnode, &digest), effects);
    }

    let new_fullnode = env.env.cluster.spawn_new_fullnode().await;
    env.env
        .cluster
        .wait_for_tx_settlement_all_nodes(&[passed, failed])
        .await;
    new_fullnode.sui_node.with(|node| {
        let cache = node.state().get_transaction_cache_reader().clone();
        assert_eq!(cache.get_executed_effects(&passed).unwrap(), passed_fx);
        assert_eq!(cache.get_executed_effects(&failed).unwrap(), failed_fx);
    });
}

/// Simulation, with and without transaction checks, reads the FreezeList at the fullnode's
/// latest version.
#[sim_test]
async fn test_simulation_reads_latest_state() {
    let mut env = RegulatedCoinEnv::new().await;
    env.migrate(vec![]).await;
    let issuer = env.issuer;
    let recipient = env.env.get_sender(1);
    let tx = env.transfer_coin_tx(issuer, env.coins[0], recipient).await;

    let (_, freeze_fx) = env.regulate("freeze_address", Some(recipient)).await;
    for checks in [TransactionChecks::Enabled, TransactionChecks::Disabled] {
        let effects = env.simulate(tx.clone(), checks);
        assert_eq!(
            execution_error(&effects),
            ExecutionErrorKind::AddressDeniedForCoin {
                address: recipient,
                coin_type: env.coin_type_name(),
            },
            "{checks:?}"
        );
        assert_eq!(
            freeze_list_access(&effects).map(|o| o.id_and_version().1),
            Some(freeze_fx.lamport_version())
        );
    }

    let (_, unfreeze_fx) = env.regulate("unfreeze_address", Some(recipient)).await;
    for checks in [TransactionChecks::Enabled, TransactionChecks::Disabled] {
        let effects = env.simulate(tx.clone(), checks);
        assert!(
            effects.status().is_ok(),
            "{checks:?}: {:?}",
            effects.status()
        );
        assert_eq!(
            freeze_list_access(&effects).map(|o| o.id_and_version().1),
            Some(unfreeze_fx.lamport_version())
        );
    }
}

/// Migration replaces the DenyCapV2 with a ComplianceCap at the same ID, which the fullnode's
/// object reads and owner index report under the new type, and freezes only the listed denied
/// addresses.
#[sim_test]
async fn test_migration_end_to_end() {
    let mut env = RegulatedCoinEnv::new().await;
    let issuer = env.issuer;
    let listed = env.env.get_sender(1);
    let omitted = env.env.get_sender(2);
    for addr in [listed, omitted] {
        env.legacy_deny(addr).await;
    }

    let deny_cap = env.object(env.cap).await;
    assert!(deny_cap.type_().unwrap().is_coin_deny_cap_v2());
    let migrate_fx = env.migrate(vec![listed]).await;
    assert!(
        migrate_fx
            .mutated()
            .iter()
            .any(|((id, version, _), owner)| *id == env.cap
                && *version == migrate_fx.lamport_version()
                && *owner == Owner::AddressOwner(issuer))
    );
    assert!(migrate_fx.created().iter().all(|(r, _)| r.0 != env.cap));
    assert!(migrate_fx.deleted().iter().all(|r| r.0 != env.cap));
    assert!(
        migrate_fx
            .modified_at_versions()
            .contains(&(env.cap, deny_cap.version()))
    );

    let compliance_cap_type = StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ident_str!("coin_regulation").to_owned(),
        name: ident_str!("ComplianceCap").to_owned(),
        type_params: vec![env.coin_type.clone()],
    };
    let compliance_cap = env.object(env.cap).await;
    assert_eq!(
        compliance_cap.struct_tag().unwrap(),
        compliance_cap_type.clone()
    );

    let channel = tonic::transport::Channel::from_shared(env.env.cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut ledger = LedgerServiceClient::new(channel.clone());
    let mut state = StateServiceClient::new(channel);
    let cap_id = env.cap.to_canonical_string(true);
    for (version, expected) in [
        (None, compliance_cap_type.clone()),
        (
            Some(deny_cap.version()),
            deny_cap.struct_tag().unwrap().clone(),
        ),
    ] {
        let object = ledger
            .get_object({
                let mut message = GetObjectRequest::default();
                message.object_id = Some(cap_id.clone());
                message.version = version.map(|v| v.value());
                message.read_mask = Some(FieldMask::from_paths(["object_type"]));
                message
            })
            .await
            .unwrap()
            .into_inner()
            .object
            .unwrap();
        assert_eq!(
            StructTag::from_str(object.object_type()).unwrap(),
            expected,
            "{version:?}"
        );
    }
    for (object_type, listed) in [
        ("0x2::coin_regulation::ComplianceCap", true),
        ("0x2::coin::DenyCapV2", false),
    ] {
        let objects = state
            .list_owned_objects({
                let mut message = ListOwnedObjectsRequest::default();
                message.owner = Some(issuer.to_string());
                message.object_type = Some(object_type.to_owned());
                message
            })
            .await
            .unwrap()
            .into_inner()
            .objects;
        assert_eq!(
            objects.iter().any(|o| o.object_id() == cap_id),
            listed,
            "{object_type}"
        );
    }

    // The legacy deny list is ignored after migration, so only the listed address is frozen.
    let (_, effects) = env
        .execute(env.transfer_coin_tx(issuer, env.coins[0], listed).await)
        .await;
    assert_eq!(
        execution_error(&effects),
        ExecutionErrorKind::AddressDeniedForCoin {
            address: listed,
            coin_type: env.coin_type_name(),
        },
    );
    let (_, effects) = env
        .execute(env.transfer_coin_tx(issuer, env.coins[1], omitted).await)
        .await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
}

/// A chain upgrades from protocol version 144 to 145, which enables coin regulation. In the
/// upgrade epoch the FreezeList does not exist yet, so the legacy deny list decides, every node
/// agrees on the effects, and no effects list the FreezeList. Once it exists, a freeze is enforced.
#[sim_test]
async fn test_regulated_coin_in_upgrade_epoch() {
    let builder = TestEnvBuilder::new()
        .with_num_validators(4)
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder
                .with_protocol_version(144.into())
                .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                    144, 145,
                ))
                .disable_fullnode_pruning()
        }));
    let mut env = RegulatedCoinEnv::build(builder).await;
    let issuer = env.issuer;
    let denied = env.env.get_sender(1);
    let clean = env.env.get_sender(2);
    let denied_in_upgrade_epoch = env.env.get_sender(3);
    let frozen = env.env.get_sender(4);
    let coin_type_name = env.coin_type_name();
    let denied_for_coin = |address| ExecutionErrorKind::AddressDeniedForCoin {
        address,
        coin_type: coin_type_name.clone(),
    };

    assert_epoch_state(&env.env, 0, 144, false);
    env.transfer_coin(issuer, env.coins[0], denied).await;
    env.legacy_deny(denied).await;

    env.env.cluster.trigger_reconfiguration().await;
    env.env.update_all_gas().await;
    assert_epoch_state(&env.env, 1, 145, false);

    let mut upgrade_epoch_fx = vec![];
    let tx = env
        .env
        .tx_builder(clean)
        .transfer_sui(Some(1), issuer)
        .build();
    upgrade_epoch_fx.push(env.execute_ok(tx).await.1);

    for (coin, recipient) in [(env.coins[1], clean), (env.coins[2], frozen)] {
        let tx = env.transfer_coin_tx(issuer, coin, recipient).await;
        upgrade_epoch_fx.push(env.execute_ok(tx).await.1);
    }

    // The legacy deny list still decides, at execution and at signing.
    let tx = env.transfer_coin_tx(issuer, env.coins[3], denied).await;
    let (_, effects) = env.execute(tx).await;
    assert_eq!(execution_error(&effects), denied_for_coin(denied));
    upgrade_epoch_fx.push(effects);
    let tx = env.transfer_coin_tx(denied, env.coins[0], issuer).await;
    let err = env.env.exec_tx_directly(tx).await.unwrap_err();
    assert_eq!(
        user_input_error(err),
        UserInputError::AddressDeniedForCoin {
            address: denied,
            coin_type: coin_type_name.clone(),
        }
    );

    // The same through address balances: a deposit, then a declared withdrawal.
    let (_, effects) = env.send_funds(env.coins[4], clean).await;
    upgrade_epoch_fx.push(effects);
    let gas = env.env.get_gas_for_sender(clean)[0];
    let tx = env.send_balance_tx(clean, 40, issuer, gas);
    let (withdraw, effects) = env.execute_ok(tx).await;
    assert_balance_writes(
        &effects,
        &env.coin_type,
        &[
            (clean, AccumulatorOperation::Split, 40),
            (issuer, AccumulatorOperation::Merge, 40),
        ],
    );
    upgrade_epoch_fx.push(effects);
    env.env
        .cluster
        .wait_for_tx_settlement_all_nodes(&[withdraw])
        .await;
    let gas = env.env.get_gas_for_sender(issuer)[0];
    let tx = env.send_balance_tx(issuer, 40, denied, gas);
    let (_, effects) = env.execute(tx).await;
    assert_eq!(execution_error(&effects), denied_for_coin(denied));
    upgrade_epoch_fx.push(effects);

    let (_, effects) = env.legacy_deny(denied_in_upgrade_epoch).await;
    upgrade_epoch_fx.push(effects);

    let digests: Vec<_> = upgrade_epoch_fx
        .iter()
        .map(|fx| *fx.transaction_digest())
        .collect();
    env.env
        .cluster
        .wait_for_tx_settlement_all_nodes(&digests)
        .await;
    assert_effects_match_on_all_nodes(&env.env, &upgrade_epoch_fx.iter().collect::<Vec<_>>());
    for effects in &upgrade_epoch_fx {
        let digest = effects.transaction_digest();
        assert_eq!(effects.executed_epoch(), 1, "{digest}");
        assert!(!lists_freeze_list(effects), "{digest} lists the FreezeList");
    }
    assert_balances_on_all_nodes(
        &env.env,
        &env.coin_type,
        &[(clean, COIN_VALUE - 40), (issuer, 40)],
    );
    assert_epoch_state(&env.env, 1, 145, false);

    env.env.cluster.trigger_reconfiguration().await;
    env.env.update_all_gas().await;
    assert_epoch_state(&env.env, 2, 145, true);

    // The deny list entry written in the upgrade epoch is in force.
    let tx = env
        .transfer_coin_tx(issuer, env.coins[3], denied_in_upgrade_epoch)
        .await;
    let (_, effects) = env.execute(tx).await;
    assert_eq!(
        execution_error(&effects),
        denied_for_coin(denied_in_upgrade_epoch)
    );

    env.migrate(vec![]).await;
    env.regulate("freeze_address", Some(frozen)).await;
    let tx = env.transfer_coin_tx(issuer, env.coins[3], frozen).await;
    let (_, effects) = env.execute(tx).await;
    assert_eq!(execution_error(&effects), denied_for_coin(frozen));
    assert!(matches!(
        freeze_list_access(&effects),
        Some(InputConsensusObject::ReadOnly(_))
    ));
    let tx = env.transfer_coin_tx(frozen, env.coins[2], issuer).await;
    let err = env.env.exec_tx_directly(tx).await.unwrap_err();
    assert_eq!(
        user_input_error(err),
        UserInputError::AddressDeniedForCoin {
            address: frozen,
            coin_type: coin_type_name.clone(),
        }
    );
    // Migration with an empty address list drops the legacy entries.
    for (coin, recipient) in [(env.coins[3], clean), (env.coins[5], denied)] {
        let tx = env.transfer_coin_tx(issuer, coin, recipient).await;
        env.execute_ok(tx).await;
    }
}

/// The value of each coin minted by the test package.
const COIN_VALUE: u64 = 1_000;

struct RegulatedCoinEnv {
    env: TestEnv,
    issuer: SuiAddress,
    coin_type: TypeTag,
    /// The `DenyCapV2`, and after migration the `ComplianceCap` with the same ID.
    cap: ObjectID,
    metadata: ObjectID,
    coins: Vec<ObjectID>,
}

impl RegulatedCoinEnv {
    /// Publishes a regulated coin whose `DenyCapV2` and coins belong to the first sender.
    async fn new() -> Self {
        let builder = TestEnvBuilder::new()
            .with_num_validators(4)
            .with_test_cluster_builder_cb(Box::new(|builder| builder.disable_fullnode_pruning()));
        Self::build(builder).await
    }

    async fn build(builder: TestEnvBuilder) -> Self {
        let mut env = builder.build().await;
        let issuer = env.get_sender(0);
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/coin_regulation_test_code");
        let tx = env.tx_builder(issuer).publish_async(path).await.build();
        let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
        assert!(effects.status().is_ok(), "{:?}", effects.status());

        let mut coin_type = None;
        let mut cap = None;
        let mut metadata = None;
        let mut coins = vec![];
        for (oref, _) in effects.created() {
            let object = env
                .cluster
                .get_object_from_fullnode_store(&oref.0)
                .await
                .unwrap();
            if object.is_package() {
                continue;
            }
            if object.type_().unwrap().is_coin_deny_cap_v2() {
                cap = Some(object.id());
            } else if object.type_().unwrap().is_coin_metadata() {
                metadata = Some(object.id());
            } else if object.is_coin() {
                coin_type = object.coin_type_maybe();
                coins.push(object.id());
            }
        }
        Self {
            env,
            issuer,
            coin_type: coin_type.unwrap(),
            cap: cap.unwrap(),
            metadata: metadata.unwrap(),
            coins,
        }
    }

    fn coin_type_name(&self) -> String {
        coin_type_name(&self.coin_type)
    }

    async fn object(&self, id: ObjectID) -> sui_types::object::Object {
        self.env
            .cluster
            .get_object_from_fullnode_store(&id)
            .await
            .unwrap()
    }

    async fn object_ref(&self, id: ObjectID) -> ObjectRef {
        self.env.cluster.get_latest_object_ref(&id).await
    }

    async fn shared_mut_arg(&self, id: ObjectID) -> ObjectArg {
        let Owner::Shared {
            initial_shared_version,
        } = self.object(id).await.owner
        else {
            panic!("{id} is not shared");
        };
        ObjectArg::SharedObject {
            id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        }
    }

    async fn execute(&mut self, tx: TransactionData) -> (TransactionDigest, TransactionEffects) {
        self.env.exec_tx_directly(tx).await.unwrap()
    }

    async fn execute_ok(&mut self, tx: TransactionData) -> (TransactionDigest, TransactionEffects) {
        let (digest, effects) = self.execute(tx).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        (digest, effects)
    }

    async fn transfer_coin_tx_with_gas(
        &self,
        sender: SuiAddress,
        coin: ObjectID,
        recipient: SuiAddress,
        gas: ObjectRef,
    ) -> TransactionData {
        let coin = self.object_ref(coin).await;
        self.env
            .tx_builder_with_gas(sender, gas)
            .transfer(FullObjectRef::from_fastpath_ref(coin), recipient)
            .build()
    }

    async fn transfer_coin_tx(
        &self,
        sender: SuiAddress,
        coin: ObjectID,
        recipient: SuiAddress,
    ) -> TransactionData {
        let gas = self.env.get_gas_for_sender(sender)[0];
        self.transfer_coin_tx_with_gas(sender, coin, recipient, gas)
            .await
    }

    async fn transfer_coin(&mut self, sender: SuiAddress, coin: ObjectID, recipient: SuiAddress) {
        let tx = self.transfer_coin_tx(sender, coin, recipient).await;
        self.execute_ok(tx).await;
    }

    /// Builds the issuer's call to `coin_regulation::<function><T>(freeze_list, cap[, addr])`.
    async fn regulation_tx(
        &self,
        function: &'static str,
        addr: Option<SuiAddress>,
    ) -> TransactionData {
        let mut args = vec![
            CallArg::Object(self.shared_mut_arg(SUI_FREEZE_LIST_OBJECT_ID).await),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(self.object_ref(self.cap).await)),
        ];
        args.extend(addr.map(|addr| CallArg::Pure(bcs::to_bytes(&addr).unwrap())));
        self.env
            .tx_builder(self.issuer)
            .move_call_with_type_args(
                SUI_FRAMEWORK_PACKAGE_ID,
                "coin_regulation",
                function,
                vec![self.coin_type.clone()],
                args,
            )
            .build()
    }

    async fn regulate(
        &mut self,
        function: &'static str,
        addr: Option<SuiAddress>,
    ) -> (TransactionDigest, TransactionEffects) {
        let tx = self.regulation_tx(function, addr).await;
        self.execute_ok(tx).await
    }

    async fn legacy_deny(&mut self, addr: SuiAddress) -> (TransactionDigest, TransactionEffects) {
        let tx = self
            .env
            .tx_builder(self.issuer)
            .move_call_with_type_args(
                SUI_FRAMEWORK_PACKAGE_ID,
                "coin",
                "deny_list_v2_add",
                vec![self.coin_type.clone()],
                vec![
                    CallArg::Object(self.shared_mut_arg(SUI_DENY_LIST_OBJECT_ID).await),
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(self.object_ref(self.cap).await)),
                    CallArg::Pure(bcs::to_bytes(&addr).unwrap()),
                ],
            )
            .build();
        self.execute_ok(tx).await
    }

    /// Registers the legacy coin's `Currency`, then converts the `DenyCapV2` into a
    /// `ComplianceCap` kept by the issuer through it.
    async fn migrate(&mut self, addresses: Vec<SuiAddress>) -> TransactionEffects {
        let metadata = self.object_ref(self.metadata).await;
        let mut builder = self.env.tx_builder(self.issuer);
        let ptb = builder.ptb_builder_mut();
        let args = vec![
            ptb.obj(self.shared_mut_arg(SUI_COIN_REGISTRY_OBJECT_ID).await)
                .unwrap(),
            ptb.obj(ObjectArg::ImmOrOwnedObject(metadata)).unwrap(),
        ];
        ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin_registry").to_owned(),
            ident_str!("migrate_legacy_metadata").to_owned(),
            vec![self.coin_type.clone()],
            args,
        );
        let tx = builder.build();
        let (_, effects) = self.execute_ok(tx).await;
        let (currency, _) = effects
            .created()
            .into_iter()
            .find(|(_, owner)| owner.is_shared())
            .unwrap();

        let currency = self.shared_mut_arg(currency.0).await;
        let freeze_list = self.shared_mut_arg(SUI_FREEZE_LIST_OBJECT_ID).await;
        let deny_list = self.shared_mut_arg(SUI_DENY_LIST_OBJECT_ID).await;
        let cap = self.object_ref(self.cap).await;
        let mut builder = self.env.tx_builder(self.issuer);
        let ptb = builder.ptb_builder_mut();
        let args = vec![
            ptb.obj(currency).unwrap(),
            ptb.obj(freeze_list).unwrap(),
            ptb.obj(deny_list).unwrap(),
            ptb.obj(ObjectArg::ImmOrOwnedObject(cap)).unwrap(),
            ptb.pure(addresses).unwrap(),
        ];
        let compliance_cap = ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin_registry").to_owned(),
            ident_str!("migrate_to_compliance_cap").to_owned(),
            vec![self.coin_type.clone()],
            args,
        );
        ptb.transfer_arg(self.issuer, compliance_cap);
        let tx = builder.build();
        self.execute_ok(tx).await.1
    }

    fn simulate(&self, tx: TransactionData, checks: TransactionChecks) -> TransactionEffects {
        self.env
            .cluster
            .fullnode_handle
            .sui_node
            .state()
            .simulate_transaction(tx, checks, false)
            .unwrap()
            .effects
    }

    /// Moves the issuer's `coin` into `recipient`'s address balance, settled on every node.
    async fn send_funds(
        &mut self,
        coin: ObjectID,
        recipient: SuiAddress,
    ) -> (TransactionDigest, TransactionEffects) {
        let coin = self.object_ref(coin).await;
        let tx = self
            .env
            .tx_builder(self.issuer)
            .move_call_with_type_args(
                SUI_FRAMEWORK_PACKAGE_ID,
                "coin",
                "send_funds",
                vec![self.coin_type.clone()],
                vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(coin)),
                    CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
                ],
            )
            .build();
        let (digest, effects) = self.execute_ok(tx).await;
        self.env
            .cluster
            .wait_for_tx_settlement_all_nodes(&[digest])
            .await;
        (digest, effects)
    }

    /// Builds `sender`'s transfer of `amount` from its address balance to `recipient`'s.
    fn send_balance_tx(
        &self,
        sender: SuiAddress,
        amount: u64,
        recipient: SuiAddress,
        gas: ObjectRef,
    ) -> TransactionData {
        let mut builder = self.env.tx_builder_with_gas(sender, gas);
        let ptb = builder.ptb_builder_mut();
        let withdrawal = ptb
            .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
                amount,
                self.coin_type.clone(),
            ))
            .unwrap();
        self.redeem_and_send(ptb, withdrawal, recipient);
        builder.build()
    }

    fn redeem_and_send(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        withdrawal: Argument,
        recipient: SuiAddress,
    ) {
        let balance = ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("balance").to_owned(),
            ident_str!("redeem_funds").to_owned(),
            vec![self.coin_type.clone()],
            vec![withdrawal],
        );
        let recipient = ptb.pure(recipient).unwrap();
        ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("balance").to_owned(),
            ident_str!("send_funds").to_owned(),
            vec![self.coin_type.clone()],
            vec![balance, recipient],
        );
    }
}

fn user_input_error(err: SuiError) -> UserInputError {
    match err.into_inner() {
        SuiErrorKind::UserInputError { error } => error,
        other => panic!("expected a user input error, got {other:?}"),
    }
}

fn execution_error(effects: &TransactionEffects) -> ExecutionErrorKind {
    match effects.status() {
        ExecutionStatus::Failure(failure) => failure.error.clone(),
        ExecutionStatus::Success => panic!("{} succeeded", effects.transaction_digest()),
    }
}

fn freeze_list_access(effects: &TransactionEffects) -> Option<InputConsensusObject> {
    effects
        .accessed_consensus_objects()
        .into_iter()
        .find(|o| o.id_and_version().0 == SUI_FREEZE_LIST_OBJECT_ID)
}

fn lists_freeze_list(effects: &TransactionEffects) -> bool {
    freeze_list_access(effects).is_some()
        || effects
            .unchanged_consensus_objects()
            .iter()
            .any(|(id, _)| *id == SUI_FREEZE_LIST_OBJECT_ID)
        || effects
            .object_changes()
            .iter()
            .any(|change| change.id == SUI_FREEZE_LIST_OBJECT_ID)
}

/// Asserts that the `Balance<coin_type>` accumulator writes of `effects` are `expected`, in any
/// order, and that `effects` has no other accumulator write.
fn assert_balance_writes(
    effects: &TransactionEffects,
    coin_type: &TypeTag,
    expected: &[(SuiAddress, AccumulatorOperation, u64)],
) {
    let balance_type = Balance::type_tag(coin_type.clone());
    let writes: Vec<_> = effects
        .accumulator_events()
        .into_iter()
        .map(|event| {
            assert_eq!(event.write.address.ty, balance_type);
            (
                event.write.address.address,
                event.write.operation,
                event.write.value,
            )
        })
        .collect();
    assert_eq!(writes.len(), expected.len(), "{writes:?}");
    for (address, operation, amount) in expected {
        let write = (
            *address,
            operation.clone(),
            AccumulatorValue::Integer(*amount),
        );
        assert!(writes.contains(&write), "{writes:?}");
    }
}

/// Asserts every node's epoch and protocol version, and whether the FreezeList exists there.
/// The coin regulation flags are on from version 145.
fn assert_epoch_state(env: &TestEnv, epoch: u64, protocol_version: u64, freeze_list: bool) {
    for handle in env.cluster.all_node_handles() {
        handle.with(|node| {
            let state = node.state();
            let epoch_store = state.epoch_store_for_testing();
            assert_eq!(epoch_store.epoch(), epoch);
            let config = epoch_store.protocol_config();
            assert_eq!(config.version.as_u64(), protocol_version);
            let enabled = protocol_version >= 145;
            assert_eq!(config.create_freeze_list(), enabled);
            assert_eq!(config.enable_coin_regulation(), enabled);
            assert_eq!(epoch_store.freeze_list_exists(), freeze_list);
            assert_eq!(
                state
                    .get_object_cache_reader()
                    .get_latest_object_ref_or_tombstone(SUI_FREEZE_LIST_OBJECT_ID)
                    .is_some(),
                freeze_list
            );
        });
    }
}

/// Asserts that `digests` sit in one checkpoint with no consensus commit prologue between them.
fn assert_in_one_commit(state: &AuthorityState, digests: &[TransactionDigest]) {
    let epoch_store = state.epoch_store_for_testing();
    let checkpoint = state
        .get_transaction_checkpoint_for_tests(&digests[0], &epoch_store)
        .unwrap()
        .unwrap();
    let contents = state
        .get_checkpoint_store()
        .get_checkpoint_contents(&checkpoint.content_digest)
        .unwrap()
        .unwrap();
    let order: Vec<TransactionDigest> = contents.iter().map(|d| d.transaction).collect();
    let positions: Vec<usize> = digests
        .iter()
        .map(|digest| {
            order
                .iter()
                .position(|d| d == digest)
                .unwrap_or_else(|| panic!("{digest} is not in checkpoint {checkpoint:?}"))
        })
        .collect();
    assert!(positions.is_sorted(), "{positions:?}");
    let cache = state.get_transaction_cache_reader();
    for digest in &order[positions[0]..=positions[positions.len() - 1]] {
        let tx = cache.get_transaction_block(digest).unwrap();
        assert!(
            !tx.transaction_data().is_consensus_commit_prologue(),
            "{digest} starts a new commit"
        );
    }
}

fn assert_effects_match_on_all_nodes(env: &TestEnv, expected: &[&TransactionEffects]) {
    for handle in env.cluster.all_node_handles() {
        handle.with(|node| {
            let cache = node.state().get_transaction_cache_reader().clone();
            for effects in expected {
                let digest = effects.transaction_digest();
                assert_eq!(
                    &&cache.get_executed_effects(digest).unwrap(),
                    effects,
                    "{digest}"
                );
            }
        });
    }
}

/// Re-executes an already executed transaction against `state`'s current store, at the input
/// and system object versions its effects recorded.
fn assert_balances_on_all_nodes(
    env: &TestEnv,
    coin_type: &TypeTag,
    expected: &[(SuiAddress, u64)],
) {
    for handle in env.cluster.all_node_handles() {
        handle.with(|node| {
            let state = node.state();
            let resolver = state.get_runtime_object_resolver();
            for (owner, balance) in expected {
                assert_eq!(
                    get_balance(resolver.as_ref(), *owner, coin_type.clone()),
                    *balance,
                    "{owner}"
                );
            }
        });
    }
}

fn reexecute(state: &AuthorityState, digest: &TransactionDigest) -> TransactionEffects {
    let epoch_store = state.epoch_store_for_testing();
    let cache = state.get_transaction_cache_reader();
    let tx = cache.get_transaction_block(digest).unwrap();
    let effects = cache.get_executed_effects(digest).unwrap();
    let store = state.get_object_store();
    let tx_data = tx.transaction_data();
    let consensus_versions = effects.accessed_consensus_objects();
    let input_objects = tx_data
        .input_objects()
        .unwrap()
        .into_iter()
        .map(|kind| {
            let object = match &kind {
                InputObjectKind::MovePackage(id) => store.get_object(id),
                InputObjectKind::ImmOrOwnedMoveObject((id, version, _)) => {
                    store.get_object_by_key(id, *version)
                }
                InputObjectKind::SharedMoveObject { id, .. } => {
                    let (_, version) = consensus_versions
                        .iter()
                        .map(|o| o.id_and_version())
                        .find(|(object_id, _)| object_id == id)
                        .unwrap();
                    store.get_object_by_key(id, version)
                }
            };
            ObjectReadResult::new(kind, ObjectReadResultKind::Object(object.unwrap()))
        })
        .collect();
    let system_object_versions = SystemObjectVersions::from_effects(&effects, store.as_ref());
    let (kind, signer, gas_data) = tx_data.execution_parts();
    let (_, _, reexecuted, _, _) = epoch_store.executor().execute_transaction_to_effects(
        state.get_backing_store().as_ref(),
        epoch_store.protocol_config(),
        Arc::new(ExecutionMetrics::new(&Registry::new())),
        false,
        ExecutionOrEarlyError::ok(None),
        &epoch_store.epoch(),
        epoch_store
            .epoch_start_config()
            .epoch_data()
            .epoch_start_timestamp(),
        CheckedInputObjects::new_for_replay(InputObjects::new(input_objects)),
        system_object_versions,
        &EmptyUnsettledObjectFunds,
        gas_data,
        epoch_store.reference_gas_price(),
        kind,
        None,
        signer,
        *digest,
        &mut None,
    );
    reexecuted
}
