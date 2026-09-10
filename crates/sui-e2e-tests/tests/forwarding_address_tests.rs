// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use sui_macros::sim_test;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    base_types::SequenceNumber,
    effects::{TransactionEffects, TransactionEffectsAPI, UnchangedConsensusKind},
    execution_status::{ExecutionFailure, ExecutionFailureStatus, ExecutionStatus},
    forwarding_address::{
        FORWARDING_ADDRESS_MODULE_NAME, FORWARDING_DEPOSIT_STRUCT_NAME, ForwardingAddress,
        ForwardingDeposit,
    },
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    supported_protocol_versions::SupportedProtocolVersions,
    transaction::{Argument, ObjectArg, SharedObjectMutability, TransactionData},
};
use test_cluster::{
    TestClusterBuilder,
    addr_balance_test_env::{TestEnv, TestEnvBuilder},
};

/// The registry must be created by the end-of-epoch transaction when a chain jumps directly from a
/// protocol version without the object to one that enables forwarding. Genesis at protocol version
/// 130 uses the frozen v130 framework snapshot, which predates the `forwarding_address` module.
#[sim_test]
async fn test_create_forwarding_address_registry_object_at_upgrade() {
    let _guard =
        sui_protocol_config::ProtocolConfig::apply_overrides_for_testing(|version, mut config| {
            // The flag is devnet-only; force it on so the test also runs under the mainnet
            // chain override. Only from version 132: at 130 the frozen framework snapshot
            // predates the forwarding_address module, so genesis must not call it.
            if version.as_u64() >= 132 {
                config.set_create_forwarding_address_registry_for_testing(true);
            }
            if version.as_u64() >= 137 {
                config.set_enable_forwarding_addresses_for_testing(true);
                config.set_forwarding_address_resolve_cost_base_for_testing(52);
                config.set_forwarding_address_resolve_cost_per_byte_for_testing(
                    config.obj_access_cost_read_per_byte(),
                );
            }
            config
        });

    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(130.into())
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(130, 137))
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // No node has the registry object yet.
    for h in &handles {
        h.with(|node| {
            assert!(
                node.state()
                    .get_object_cache_reader()
                    .get_latest_object_ref_or_tombstone(SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
                    .is_none()
            );
        });
    }

    test_cluster.wait_for_protocol_version(137.into()).await;
    // A direct jump activates the native before the registry can be created. Forwarding-shaped
    // recipients must fail closed during this epoch instead of receiving funds as ordinary
    // addresses.
    let (sender, gas) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let forwarding_address = ForwardingAddress::derive(7, 42);
    let transaction =
        TestTransactionBuilder::new(sender, gas, test_cluster.get_reference_gas_price().await)
            .transfer_sui_to_address_balance(
                FundSource::coin(gas),
                vec![(1_000_000, forwarding_address)],
            )
            .build();
    let (_, transition_effects) = test_cluster
        .sign_and_execute_transaction_directly(&transaction)
        .await
        .unwrap();
    match transition_effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(location, 1),
            ..
        }) => assert_eq!(location.module.name(), FORWARDING_ADDRESS_MODULE_NAME),
        other => {
            panic!("expected forwarding to fail closed before registry creation, got {other:?}")
        }
    }

    // The registry object is created at the end of the first epoch in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    // Checking through the epoch start config also verifies that the registry's initial
    // shared version is registered there at the start of the following epoch.
    for h in &handles {
        h.with(|node| {
            node.state()
                .epoch_store_for_testing()
                .epoch_start_config()
                .forwarding_address_registry_obj_initial_shared_version()
                .expect("forwarding address registry object should exist");
        });
    }
}

fn forwarding_address_test_env(enable_forwarding_addresses: bool) -> TestEnvBuilder {
    TestEnvBuilder::new().with_proto_override_cb(Box::new(move |_, mut config| {
        config.set_create_forwarding_address_registry_for_testing(true);
        config.set_enable_forwarding_addresses_for_testing(enable_forwarding_addresses);
        config.set_forwarding_address_resolve_cost_base_for_testing(52);
        config.set_forwarding_address_resolve_cost_per_byte_for_testing(
            config.obj_access_cost_read_per_byte(),
        );
        config
    }))
}

fn forwarding_address_registry_initial_shared_version(env: &TestEnv) -> SequenceNumber {
    env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .epoch_store_for_testing()
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .expect("forwarding address registry should be created")
    })
}

fn add_master_id_registration(
    builder: &mut ProgrammableTransactionBuilder,
    master_id: u64,
    initial_shared_version: SequenceNumber,
) {
    let registry = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let master_id = builder.pure(master_id).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("forwarding_address").unwrap(),
        Identifier::new("register").unwrap(),
        vec![],
        vec![registry, master_id],
    );
}

