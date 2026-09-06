// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use sui_macros::sim_test;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    effects::TransactionEffectsAPI,
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

#[sim_test]
async fn test_forwarding_address_deposit() {
    let mut env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut config| {
            config.set_create_forwarding_address_registry_for_testing(true);
            config.set_enable_forwarding_addresses_for_testing(true);
            config.set_forwarding_address_resolve_cost_base_for_testing(52);
            config.set_forwarding_address_resolve_cost_per_byte_for_testing(
                config.obj_access_cost_read_per_byte(),
            );
            config
        }))
        .build()
        .await;
    let master = env.get_sender(0);
    let depositor = env.get_sender(1);
    let initial_shared_version = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .epoch_store_for_testing()
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap()
    });

    let registered_master_id = 7;
    let tag = 42;
    let forwarding_address = ForwardingAddress::derive(registered_master_id, tag);
    let unresolved_address = ForwardingAddress::derive(registered_master_id + 1, tag);
    let amount = 1_000_000;

    let (_, unresolved_effects) =
        send_to_address_balance(&mut env, depositor, unresolved_address, amount).await;
    match unresolved_effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(location, 1),
            ..
        }) => {
            assert_eq!(location.module.name(), FORWARDING_ADDRESS_MODULE_NAME);
        }
        other => panic!("expected an unregistered forwarding-address abort, got {other:?}"),
    }

    let mut builder = ProgrammableTransactionBuilder::new();
    let registry = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let master_id = builder.pure(registered_master_id).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("forwarding_address").unwrap(),
        Identifier::new("register").unwrap(),
        vec![],
        vec![registry, master_id],
    );
    let amount_arg = builder.pure(amount).unwrap();
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("split").unwrap(),
        vec![GAS::type_tag()],
        vec![Argument::GasCoin, amount_arg],
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
    let registration = TransactionData::new_programmable(
        master,
        vec![env.get_gas_for_sender(master)[0]],
        builder.finish(),
        10_000_000,
        env.rgp,
    );
    let (digest, registration_effects) = env.exec_tx_directly(registration).await.unwrap();
    assert!(registration_effects.status().is_ok());
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    // Advance the registry ahead of the depositor's untouched gas object. The implicit registry
    // read must still raise the deposit's Lamport version above the registry version.
    let mut registry_dependency = digest;
    for master_id in 8..11 {
        let (digest, effects) =
            register_master_id(&mut env, master, master_id, initial_shared_version).await;
        assert!(effects.status().is_ok());
        env.cluster.wait_for_tx_settlement(&[digest]).await;
        registry_dependency = digest;
    }

    let (digest, stored_registration_effects) =
        send_to_address_balance(&mut env, depositor, forwarding_address, amount).await;
    assert!(stored_registration_effects.status().is_ok());
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert!(
        stored_registration_effects
            .dependencies()
            .contains(&registry_dependency),
        "the implicit registry read must depend on the transaction that produced its version"
    );
    let registry_version = stored_registration_effects
        .accessed_consensus_objects()
        .into_iter()
        .find_map(|object| {
            let (id, version) = object.id_and_version();
            (id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID).then_some(version)
        })
        .expect("deposit must record its registry read");
    assert!(stored_registration_effects.lamport_version() > registry_version);

    assert_eq!(env.get_sui_balance_ab(master), amount * 2);
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);

    let events = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_transaction_cache_reader()
            .get_events(&digest)
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

async fn register_master_id(
    env: &mut TestEnv,
    master: sui_types::base_types::SuiAddress,
    master_id: u64,
    initial_shared_version: sui_types::base_types::SequenceNumber,
) -> (
    sui_types::base_types::TransactionDigest,
    sui_types::effects::TransactionEffects,
) {
    let mut builder = ProgrammableTransactionBuilder::new();
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
    let gas = env.get_gas_for_sender(sender)[0];
    let transaction = TestTransactionBuilder::new(sender, gas, env.rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
        .build();
    env.exec_tx_directly(transaction).await.unwrap()
}
