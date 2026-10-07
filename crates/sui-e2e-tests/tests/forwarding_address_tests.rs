// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, UnchangedConsensusKind},
    execution_status::{ExecutionFailure, ExecutionFailureStatus, ExecutionStatus},
    forwarding_address::{
        FORWARDING_ADDRESS_MODULE_NAME, FORWARDING_ADDRESS_PAYLOAD_LENGTH,
        FORWARDING_DEPOSIT_STRUCT_NAME, FORWARDING_TRANSFER_STRUCT_NAME, ForwardingAddress,
        ForwardingDeposit, ForwardingTransfer, MASTER_REGISTERED_STRUCT_NAME, MasterRecord,
        MasterRecordKey, MasterRegistered, RotationFinalized,
    },
    gas_coin::GAS,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    supported_protocol_versions::SupportedProtocolVersions,
    transaction::{Argument, CallArg, ObjectArg, SharedObjectMutability, TransactionData},
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

/// The production registration price, in internal gas units (1M gas units).
const REGISTER_COST: u64 = 1_000_000_000;
const REGISTER_COST_GAS_UNITS: u64 = REGISTER_COST / 1000;

/// Starts with the high bit set so the tests cover a payload no integer encoding would produce.
const PAYLOAD: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH] = [
    0x80, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0xff,
];
/// Rotation delay every registration in these tests uses.
const ROTATION_DELAY_EPOCHS: u64 = 1;
/// The mixed id the registry assigns to its first registration (the Move unit test
/// `master_id_mixing_is_invertible_and_keeps_zero_reserved` pins the same constant).
const FIRST_MASTER_ID: u64 = 0x52ca8647179c;

/// Forwarding ships in two steps: one release creates the registry, a later one enables
/// resolution. The registry must exist by the first epoch that enables forwarding, and a
/// registration and a deposit in that epoch must work. Genesis at protocol version 130 uses the
/// frozen v130 framework snapshot, which predates the `forwarding_address` module. Epochs are
/// closed explicitly so no transaction lands in a reconfiguration window.
#[sim_test]
async fn test_forwarding_address_upgrade_creates_registry_before_enabling() {
    let mut env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|version, mut config| {
            // The registry is created from protocol version 132 onwards. This matches the
            // production configuration, but we need to set it explicitly here because
            // apply_overrides_for_testing may be used to override the chain to Unknown.
            if version.as_u64() >= 132 {
                config.set_create_forwarding_address_registry_for_testing(true);
            }
            if version.as_u64() >= 139 {
                set_forwarding_address_config_for_testing(&mut config);
            }
            config
        }))
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder
                .with_protocol_version(130.into())
                .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                    130, 138,
                ))
                .with_fullnode_supported_protocol_versions_config(
                    SupportedProtocolVersions::new_for_testing(130, 139),
                )
        }))
        .build()
        .await;

    for h in env.cluster.all_node_handles() {
        h.with(|node| {
            assert!(
                node.state()
                    .get_object_cache_reader()
                    .get_object(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
                    .is_none(),
                "registry should not exist before the upgrade"
            );
        });
    }

    // The first epoch at 138 (the first version with the registry) creates the registry at its
    // end, so it is in the epoch start configuration from the following epoch on.
    reconfigure_until_protocol_version(&env, 138).await;
    env.cluster.trigger_reconfiguration().await;
    for h in env.cluster.all_node_handles() {
        h.with(|node| {
            node.state()
                .epoch_store_for_testing()
                .epoch_start_config()
                .forwarding_address_registry_obj_initial_shared_version()
                .expect("forwarding address registry object should exist");
        });
    }

    env.cluster
        .update_validator_supported_versions(SupportedProtocolVersions::new_for_testing(130, 139))
        .await;
    reconfigure_until_protocol_version(&env, 139).await;
    env.update_all_gas().await;

    let master = env.get_sender(0);
    let depositor = env.get_sender(1);
    let amount = 1_000_000;
    let initial_master_balance = env.get_sui_balance_ab(master);
    let registration = register_master(&mut env, master).await;
    let forwarding_address = ForwardingAddress::derive_opaque(registration.master_id, PAYLOAD);
    let transaction =
        forwarding_address_deposit_transaction(&env, depositor, forwarding_address, amount);
    let (digest, effects) = env.exec_tx_directly(transaction).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert_eq!(
        env.get_sui_balance_ab(master),
        initial_master_balance + amount
    );
    assert_forwarding_deposit_event(&env, &digest, forwarding_address, master, amount);
}