fn add_gas_coin_balance_deposit(
    builder: &mut ProgrammableTransactionBuilder,
    forwarding_address: sui_types::base_types::SuiAddress,
    amount: u64,
) {
    let amount = builder.pure(amount).unwrap();
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("split").unwrap(),
        vec![GAS::type_tag()],
        vec![Argument::GasCoin, amount],
    );
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("into_balance").unwrap(),
        vec![GAS::type_tag()],
        vec![coin],
    );
    let recipient = builder.pure(forwarding_address).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![GAS::type_tag()],
        vec![balance, recipient],
    );
}

fn forwarding_address_deposit_transaction(
    env: &TestEnv,
    sender: sui_types::base_types::SuiAddress,
    recipient: sui_types::base_types::SuiAddress,
    amount: u64,
) -> TransactionData {
    let gas = env.get_gas_for_sender(sender)[0];
    TestTransactionBuilder::new(sender, gas, env.rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
        .build()
}

fn assert_forwarding_deposit_event(
    env: &TestEnv,
    digest: &sui_types::base_types::TransactionDigest,
    forwarding_address: sui_types::base_types::SuiAddress,
    master: sui_types::base_types::SuiAddress,
    amount: u64,
    tag: u128,
) {
    let events = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_transaction_cache_reader()
            .get_events(digest)
            .unwrap()
    });
    assert_eq!(events.data.len(), 1);
    let event = &events.data[0];
    assert_eq!(
        event.type_.module.as_ident_str(),
        FORWARDING_ADDRESS_MODULE_NAME
    );
    assert_eq!(
        event.type_.name.as_ident_str(),
        FORWARDING_DEPOSIT_STRUCT_NAME
    );
    assert_eq!(
        bcs::from_bytes::<ForwardingDeposit>(&event.contents).unwrap(),
        ForwardingDeposit {
            forwarding_address,
            master,
            amount,
            tag,
        }
    );
}

fn assert_no_events(env: &TestEnv, digest: &sui_types::base_types::TransactionDigest) {
    let events = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_transaction_cache_reader()
            .get_events(digest)
    });
    assert!(
        events.is_none_or(|events| events.data.is_empty()),
        "ordinary address-balance routing should not emit forwarding events"
    );
}

fn assert_unregistered_forwarding_address_abort(status: &ExecutionStatus, context: &str) {
    match status {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(location, 1),
            ..
        }) => assert_eq!(location.module.name(), FORWARDING_ADDRESS_MODULE_NAME),
        other => {
            panic!("{context}: expected an unregistered forwarding-address abort, got {other:?}")
        }
    }
}