/// Validators announce their supported versions through consensus, so the upgrade happens at the
/// first epoch change after a quorum of announcements has landed, which may take more than one
/// epoch after a restart.
async fn reconfigure_until_protocol_version(env: &TestEnv, version: u64) {
    for _ in 0..5 {
        env.cluster.trigger_reconfiguration().await;
        let versions: Vec<u64> = env
            .cluster
            .all_node_handles()
            .iter()
            .map(|h| {
                h.with(|node| {
                    node.state()
                        .epoch_store_for_testing()
                        .protocol_config()
                        .version
                        .as_u64()
                })
            })
            .collect();
        if versions.iter().all(|v| *v == version) {
            return;
        }
        assert!(
            versions.iter().all(|v| *v < version),
            "protocol version passed {version}: {versions:?}"
        );
    }
    panic!("protocol version did not reach {version} within 5 epochs");
}

fn set_forwarding_address_config_for_testing(config: &mut ProtocolConfig) {
    config.set_enable_forwarding_addresses_for_testing(true);
    config.set_forwarding_address_resolve_cost_base_for_testing(52);
    config.set_forwarding_address_resolve_lookup_cost_base_for_testing(
        512 * config.obj_access_cost_read_per_byte(),
    );
    config.set_forwarding_address_register_cost_base_for_testing(REGISTER_COST);
    config.set_forwarding_address_max_variant_for_testing(0);
    config.set_forwarding_address_max_hops_for_testing(3);
}

fn forwarding_address_test_env(enable_forwarding_addresses: bool) -> TestEnvBuilder {
    TestEnvBuilder::new().with_proto_override_cb(Box::new(move |_, mut config| {
        config.set_create_forwarding_address_registry_for_testing(true);
        set_forwarding_address_config_for_testing(&mut config);
        config.set_enable_forwarding_addresses_for_testing(enable_forwarding_addresses);
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

struct Registration {
    digest: TransactionDigest,
    master_id: u64,
    cap_id: ObjectID,
}

/// Registers `master` and transfers the returned `MasterCap` back to it.
async fn register_master(env: &mut TestEnv, master: SuiAddress) -> Registration {
    let initial_shared_version = forwarding_address_registry_initial_shared_version(env);
    let mut builder = ProgrammableTransactionBuilder::new();
    let registry = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let rotation_delay = builder.pure(ROTATION_DELAY_EPOCHS).unwrap();
    let cap = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("forwarding_address").unwrap(),
        Identifier::new("register").unwrap(),
        vec![],
        vec![registry, rotation_delay],
    );
    builder.transfer_arg(master, cap);
    let transaction = TransactionData::new_programmable(
        master,
        vec![env.get_gas_for_sender(master)[0]],
        builder.finish(),
        register_gas_budget(env),
        env.rgp,
    );
    let (digest, effects) = env.exec_tx_directly(transaction).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    let computation_cost = effects.gas_cost_summary().computation_cost;
    assert!(
        computation_cost >= REGISTER_COST_GAS_UNITS * env.rgp,
        "registration must pay the registration fee: {computation_cost}"
    );
    env.cluster.wait_for_tx_settlement(&[digest]).await;

    let events = get_events(env, &digest);
    let registered = events
        .iter()
        .find(|event| event.type_.name.as_ident_str() == MASTER_REGISTERED_STRUCT_NAME)
        .expect("register must emit MasterRegistered");
    let registered: MasterRegistered = bcs::from_bytes(&registered.contents).unwrap();
    assert_eq!(registered.master, master);
    assert_ne!(registered.master_id, 0, "master ID 0 is reserved");

    let (cap, record) = env.cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let cap = state
            .get_object_cache_reader()
            .get_object(&registered.cap_id)
            .expect("MasterCap must exist");
        let registry_version = state
            .get_object_cache_reader()
            .get_object(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
            .expect("registry must exist")
            .version();
        let record = MasterRecordKey(registered.master_id)
            .load(
                state.get_runtime_object_resolver().as_ref(),
                registry_version,
            )
            .unwrap();
        (cap, record)
    });
    assert_eq!(cap.owner, Owner::AddressOwner(master));
    assert!(
        cap.type_()
            .is_some_and(|t| t.name().as_str() == "MasterCap"),
        "{cap:?}"
    );
    assert_eq!(
        record,
        Some(MasterRecord {
            master,
            paused: false,
            pending: None,
            rotation_delay_epochs: ROTATION_DELAY_EPOCHS,
        })
    );

    Registration {
        digest,
        master_id: registered.master_id,
        cap_id: registered.cap_id,
    }
}

fn register_gas_budget(env: &TestEnv) -> u64 {
    (REGISTER_COST_GAS_UNITS + 10_000) * env.rgp
}

fn get_events(env: &TestEnv, digest: &TransactionDigest) -> Vec<sui_types::event::Event> {
    env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_transaction_cache_reader()
            .get_events(digest)
            .map(|events| events.data)
            .unwrap_or_default()
    })
}

fn add_gas_coin_balance_deposit(
    builder: &mut ProgrammableTransactionBuilder,
    forwarding_address: SuiAddress,
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
    sender: SuiAddress,
    recipient: SuiAddress,
    amount: u64,
) -> TransactionData {
    let gas = env.get_gas_for_sender(sender)[0];
    TestTransactionBuilder::new(sender, gas, env.rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
        .build()
}

fn assert_forwarding_deposit_event(
    env: &TestEnv,
    digest: &TransactionDigest,
    forwarding_address: SuiAddress,
    master: SuiAddress,
    amount: u64,
) {
    let events = get_events(env, digest);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_forwarding_deposit(&events[0], forwarding_address, master, amount);
}

fn assert_forwarding_deposit(
    event: &sui_types::event::Event,
    forwarding_address: SuiAddress,
    master: SuiAddress,
    amount: u64,
) {
    assert_eq!(
        event.type_.module.as_ident_str(),
        FORWARDING_ADDRESS_MODULE_NAME
    );
    assert_eq!(
        event.type_.name.as_ident_str(),
        FORWARDING_DEPOSIT_STRUCT_NAME
    );
    let deposit = bcs::from_bytes::<ForwardingDeposit>(&event.contents).unwrap();
    assert_eq!(
        deposit,
        ForwardingDeposit {
            forwarding_address,
            master,
            amount,
        }
    );
    assert_eq!(
        ForwardingAddress::parse(deposit.forwarding_address).map(|parsed| parsed.payload),
        Some(PAYLOAD),
        "the event carries the whole address, so the payload survives untouched"
    );
}

fn assert_no_events(env: &TestEnv, digest: &TransactionDigest) {
    assert!(
        get_events(env, digest).is_empty(),
        "ordinary address-balance routing should not emit forwarding events"
    );
}

/// Resolution runs at the end of execution, so an unresolvable forwarding address fails the
/// whole transaction with an execution error carrying no command index.
fn assert_forwarding_unresolvable(status: &ExecutionStatus, context: &str) {
    assert!(
        matches!(
            status,
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionFailureStatus::FeatureNotYetSupported,
                command: None,
            })
        ),
        "{context}: expected an unresolvable forwarding address, got {status:?}"
    );
}