#[sim_test]
async fn test_forwarding_address_deposit() {
    let mut env = forwarding_address_test_env(true).build().await;
    let master = env.get_sender(0);
    let master_id = 7;
    let tag = 42;
    let forwarding_address = ForwardingAddress::derive(master_id, tag);
    let amount = 1_000_000;
    let initial_master_balance = env.get_sui_balance_ab(master);
    let initial_shared_version = forwarding_address_registry_initial_shared_version(&env);

    let mut builder = ProgrammableTransactionBuilder::new();
    add_master_id_registration(&mut builder, master_id, initial_shared_version);
    add_gas_coin_balance_deposit(&mut builder, forwarding_address, amount);
    let transaction = TransactionData::new_programmable(
        master,
        vec![env.get_gas_for_sender(master)[0]],
        builder.finish(),
        10_000_000,
        env.rgp,
    );
    let simulated_effects =
        simulate_forwarding_deposit_and_check_registry(&env, &transaction).await;
    assert!(simulated_effects.status().is_ok());

    let (digest, effects) = env.exec_tx_directly(transaction).await.unwrap();
    assert!(effects.status().is_ok());
    env.cluster.wait_for_tx_settlement(&[digest]).await;

    assert_eq!(
        env.get_sui_balance_ab(master),
        initial_master_balance + amount
    );
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
    assert_forwarding_deposit_event(&env, &digest, forwarding_address, master, amount, tag);
    let depositor = env.get_sender(1);
    let mut registry_dependency = digest;
    for additional_master_id in 8..11 {
        let (registration, effects) = register_master_id(
            &mut env,
            master,
            additional_master_id,
            initial_shared_version,
        )
        .await;
        assert!(effects.status().is_ok());
        env.cluster.wait_for_tx_settlement(&[registration]).await;
        registry_dependency = registration;
    }

    let (stored_deposit, effects) =
        send_to_address_balance(&mut env, depositor, forwarding_address, amount).await;
    assert!(effects.status().is_ok());
    env.cluster.wait_for_tx_settlement(&[stored_deposit]).await;
    assert!(
        effects.dependencies().contains(&registry_dependency),
        "the implicit registry read must depend on the transaction that produced its version"
    );
    let registry_version = effects
        .accessed_consensus_objects()
        .into_iter()
        .find_map(|object| {
            let (id, version) = object.id_and_version();
            (id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID).then_some(version)
        })
        .expect("deposit must record its registry read");
    assert!(effects.lamport_version() > registry_version);

    assert_eq!(
        env.get_sui_balance_ab(master),
        initial_master_balance + amount * 2
    );
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
    assert_forwarding_deposit_event(
        &env,
        &stored_deposit,
        forwarding_address,
        master,
        amount,
        tag,
    );

    let master_balance_before_gas_coin_send = env.get_sui_balance_ab(master);
    let mut builder = ProgrammableTransactionBuilder::new();
    let recipient = builder.pure(forwarding_address).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![GAS::type_tag()],
        vec![Argument::GasCoin, recipient],
    );
    let direct_gas_coin_send = TransactionData::new_programmable(
        depositor,
        vec![env.get_gas_for_sender(depositor)[0]],
        builder.finish(),
        10_000_000,
        env.rgp,
    );
    let (_, effects) = env.exec_tx_directly(direct_gas_coin_send).await.unwrap();
    assert!(matches!(
        effects.status(),
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::FeatureNotYetSupported,
            ..
        })
    ));
    assert_eq!(
        env.get_sui_balance_ab(master),
        master_balance_before_gas_coin_send
    );
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
}
#[sim_test]
async fn test_duplicate_forwarding_address_registration_preserves_first_master() {
    let mut env = forwarding_address_test_env(true).build().await;
    let first_master = env.get_sender(0);
    let duplicate_registrant = env.get_sender(1);
    let depositor = env.get_sender(2);
    let master_id = 7;
    let tag = 42;
    let forwarding_address = ForwardingAddress::derive(master_id, tag);

    let amount = 1_000_000;
    let initial_shared_version = forwarding_address_registry_initial_shared_version(&env);

    let (first_registration, effects) =
        register_master_id(&mut env, first_master, master_id, initial_shared_version).await;
    assert!(effects.status().is_ok());
    env.cluster
        .wait_for_tx_settlement(&[first_registration])
        .await;

    let (_, duplicate_effects) = register_master_id(
        &mut env,
        duplicate_registrant,
        master_id,
        initial_shared_version,
    )
    .await;
    match duplicate_effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(location, 0),
            ..
        }) => assert_eq!(location.module.name().as_str(), "dynamic_field"),
        other => panic!(
            "expected duplicate registration to abort with EFieldAlreadyExists, got {other:?}"
        ),
    }

    let unregistered_address = ForwardingAddress::derive(master_id + 1, tag);
    let (unregistered_digest, unregistered_effects) =
        send_to_address_balance(&mut env, depositor, unregistered_address, amount).await;
    assert_unregistered_forwarding_address_abort(
        unregistered_effects.status(),
        "unregistered forwarding deposit",
    );
    env.cluster
        .wait_for_tx_settlement(&[unregistered_digest])
        .await;
    assert_eq!(env.get_sui_balance_ab(unregistered_address), 0);
    assert!(forwarding_address_registry_read_only_version(&unregistered_effects).is_some());
    assert_eq!(env.get_sui_balance_ab(first_master), 0);
    assert_eq!(env.get_sui_balance_ab(duplicate_registrant), 0);
    assert_no_events(&env, &unregistered_digest);

    let (digest, effects) =
        send_to_address_balance(&mut env, depositor, forwarding_address, amount).await;
    assert!(effects.status().is_ok());
    env.cluster.wait_for_tx_settlement(&[digest]).await;

    assert_eq!(env.get_sui_balance_ab(first_master), amount);
    assert_eq!(env.get_sui_balance_ab(duplicate_registrant), 0);
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
    assert_forwarding_deposit_event(&env, &digest, forwarding_address, first_master, amount, tag);

    let fullnode = env.cluster.spawn_new_fullnode().await.sui_node;
    let replayed_effects = fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(
            "test_duplicate_forwarding_address_registration_preserves_first_master",
            &[first_registration, digest],
        )
        .await;
    assert_eq!(
        &replayed_effects[1], &effects,
        "a fresh fullnode must reproduce the forwarding deposit effects"
    );
}

#[sim_test]
async fn test_forwarding_address_registry_without_feature_routes_to_address_balance() {
    let mut env = forwarding_address_test_env(false).build().await;
    let sender = env.get_sender(0);
    let forwarding_address = ForwardingAddress::derive(7, 42);
    let amount = 1_000_000;

    forwarding_address_registry_initial_shared_version(&env);
    let (digest, effects) =
        send_to_address_balance(&mut env, sender, forwarding_address, amount).await;
    assert!(effects.status().is_ok());
    env.cluster.wait_for_tx_settlement(&[digest]).await;

    assert_eq!(env.get_sui_balance_ab(forwarding_address), amount);
    assert!(forwarding_address_registry_read_only_version(&effects).is_none());
    assert_no_events(&env, &digest);
}