#[sim_test]
async fn test_forwarding_address_deposit() {
    let mut env = forwarding_address_test_env(true).build().await;
    let master = env.get_sender(0);
    let depositor = env.get_sender(1);
    let amount = 1_000_000;
    let initial_master_balance = env.get_sui_balance_ab(master);

    let registration = register_master(&mut env, master).await;
    let forwarding_address = ForwardingAddress::derive_opaque(registration.master_id, PAYLOAD);

    // A deposit from the master itself, simulated first so the registry read is visible to
    // dry-run, then executed.
    let mut builder = ProgrammableTransactionBuilder::new();
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
    assert!(simulated_effects.status().is_ok(), "{simulated_effects:?}");

    let (digest, effects) = env.exec_tx_directly(transaction).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert_eq!(
        env.get_sui_balance_ab(master),
        initial_master_balance + amount
    );
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
    assert_forwarding_deposit_event(&env, &digest, forwarding_address, master, amount);

    // Deposits from another sender keep resolving after later registrations advance the registry.
    for _ in 0..3 {
        register_master(&mut env, depositor).await;
    }
    let (stored_deposit, effects) =
        send_to_address_balance(&mut env, depositor, forwarding_address, amount).await;
    assert!(effects.status().is_ok(), "{effects:?}");
    env.cluster.wait_for_tx_settlement(&[stored_deposit]).await;
    assert!(forwarding_address_registry_read_only_version(&effects).is_some());
    assert_eq!(
        env.get_sui_balance_ab(master),
        initial_master_balance + amount * 2
    );
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
    assert_forwarding_deposit_event(&env, &stored_deposit, forwarding_address, master, amount);

    // Sending the gas coin itself to a forwarding address fails, registered or not: the master
    // would net the coin's value minus gas, which no ForwardingDeposit can state.
    let master_balance_before_gas_coin_send = env.get_sui_balance_ab(master);
    for recipient in [
        ForwardingAddress::derive(registration.master_id, 1, PAYLOAD),
        forwarding_address,
    ] {
        let mut builder = ProgrammableTransactionBuilder::new();
        let recipient_arg = builder.pure(recipient).unwrap();
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("send_funds").unwrap(),
            vec![GAS::type_tag()],
            vec![Argument::GasCoin, recipient_arg],
        );
        let gas = env.get_gas_for_sender(depositor)[0];
        let direct_gas_coin_send = TransactionData::new_programmable(
            depositor,
            vec![gas],
            builder.finish(),
            10_000_000,
            env.rgp,
        );
        let (digest, effects) = env.exec_tx_directly(direct_gas_coin_send).await.unwrap();
        assert_forwarding_unresolvable(effects.status(), "gas coin to a forwarding address");
        env.cluster.wait_for_tx_settlement(&[digest]).await;
    }
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
    assert_eq!(
        env.get_sui_balance_ab(master),
        master_balance_before_gas_coin_send
    );
}

#[sim_test]
async fn test_registrations_get_distinct_ids_and_route_separately() {
    let mut env = forwarding_address_test_env(true).build().await;
    let first_master = env.get_sender(0);
    let second_master = env.get_sender(1);
    let depositor = env.get_sender(2);
    let amount = 1_000_000;

    let first = register_master(&mut env, first_master).await;
    let second = register_master(&mut env, second_master).await;
    assert_ne!(first.master_id, second.master_id);
    assert_ne!(first.cap_id, second.cap_id);
    assert_ne!(first.digest, second.digest);

    // Both masters derive addresses with the same payload; deposits still route by master ID.
    let first_address = ForwardingAddress::derive_opaque(first.master_id, PAYLOAD);
    let second_address = ForwardingAddress::derive_opaque(second.master_id, PAYLOAD);
    assert_ne!(first_address, second_address);

    let (digest, effects) =
        send_to_address_balance(&mut env, depositor, second_address, amount).await;
    assert!(effects.status().is_ok(), "{effects:?}");
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert_eq!(env.get_sui_balance_ab(second_master), amount);
    assert_eq!(env.get_sui_balance_ab(first_master), 0);
    assert_eq!(env.get_sui_balance_ab(second_address), 0);
    assert_forwarding_deposit_event(&env, &digest, second_address, second_master, amount);

    let fullnode = env.cluster.spawn_new_fullnode().await.sui_node;
    let replayed_effects = fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(
            "test_registrations_get_distinct_ids_and_route_separately",
            &[first.digest, second.digest, digest],
        )
        .await;
    assert_eq!(
        &replayed_effects[2], &effects,
        "a fresh fullnode must reproduce the forwarding deposit effects"
    );
}

#[sim_test]
async fn test_malformed_forwarding_addresses_abort_instead_of_stranding() {
    let mut env = forwarding_address_test_env(true).build().await;
    let master = env.get_sender(0);
    let depositor = env.get_sender(1);
    let amount = 1_000_000;

    let registration = register_master(&mut env, master).await;

    // (recipient, whether resolution reads the registry before failing, context)
    let cases = [
        (
            ForwardingAddress::derive_opaque(0, PAYLOAD),
            true,
            "reserved master ID 0",
        ),
        (
            ForwardingAddress::derive(registration.master_id, 1, PAYLOAD),
            false,
            "variant 1 above the protocol maximum",
        ),
    ];
    for (recipient, reads_registry, context) in cases {
        let (digest, effects) =
            send_to_address_balance(&mut env, depositor, recipient, amount).await;
        assert_forwarding_unresolvable(effects.status(), context);
        env.cluster.wait_for_tx_settlement(&[digest]).await;
        assert_eq!(env.get_sui_balance_ab(recipient), 0, "{context}");
        assert_eq!(env.get_sui_balance_ab(master), 0, "{context}");
        assert!(
            forwarding_address_registry_read_only_version(&effects).is_some() == reads_registry,
            "{context}: only a registry lookup records the registry read: {effects:?}"
        );
        assert_no_events(&env, &digest);
    }
}

/// Registering and depositing in one transaction: resolution reads the registry as of the version
/// consensus assigned, so it does not see the record `register` wrote earlier in the same
/// transaction, and the whole transaction fails.
#[sim_test]
async fn test_register_and_deposit_in_the_same_transaction_fails() {
    let mut env = forwarding_address_test_env(true).build().await;
    let master = env.get_sender(0);
    let amount = 1_000_000;
    let initial_master_balance = env.get_sui_balance_ab(master);
    let forwarding_address = ForwardingAddress::derive_opaque(FIRST_MASTER_ID, PAYLOAD);

    let initial_shared_version = forwarding_address_registry_initial_shared_version(&env);
    let mut builder = ProgrammableTransactionBuilder::new();
    let registry = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let rotation_delay = builder.pure(ROTATION_DELAY_EPOCHS).unwrap();
    let cap = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("forwarding_address").unwrap(),
        Identifier::new("register").unwrap(),
        vec![],
        vec![registry, rotation_delay],
    );
    builder.transfer_arg(master, cap);
    add_gas_coin_balance_deposit(&mut builder, forwarding_address, amount);
    let transaction = TransactionData::new_programmable(
        master,
        vec![env.get_gas_for_sender(master)[0]],
        builder.finish(),
        register_gas_budget(&env),
        env.rgp,
    );
    let (digest, effects) = env.exec_tx_directly(transaction).await.unwrap();
    assert_forwarding_unresolvable(
        effects.status(),
        "deposit to an id registered in the same transaction",
    );
    env.cluster.wait_for_tx_settlement(&[digest]).await;

    assert_eq!(env.get_sui_balance_ab(master), initial_master_balance);
    assert_eq!(env.get_sui_balance_ab(forwarding_address), 0);
    assert!(get_events(&env, &digest).is_empty());

    // The aborted registration allocated nothing, so the first id is still available.
    let registration = register_master(&mut env, master).await;
    assert_eq!(registration.master_id, FIRST_MASTER_ID);
}

#[sim_test]
async fn test_forwarding_address_registry_without_feature_routes_to_address_balance() {
    let mut env = forwarding_address_test_env(false).build().await;
    let sender = env.get_sender(0);
    let forwarding_address = ForwardingAddress::derive_opaque(7, PAYLOAD);
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
    let amount = 1_000_000;

    let registration = register_master(&mut env, master).await;
    let registered_address = ForwardingAddress::derive_opaque(registration.master_id, PAYLOAD);
    let unregistered_address = ForwardingAddress::derive_opaque(0, PAYLOAD);

    let registered_transaction =
        forwarding_address_deposit_transaction(&env, depositor, registered_address, amount);
    let registered_effects =
        simulate_forwarding_deposit_and_check_registry(&env, &registered_transaction).await;
    assert!(
        registered_effects.status().is_ok(),
        "{registered_effects:?}"
    );
    assert!(forwarding_address_registry_read_only_version(&registered_effects).is_some());

    let unregistered_transaction =
        forwarding_address_deposit_transaction(&env, depositor, unregistered_address, amount);
    let unregistered_effects =
        simulate_forwarding_deposit_and_check_registry(&env, &unregistered_transaction).await;
    assert_forwarding_unresolvable(
        unregistered_effects.status(),
        "simulated unregistered forwarding deposit",
    );
    assert!(forwarding_address_registry_read_only_version(&unregistered_effects).is_some());
}

async fn send_to_address_balance(
    env: &mut TestEnv,
    sender: SuiAddress,
    recipient: SuiAddress,
    amount: u64,
) -> (TransactionDigest, TransactionEffects) {
    let transaction = forwarding_address_deposit_transaction(env, sender, recipient, amount);
    env.exec_tx_directly(transaction).await.unwrap()
}