fn forwarding_address_registry_read_only_version(
    effects: &TransactionEffects,
) -> Option<SequenceNumber> {
    effects
        .unchanged_consensus_objects()
        .iter()
        .find_map(|(id, kind)| match kind {
            UnchangedConsensusKind::ReadOnlyRoot((version, _))
                if *id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID =>
            {
                Some(*version)
            }
            _ => None,
        })
}

async fn simulate_forwarding_deposit_and_check_registry(
    env: &TestEnv,
    transaction: &TransactionData,
) -> TransactionEffects {
    use sui_rpc::field::FieldMaskUtil;
    use sui_rpc::proto::sui::rpc::v2 as proto;

    let mut request = proto::SimulateTransactionRequest::default()
        .with_transaction(
            proto::Transaction::default().with_bcs(proto::Bcs::serialize(transaction).unwrap()),
        )
        .with_read_mask(prost_types::FieldMask::from_paths(["*"]));
    request.set_checks(proto::simulate_transaction_request::TransactionChecks::Disabled);
    request.set_do_gas_selection(false);
    let response = env
        .cluster
        .grpc_client()
        .into_inner()
        .execution_client()
        .simulate_transaction(request)
        .await
        .unwrap()
        .into_inner();
    let simulated_transaction = response.transaction.unwrap();
    let effects: TransactionEffects = simulated_transaction
        .effects
        .unwrap()
        .bcs
        .unwrap()
        .deserialize()
        .unwrap();
    let registry_version = effects
        .accessed_consensus_objects()
        .into_iter()
        .find_map(|object| {
            let (id, version) = object.id_and_version();
            (id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID).then_some(version)
        })
        .expect("forwarding deposits must record their explicit or implicit registry input");
    let registry_id = SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID.to_string();
    assert!(
        simulated_transaction
            .objects
            .unwrap()
            .objects
            .iter()
            .any(|object| {
                object.object_id() == registry_id && object.version() == registry_version.value()
            }),
        "simulation must return the forwarding registry at the version referenced by its effects"
    );
    effects
}

#[sim_test]
async fn test_simulate_forwarding_address_deposits_return_read_only_registry() {
    let mut env = forwarding_address_test_env(true).build().await;
    let master = env.get_sender(0);
    let depositor = env.get_sender(1);
    let master_id = 7;
    let registered_address = ForwardingAddress::derive(master_id, 42);
    let unregistered_address = ForwardingAddress::derive(master_id + 1, 42);
    let amount = 1_000_000;

    let initial_shared_version = forwarding_address_registry_initial_shared_version(&env);
    let (registration, effects) =
        register_master_id(&mut env, master, master_id, initial_shared_version).await;
    assert!(effects.status().is_ok());
    env.cluster.wait_for_tx_settlement(&[registration]).await;

    let registered_transaction =
        forwarding_address_deposit_transaction(&env, depositor, registered_address, amount);
    let registered_effects =
        simulate_forwarding_deposit_and_check_registry(&env, &registered_transaction).await;
    assert!(registered_effects.status().is_ok());
    assert!(forwarding_address_registry_read_only_version(&registered_effects).is_some());

    let unregistered_transaction =
        forwarding_address_deposit_transaction(&env, depositor, unregistered_address, amount);
    let unregistered_effects =
        simulate_forwarding_deposit_and_check_registry(&env, &unregistered_transaction).await;
    assert_unregistered_forwarding_address_abort(
        unregistered_effects.status(),
        "simulated unregistered forwarding deposit",
    );
    assert!(forwarding_address_registry_read_only_version(&unregistered_effects).is_some());
}

async fn register_master_id(
    env: &mut TestEnv,
    master: sui_types::base_types::SuiAddress,
    master_id: u64,
    initial_shared_version: SequenceNumber,
) -> (
    sui_types::base_types::TransactionDigest,
    sui_types::effects::TransactionEffects,
) {
    let mut builder = ProgrammableTransactionBuilder::new();
    add_master_id_registration(&mut builder, master_id, initial_shared_version);
    let transaction = TransactionData::new_programmable(
        master,
        vec![env.get_gas_for_sender(master)[0]],
        builder.finish(),
        10_000_000,
        env.rgp,
    );
    env.exec_tx_directly(transaction).await.unwrap()
}

async fn send_to_address_balance(
    env: &mut TestEnv,
    sender: sui_types::base_types::SuiAddress,
    recipient: sui_types::base_types::SuiAddress,
    amount: u64,
) -> (
    sui_types::base_types::TransactionDigest,
    sui_types::effects::TransactionEffects,
) {
    let transaction = forwarding_address_deposit_transaction(env, sender, recipient, amount);
    env.exec_tx_directly(transaction).await.unwrap()
}