/// A PTB that withdraws `amount` from the shared object-balance vault and deposits it to
/// `recipient`, optionally registering a master for the sender first. With `register`, the
/// registry is a mutated input as well as read for resolution, and the accumulator root is read
/// for the object balance.
fn vault_withdraw_to_transaction(
    env: &TestEnv,
    sender: SuiAddress,
    vault: (ObjectID, ObjectID, SequenceNumber),
    amount: u64,
    recipient: SuiAddress,
    register: bool,
) -> TransactionData {
    let (package_id, vault_id, vault_initial_shared_version) = vault;
    let mut builder = ProgrammableTransactionBuilder::new();
    if register {
        let registry = builder
            .obj(ObjectArg::SharedObject {
                id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version: forwarding_address_registry_initial_shared_version(env),
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();
        let rotation_delay = builder.pure(ROTATION_DELAY_EPOCHS).unwrap();
        let cap = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("forwarding_address").unwrap(),
            Identifier::new("register").unwrap(),
            vec![],
            vec![registry, rotation_delay],
        );
        builder.transfer_arg(sender, cap);
    }
    let vault = builder
        .obj(ObjectArg::SharedObject {
            id: vault_id,
            initial_shared_version: vault_initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let amount = builder.pure(amount).unwrap();
    let balance = builder.programmable_move_call(
        package_id,
        Identifier::new("object_balance").unwrap(),
        Identifier::new("withdraw_funds").unwrap(),
        vec![GAS::type_tag()],
        vec![vault, amount],
    );
    let recipient = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![GAS::type_tag()],
        vec![balance, recipient],
    );
    TransactionData::new_programmable(
        sender,
        vec![env.get_gas_for_sender(sender)[0]],
        builder.finish(),
        register_gas_budget(env),
        env.rgp,
    )
}

async fn setup_shared_vault(
    env: &mut TestEnv,
    amount: u64,
) -> (ObjectID, ObjectID, SequenceNumber) {
    let sender = env.get_sender(0);
    let tx = env
        .tx_builder(sender)
        .publish_examples("object_balance")
        .await
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    let package_id = effects
        .created()
        .into_iter()
        .find(|(_, owner)| owner.is_immutable())
        .unwrap()
        .0
        .0;
    let tx = env
        .tx_builder(sender)
        .move_call(package_id, "object_balance", "new_shared", vec![])
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    let (vault_ref, vault_owner) = effects.created().into_iter().next().unwrap();
    let Owner::Shared {
        initial_shared_version,
    } = vault_owner
    else {
        panic!("vault must be shared, got {vault_owner:?}");
    };
    let gas = env.get_sender_and_gas(0).1;
    let tx = env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, vault_ref.0.into())])
        .build();
    env.exec_tx_directly(tx).await.unwrap();
    env.trigger_reconfiguration().await;
    (package_id, vault_ref.0, initial_shared_version)
}

/// Object funds withdrawals deposited to forwarding addresses, in the same consensus commit as
/// registrations: some transactions mutate the registry and read it for resolution while also
/// reading the accumulator root for the object balance, and others only read both.
#[sim_test]
async fn test_object_funds_forwarded_alongside_registrations_in_one_commit() {
    let mut env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut config| {
            config.set_create_forwarding_address_registry_for_testing(true);
            set_forwarding_address_config_for_testing(&mut config);
            config.set_enable_object_funds_withdraw_for_testing(true);
            config.set_check_object_funds_withdraw_in_execution_for_testing(true);
            config
        }))
        .build()
        .await;
    let master = env.get_sender(0);
    let registration = register_master(&mut env, master).await;
    let forwarding_address = ForwardingAddress::derive_opaque(registration.master_id, PAYLOAD);
    let vault = setup_shared_vault(&mut env, 1000).await;
    let initial_master_balance = env.get_sui_balance_ab(master);

    // Registering withdrawers and a plain withdrawer, all resolving to the master, within the
    // vault balance: every transaction succeeds.
    let transactions = [
        vault_withdraw_to_transaction(
            &env,
            env.get_sender(1),
            vault,
            300,
            forwarding_address,
            true,
        ),
        vault_withdraw_to_transaction(
            &env,
            env.get_sender(2),
            vault,
            300,
            forwarding_address,
            false,
        ),
        vault_withdraw_to_transaction(
            &env,
            env.get_sender(3),
            vault,
            300,
            forwarding_address,
            true,
        ),
    ];
    let results = env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&transactions)
        .await
        .unwrap();
    env.update_all_gas().await;
    for (digest, effects) in &results {
        assert!(effects.status().is_ok(), "{effects:?}");
        let events = get_events(&env, digest);
        let deposit = events
            .iter()
            .find(|event| event.type_.name.as_ident_str() == FORWARDING_DEPOSIT_STRUCT_NAME)
            .expect("each withdrawal must emit ForwardingDeposit");
        assert_forwarding_deposit(deposit, forwarding_address, master, 300);
    }
    let digests: Vec<_> = results.iter().map(|(digest, _)| *digest).collect();
    env.cluster.wait_for_tx_settlement(&digests).await;
    assert_eq!(env.get_sui_balance_ab(master), initial_master_balance + 900);

    // Overdrawing the remaining 100 in the same commit as a registration: the registering
    // transaction fails on the object balance, and its registration and deposit roll back.
    let transactions = [
        vault_withdraw_to_transaction(
            &env,
            env.get_sender(1),
            vault,
            600,
            forwarding_address,
            true,
        ),
        vault_withdraw_to_transaction(
            &env,
            env.get_sender(2),
            vault,
            100,
            forwarding_address,
            false,
        ),
    ];
    let results = env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&transactions)
        .await
        .unwrap();
    env.update_all_gas().await;
    assert!(results[0].1.status().is_err(), "{:?}", results[0].1);
    assert!(get_events(&env, &results[0].0).is_empty());
    assert!(results[1].1.status().is_ok(), "{:?}", results[1].1);
    let digests: Vec<_> = results.iter().map(|(digest, _)| *digest).collect();
    env.cluster.wait_for_tx_settlement(&digests).await;
    assert_eq!(
        env.get_sui_balance_ab(master),
        initial_master_balance + 1000
    );
}

/// A master id's lifecycle on a real cluster: pausing with the cap makes deposits fail until the
/// cap unpauses; a rotation proposed by the cap keeps paying the old master until the delay has
/// passed and anyone finalizes it, after which deposits reach the new master.
#[sim_test]
async fn test_pause_and_rotation_policy() {
    let mut env = forwarding_address_test_env(true).build().await;
    let master = env.get_sender(0);
    let depositor = env.get_sender(1);
    let new_master = env.get_sender(2);
    let amount = 1_000_000;
    let registration = register_master(&mut env, master).await;
    let forwarding_address = ForwardingAddress::derive_opaque(registration.master_id, PAYLOAD);

    let registry_call = |env: &TestEnv, sender: SuiAddress, function: &str, args: Vec<CallArg>| {
        let mut builder = ProgrammableTransactionBuilder::new();
        let registry = builder
            .obj(ObjectArg::SharedObject {
                id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version: forwarding_address_registry_initial_shared_version(env),
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();
        let mut arguments = vec![registry];
        for arg in args {
            arguments.push(builder.input(arg).unwrap());
        }
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("forwarding_address").unwrap(),
            Identifier::new(function).unwrap(),
            vec![],
            arguments,
        );
        TransactionData::new_programmable(
            sender,
            vec![env.get_gas_for_sender(sender)[0]],
            builder.finish(),
            10_000_000,
            env.rgp,
        )
    };
    let cap = |env: &TestEnv| {
        CallArg::Object(ObjectArg::ImmOrOwnedObject(
            env.cluster.fullnode_handle.sui_node.with(|node| {
                node.state()
                    .get_object_cache_reader()
                    .get_object(&registration.cap_id)
                    .expect("MasterCap must exist")
                    .compute_object_reference()
            }),
        ))
    };
    let master_id = CallArg::Pure(bcs::to_bytes(&registration.master_id).unwrap());

    // Pause with the cap: deposits fail and nothing is credited.
    let tx = registry_call(&env, master, "pause", vec![cap(&env)]);
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    let tx = forwarding_address_deposit_transaction(&env, depositor, forwarding_address, amount);
    let (digest, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert_forwarding_unresolvable(effects.status(), "deposit to a paused id");
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert_eq!(env.get_sui_balance_ab(master), 0);

    // Unpause with the cap: deposits flow again.
    let tx = registry_call(&env, master, "unpause", vec![cap(&env)]);
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    let tx = forwarding_address_deposit_transaction(&env, depositor, forwarding_address, amount);
    let (digest, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert_eq!(env.get_sui_balance_ab(master), amount);

    // Propose a rotation: still the old master while pending, and not finalizable this epoch.
    let tx = registry_call(
        &env,
        master,
        "propose_rotation",
        vec![
            cap(&env),
            CallArg::Pure(bcs::to_bytes(&new_master).unwrap()),
        ],
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    let tx = forwarding_address_deposit_transaction(&env, depositor, forwarding_address, amount);
    let (digest, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert_eq!(env.get_sui_balance_ab(master), 2 * amount);
    let tx = registry_call(
        &env,
        depositor,
        "finalize_rotation",
        vec![master_id.clone()],
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(
        matches!(
            effects.status(),
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionFailureStatus::MoveAbort(location, _),
                ..
            }) if location.function_name.as_deref() == Some("finalize_rotation")
        ),
        "finalizing before the delay must abort in finalize_rotation: {effects:?}"
    );

    // After the delay anyone can finalize; deposits then reach the new master.
    env.cluster.trigger_reconfiguration().await;
    env.update_all_gas().await;
    let tx = registry_call(&env, depositor, "finalize_rotation", vec![master_id]);
    let (digest, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    let events = get_events(&env, &digest);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        bcs::from_bytes::<RotationFinalized>(&events[0].contents).unwrap(),
        RotationFinalized {
            master_id: registration.master_id,
            master: new_master,
        }
    );
    let tx = forwarding_address_deposit_transaction(&env, depositor, forwarding_address, amount);
    let (digest, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert_eq!(env.get_sui_balance_ab(master), 2 * amount);
    assert_eq!(env.get_sui_balance_ab(new_master), amount);
    assert_forwarding_deposit_event(&env, &digest, forwarding_address, new_master, amount);
}

/// Objects sent to a forwarding address end up owned by the master, with a `ForwardingTransfer`
/// event naming the forwarding address, whether sent by `TransferObjects` or from Move.
#[sim_test]
async fn test_objects_sent_to_forwarding_addresses_reach_the_master() {
    let mut env = forwarding_address_test_env(true).build().await;
    let master = env.get_sender(0);
    let sender = env.get_sender(1);
    let registration = register_master(&mut env, master).await;
    let forwarding_address = ForwardingAddress::derive_opaque(registration.master_id, PAYLOAD);

    let mut builder = ProgrammableTransactionBuilder::new();
    let amount = builder.pure(1_000u64).unwrap();
    let coin = builder.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount],
    ));
    builder.transfer_arg(forwarding_address, coin);
    let transaction = TransactionData::new_programmable(
        sender,
        vec![env.get_gas_for_sender(sender)[0]],
        builder.finish(),
        10_000_000,
        env.rgp,
    );
    let (digest, effects) = env.exec_tx_directly(transaction).await.unwrap();
    assert!(effects.status().is_ok(), "{effects:?}");
    let (coin_ref, owner) = effects.created().into_iter().next().unwrap();
    assert_eq!(owner, Owner::AddressOwner(master), "{effects:?}");

    let events = get_events(&env, &digest);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events[0].type_.name.as_ident_str(),
        FORWARDING_TRANSFER_STRUCT_NAME
    );
    assert_eq!(
        bcs::from_bytes::<ForwardingTransfer>(&events[0].contents).unwrap(),
        ForwardingTransfer {
            forwarding_address,
            master,
            object_id: coin_ref.0,
        }
    );

    // Unregistered id: the object is not stranded, the transaction fails.
    let unregistered = ForwardingAddress::derive_opaque(0, PAYLOAD);
    let mut builder = ProgrammableTransactionBuilder::new();
    let amount = builder.pure(1_000u64).unwrap();
    let coin = builder.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount],
    ));
    builder.transfer_arg(unregistered, coin);
    let transaction = TransactionData::new_programmable(
        sender,
        vec![env.get_gas_for_sender(sender)[0]],
        builder.finish(),
        10_000_000,
        env.rgp,
    );
    let (_, effects) = env.exec_tx_directly(transaction).await.unwrap();
    assert_forwarding_unresolvable(
        effects.status(),
        "object to an unregistered forwarding address",
    );
}
