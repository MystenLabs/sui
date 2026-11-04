// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_test_utils::submit_to_consensus;
use crate::authority::shared_object_congestion_tracker::SharedObjectCongestionTracker;
use crate::authority::{AuthorityState, ExecutionEnv};
use crate::consensus_test_utils;
use crate::consensus_test_utils::TestConsensusCommit;
use crate::{
    authority::{
        authority_tests::execute_programmable_transaction,
        move_integration_tests::build_and_publish_test_package,
        test_authority_builder::TestAuthorityBuilder,
    },
    move_call,
};
use move_core_types::{ident_str, identifier::Identifier};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use sui_config::certificate_deny_config::CertificateDenyConfigBuilder;
use sui_macros::{clear_fail_point, register_fail_point_arg, sim_test};
use sui_protocol_config::{
    Chain, ExecutionTimeEstimateParams, PerObjectCongestionControlMode, ProtocolConfig,
    ProtocolVersion,
};
use sui_types::digests::TransactionDigest;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::transaction::{
    Argument, Command, GasData, ObjectArg, PlainTransactionWithClaims, SharedObjectMutability,
    Transaction, TransactionData, TransactionDataV1, TransactionExpiration, TransactionKind,
    VerifiedTransaction,
};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::{AccountKeyPair, get_key_pair},
    effects::{InputConsensusObject, TransactionEffectsAPI},
    execution_status::{CongestedObjects, ExecutionErrorKind},
    gas_allowance::GasAllowance,
    object::Object,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
};

pub const TEST_ONLY_GAS_PRICE: u64 = 1000;
pub const TEST_ONLY_GAS_UNIT: u64 = 10_000;

struct TestSetup {
    setup_authority_state: Arc<AuthorityState>,
    protocol_config: ProtocolConfig,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    package: ObjectRef,
    gas_object_id: ObjectID,
}

impl TestSetup {
    async fn new() -> Self {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        // Keep the debt scenarios independent of later protocol price changes.
        protocol_config.set_gas_allowance_congestion_control_cost_for_testing(1_000);
        // Use ExecutionTimeEstimate mode with parameters that will cause congestion
        protocol_config.set_per_object_congestion_control_mode_for_testing(
            PerObjectCongestionControlMode::ExecutionTimeEstimate(ExecutionTimeEstimateParams {
                target_utilization: 0, // 0% utilization means budget of 0, so any cost will exceed it
                allowed_txn_cost_overage_burst_limit_us: 0,
                max_estimate_us: u64::MAX,
                randomness_scalar: 100,
                stored_observations_num_included_checkpoints: 10,
                stored_observations_limit: u64::MAX,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: false,
                observations_chunk_size: Some(18),
            }),
        );

        // Set max deferral rounds to 0 to test cancellation. All deferred transactions will be cancelled.
        protocol_config.set_max_deferral_rounds_for_congestion_control_for_testing(0);

        let setup_authority_state = TestAuthorityBuilder::new()
            .with_reference_gas_price(TEST_ONLY_GAS_PRICE)
            .with_protocol_config(protocol_config.clone())
            .build()
            .await;

        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
        setup_authority_state.insert_genesis_object(gas_object.clone());

        let package = build_and_publish_test_package(
            &setup_authority_state,
            &sender,
            &sender_key,
            &gas_object_id,
            "congestion_control",
            false,
        )
        .await;

        Self {
            setup_authority_state,
            protocol_config,
            sender,
            sender_key,
            package,
            gas_object_id,
        }
    }

    // Creates a shared object in `setup_authority_state` and returns the object reference.
    async fn create_shared_object(&self) -> ObjectRef {
        let mut builder = ProgrammableTransactionBuilder::new();
        move_call! {
            builder,
            (self.package.0)::congestion_control::create_shared()
        };
        let pt = builder.finish();

        let create_shared_object_effects = execute_programmable_transaction(
            &self.setup_authority_state,
            &self.gas_object_id,
            &self.sender,
            &self.sender_key,
            pt,
            TEST_ONLY_GAS_UNIT,
        )
        .await
        .unwrap();
        assert!(
            create_shared_object_effects.status().is_ok(),
            "Execution error {:?}",
            create_shared_object_effects.status()
        );
        assert_eq!(create_shared_object_effects.created().len(), 1);
        create_shared_object_effects.created()[0].0
    }

    // Creates an owned object in `setup_authority_state` and returns the object reference.
    async fn create_owned_object(&self) -> ObjectRef {
        let mut builder = ProgrammableTransactionBuilder::new();
        move_call! {
            builder,
            (self.package.0)::congestion_control::create_owned()
        };
        let pt = builder.finish();

        let create_owned_object_effects = execute_programmable_transaction(
            &self.setup_authority_state,
            &self.gas_object_id,
            &self.sender,
            &self.sender_key,
            pt,
            TEST_ONLY_GAS_UNIT,
        )
        .await
        .unwrap();
        assert!(
            create_owned_object_effects.status().is_ok(),
            "Execution error {:?}",
            create_owned_object_effects.status()
        );
        assert_eq!(create_owned_object_effects.created().len(), 1);
        create_owned_object_effects.created()[0].0
    }

    async fn create_gas_allowance(&self) -> ObjectRef {
        let mut builder = ProgrammableTransactionBuilder::new();
        let amount = builder.pure(100_000_000u64).unwrap();
        let payment = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
        let spender = builder.pure(self.sender).unwrap();
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("gas_allowance").unwrap(),
            Identifier::new("new").unwrap(),
            vec![],
            vec![payment, spender],
        );
        let effects = execute_programmable_transaction(
            &self.setup_authority_state,
            &self.gas_object_id,
            &self.sender,
            &self.sender_key,
            builder.finish(),
            TEST_ONLY_GAS_UNIT,
        )
        .await
        .unwrap();
        effects
            .created()
            .iter()
            .find_map(|(object, owner)| owner.is_shared().then_some(*object))
            .expect("gas allowance creation must produce a shared object")
    }

    // Converts an object to a genesis object by setting its previous_transaction to a genesis marker.
    fn convert_to_genesis_obj(obj: Object) -> Object {
        let mut genesis_obj = obj.clone();
        genesis_obj.previous_transaction = TransactionDigest::genesis_marker();
        genesis_obj
    }

    // Returns a list of objects that can be used as genesis object for a brand new authority state,
    // including the gas object, the package object, and the objects passed in `objects`.
    async fn create_genesis_objects_for_new_authority_state(
        &self,
        objects: &[ObjectID],
    ) -> Vec<Object> {
        let mut genesis_objects = Vec::new();
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.package.0)
                .unwrap(),
        ));
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.gas_object_id)
                .unwrap(),
        ));

        for obj in objects {
            genesis_objects.push(TestSetup::convert_to_genesis_obj(
                self.setup_authority_state.get_object(obj).unwrap(),
            ));
        }
        genesis_objects
    }
}

// Tests execution aspect of cancelled transaction due to shared object congestion. Mainly tests that
//   1. Cancelled transaction should return correct error status.
//   2. Executing cancelled transaction with effects should result in the same transaction cancellation.
#[sim_test]
async fn test_congestion_control_execution_cancellation() {
    test_execution_cancellation(false, false, false).await;
}

#[sim_test]
async fn test_congestion_control_allowance_execution_cancellation() {
    test_execution_cancellation(true, false, false).await;
}

#[sim_test]
async fn test_congestion_control_allowance_only_execution_cancellation() {
    test_execution_cancellation(true, true, false).await;
}

#[sim_test]
async fn test_allowance_congestion_replay_preserves_certificate_denial() {
    test_execution_cancellation(true, true, true).await;
}

async fn test_execution_cancellation(
    use_allowance: bool,
    congestion_on_allowance: bool,
    certificate_denied: bool,
) {
    telemetry_subscribers::init_for_testing();

    // Compare consensus execution with effects replay on an identical authority.
    let test_setup = TestSetup::new().await;
    let shared_object_1 = test_setup.create_shared_object().await;
    let shared_object_2 = test_setup.create_shared_object().await;
    let owned_object = test_setup.create_owned_object().await;
    let allowance = if use_allowance {
        Some(test_setup.create_gas_allowance().await)
    } else {
        None
    };
    let allowance_balance = allowance.map(|allowance| {
        GasAllowance::from_object(
            &test_setup
                .setup_authority_state
                .get_object(&allowance.0)
                .unwrap(),
        )
        .unwrap()
        .balance
        .value()
    });

    // Gets objects that can be used as genesis objects for new authority states.
    let mut object_ids = vec![shared_object_1.0, shared_object_2.0, owned_object.0];
    if let Some(allowance) = allowance {
        object_ids.push(allowance.0);
    }
    let genesis_objects = test_setup
        .create_genesis_objects_for_new_authority_state(&object_ids)
        .await;
    let genesis_object_ref = |id| {
        genesis_objects
            .iter()
            .find(|object| object.id() == id)
            .unwrap()
            .compute_object_reference()
    };

    let congested_object = if congestion_on_allowance {
        allowance
            .expect("allowance-only congestion requires an allowance")
            .0
    } else {
        shared_object_1.0
    };
    // Set initial cost of 10, which with 0% target utilization and no burst limit exceeds the
    // budget and cancels the transaction.
    register_fail_point_arg("initial_congestion_tracker", move || {
        Some(SharedObjectCongestionTracker::new(
            [(congested_object, 10)],
            ExecutionTimeEstimateParams {
                target_utilization: 0,
                allowed_txn_cost_overage_burst_limit_us: 0,
                max_estimate_us: u64::MAX,
                randomness_scalar: 100,
                stored_observations_num_included_checkpoints: 10,
                stored_observations_limit: u64::MAX,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: false,
                observations_chunk_size: Some(18),
            },
            false,
            false,
        ))
    });

    let mut txn_builder = ProgrammableTransactionBuilder::new();
    // A cancelled allowance remains a loaded shared input so it can pay cancellation gas.
    if let Some(allowance) = allowance {
        txn_builder
            .obj(ObjectArg::SharedObject {
                id: allowance.0,
                initial_shared_version: allowance.1,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();
    }
    let arg1 = txn_builder
        .obj(ObjectArg::SharedObject {
            id: shared_object_1.0,
            initial_shared_version: shared_object_1.1,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let arg2 = txn_builder
        .obj(ObjectArg::SharedObject {
            id: shared_object_2.0,
            initial_shared_version: shared_object_2.1,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let owned_object_ref = genesis_object_ref(owned_object.0);
    let arg3 = txn_builder
        .obj(ObjectArg::ImmOrOwnedObject(owned_object_ref))
        .unwrap();
    move_call! {
        txn_builder,
        (test_setup.package.0)::congestion_control::increment(arg1, arg2, arg3)
    };
    let data = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(txn_builder.finish()),
        sender: test_setup.sender,
        gas_data: GasData {
            payment: allowance.map_or_else(
                || vec![genesis_object_ref(test_setup.gas_object_id)],
                |_| vec![],
            ),
            owner: allowance.map_or(test_setup.sender, |allowance| allowance.0.into()),
            price: TEST_ONLY_GAS_PRICE,
            budget: TEST_ONLY_GAS_PRICE * TEST_ONLY_GAS_UNIT,
        },
        expiration: TransactionExpiration::None,
    });
    let congested_tx = Transaction::from_data_and_signer(data, vec![&test_setup.sender_key]);
    let expected_error = if certificate_denied {
        ExecutionErrorKind::CertificateDenied
    } else {
        ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
            congested_objects: CongestedObjects(vec![congested_object]),
        }
    };
    let deny_config = if certificate_denied {
        CertificateDenyConfigBuilder::new()
            .add_certificate_deny(*congested_tx.digest())
            .build()
    } else {
        CertificateDenyConfigBuilder::new().build()
    };

    // Both authorities enforce the same earlier error, independently of cancellation metadata.
    let authority_state = TestAuthorityBuilder::new()
        .with_reference_gas_price(TEST_ONLY_GAS_PRICE)
        .with_protocol_config(test_setup.protocol_config.clone())
        .with_certificate_deny_config(deny_config.clone())
        .build()
        .await;
    authority_state.insert_genesis_objects(&genesis_objects);
    let authority_state_2 = TestAuthorityBuilder::new()
        .with_reference_gas_price(TEST_ONLY_GAS_PRICE)
        .with_protocol_config(test_setup.protocol_config.clone())
        .with_certificate_deny_config(deny_config)
        .build()
        .await;
    authority_state_2.insert_genesis_objects(&genesis_objects);

    let consensus_setup =
        consensus_test_utils::setup_consensus_handler_for_testing(&authority_state).await;
    let mut consensus_handler = consensus_setup.consensus_handler;
    let captured_transactions = consensus_setup.captured_transactions;

    let consensus_transactions = vec![ConsensusTransaction::new_user_transaction_v2_message(
        &authority_state.name,
        PlainTransactionWithClaims::no_aliases(congested_tx.clone()),
    )];
    let commit = TestConsensusCommit::new(consensus_transactions, 1, 0, 0);

    consensus_handler
        .handle_consensus_commit_for_test(commit)
        .await;

    // Wait for captured transactions to be available
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Get the captured transactions
    let scheduled_txns = {
        let mut captured = captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected transactions to be scheduled"
        );
        let (scheduled_txns, _) = captured.remove(0);
        scheduled_txns
    };

    // Both prologue and the cancelled transaction should be scheduled
    // The cancelled transaction will abort during execution
    assert_eq!(
        scheduled_txns.len(),
        2,
        "Expected prologue + cancelled transaction"
    );

    // Now execute the cancelled transaction to get the effects
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let executable = VerifiedExecutableTransaction::new_from_consensus(
        VerifiedTransaction::new_unchecked(congested_tx.clone()),
        epoch_store.epoch(),
    );

    // Find the assigned versions for our specific transaction
    let tx_key = executable.key();
    let assigned_versions = scheduled_txns
        .iter()
        .find(|(s, _)| s.key() == tx_key)
        .map(|(_, v)| v.clone())
        .expect("Transaction should have assigned versions");

    let execution_env = ExecutionEnv::new().with_assigned_versions(assigned_versions);
    let (effects, execution_error) = authority_state
        .try_execute_executable_for_test(&executable, execution_env)
        .await;

    // Earlier certificate denial masks congestion without changing the retained allowance input.
    assert!(execution_error.is_some());
    assert_eq!(
        execution_error.unwrap().to_execution_status().0,
        expected_error
    );
    let effects = effects.data();

    assert!(effects.gas_cost_summary().computation_cost > 0);
    if let Some(allowance) = allowance {
        assert!(
            GasAllowance::from_object(&authority_state.get_object(&allowance.0).unwrap())
                .unwrap()
                .balance
                .value()
                < allowance_balance.unwrap()
        );
    }

    // The allowance is a normal mutable input while all non-allowance shared inputs remain
    // cancelled sentinels.
    let expected_cancelled = vec![
        InputConsensusObject::Cancelled(
            shared_object_1.0,
            if congested_object == shared_object_1.0 {
                SequenceNumber::CONGESTED
            } else {
                SequenceNumber::CANCELLED_READ
            },
        ),
        InputConsensusObject::Cancelled(
            shared_object_2.0,
            if congested_object == shared_object_2.0 {
                SequenceNumber::CONGESTED
            } else {
                SequenceNumber::CANCELLED_READ
            },
        ),
    ];
    let consensus_objects = effects.accessed_consensus_objects();
    if let Some(allowance) = allowance {
        assert!(matches!(
            consensus_objects.first(),
            Some(InputConsensusObject::Mutate((id, version, _)))
                if *id == allowance.0 && *version == allowance.1
        ));
        assert_eq!(consensus_objects[1..].to_vec(), expected_cancelled);
    } else {
        assert_eq!(consensus_objects, expected_cancelled);
    }

    // Run the same transaction in `authority_state_2`, but using the above effects for the execution.
    let (executable, _) = submit_to_consensus(&authority_state_2, congested_tx)
        .await
        .unwrap();
    let assigned_versions = authority_state_2
        .epoch_store_for_testing()
        .acquire_shared_version_assignments_from_effects(
            &executable,
            effects,
            None,
            authority_state_2.get_object_cache_reader().as_ref(),
        );
    let execution_env = ExecutionEnv::new().with_assigned_versions(assigned_versions);
    let (effects_2, execution_error) = authority_state_2
        .try_execute_executable_for_test(&executable, execution_env)
        .await;

    // Effects replay preserves error precedence, all mutations and the escrow charge.
    assert_eq!(
        execution_error.unwrap().to_execution_status().0,
        expected_error
    );
    assert_eq!(effects, effects_2.data());
    if let Some(allowance) = allowance {
        assert!(
            GasAllowance::from_object(&authority_state_2.get_object(&allowance.0).unwrap())
                .unwrap()
                .balance
                .value()
                < allowance_balance.unwrap()
        );
    }
}

fn zero_command_allowance_transaction(
    test_setup: &TestSetup,
    allowance: ObjectRef,
    shared_objects: impl IntoIterator<Item = ObjectRef>,
    budget: u64,
) -> Transaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    for object in std::iter::once(allowance).chain(shared_objects) {
        builder
            .obj(ObjectArg::SharedObject {
                id: object.0,
                initial_shared_version: object.1,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();
    }
    Transaction::from_data_and_signer(
        TransactionData::V1(TransactionDataV1 {
            kind: TransactionKind::ProgrammableTransaction(builder.finish()),
            sender: test_setup.sender,
            gas_data: GasData {
                payment: vec![],
                owner: allowance.0.into(),
                price: TEST_ONLY_GAS_PRICE,
                budget,
            },
            expiration: TransactionExpiration::ValidDuring {
                min_epoch: Some(0),
                max_epoch: Some(0),
                min_timestamp: None,
                max_timestamp: None,
                chain: test_setup.setup_authority_state.get_chain_identifier(),
                nonce: 0,
            },
        }),
        vec![&test_setup.sender_key],
    )
}

#[sim_test]
async fn test_zero_command_allowance_cancellations_carry_only_settlement_debt() {
    telemetry_subscribers::init_for_testing();

    let test_setup = TestSetup::new().await;
    let allowance = test_setup.create_gas_allowance().await;
    let congestion_sentinel = test_setup.create_shared_object().await;
    let unrelated_sentinel = test_setup.create_shared_object().await;

    let use_initial_debt = Arc::new(AtomicBool::new(true));
    let initial_debt = use_initial_debt.clone();
    register_fail_point_arg("initial_congestion_tracker", move || {
        initial_debt.swap(false, Ordering::SeqCst).then(|| {
            SharedObjectCongestionTracker::new(
                [(congestion_sentinel.0, 1_500), (unrelated_sentinel.0, 750)],
                ExecutionTimeEstimateParams {
                    target_utilization: 0,
                    allowed_txn_cost_overage_burst_limit_us: 0,
                    max_estimate_us: u64::MAX,
                    randomness_scalar: 100,
                    stored_observations_num_included_checkpoints: 10,
                    stored_observations_limit: u64::MAX,
                    stake_weighted_median_threshold: 0,
                    default_none_duration_for_new_keys: false,
                    observations_chunk_size: Some(18),
                },
                false,
                false,
            )
        })
    });

    let mut consensus_setup = consensus_test_utils::setup_consensus_handler_for_testing(
        &test_setup.setup_authority_state,
    )
    .await;
    let first = zero_command_allowance_transaction(
        &test_setup,
        allowance,
        [congestion_sentinel, unrelated_sentinel],
        TEST_ONLY_GAS_PRICE * TEST_ONLY_GAS_UNIT,
    );
    let second = zero_command_allowance_transaction(
        &test_setup,
        allowance,
        [congestion_sentinel, unrelated_sentinel],
        TEST_ONLY_GAS_PRICE * (TEST_ONLY_GAS_UNIT + 1),
    );
    consensus_setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![
                ConsensusTransaction::new_user_transaction_v2_message(
                    &test_setup.setup_authority_state.name,
                    PlainTransactionWithClaims::no_aliases(first),
                ),
                ConsensusTransaction::new_user_transaction_v2_message(
                    &test_setup.setup_authority_state.name,
                    PlainTransactionWithClaims::no_aliases(second),
                ),
            ],
            1,
            0,
            0,
        ))
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let first_commit = consensus_setup.captured_transactions.lock().remove(0);
    assert_eq!(first_commit.0.len(), 3, "prologue plus both cancellations");
    assert!(
        first_commit
            .0
            .iter()
            .skip(1)
            .all(|(_, assigned)| assigned.congested_gas_allowance.is_none())
    );
    assert_eq!(
        consensus_setup
            .metrics
            .consensus_handler_max_object_costs
            .with_label_values(&["regular_commit"])
            .get(),
        2_000,
        "only the retained allowance receives two native settlement costs"
    );

    let carried = zero_command_allowance_transaction(
        &test_setup,
        allowance,
        [congestion_sentinel, unrelated_sentinel],
        TEST_ONLY_GAS_PRICE * (TEST_ONLY_GAS_UNIT + 2),
    );
    consensus_setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![ConsensusTransaction::new_user_transaction_v2_message(
                &test_setup.setup_authority_state.name,
                PlainTransactionWithClaims::no_aliases(carried),
            )],
            2,
            1,
            0,
        ))
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let second_commit = consensus_setup.captured_transactions.lock().remove(0);
    assert_eq!(
        second_commit.0.len(),
        2,
        "prologue plus the carried cancellation"
    );
    assert_eq!(
        second_commit.0[1].1.congested_gas_allowance,
        Some(allowance.0),
        "the debt from the prior commit makes the allowance itself congested"
    );
    assert_eq!(
        consensus_setup
            .metrics
            .consensus_handler_max_object_costs
            .with_label_values(&["regular_commit"])
            .get(),
        3_000,
        "carried allowance debt receives one more native settlement cost"
    );
    // Sustained cancellations must keep adding native work to the next commit's critical path.
    for round in 3..=22 {
        let transactions = (0..10)
            .map(|index| {
                let transaction = zero_command_allowance_transaction(
                    &test_setup,
                    allowance,
                    [congestion_sentinel, unrelated_sentinel],
                    TEST_ONLY_GAS_PRICE * (TEST_ONLY_GAS_UNIT + 3 + (round - 3) * 10 + index),
                );
                ConsensusTransaction::new_user_transaction_v2_message(
                    &test_setup.setup_authority_state.name,
                    PlainTransactionWithClaims::no_aliases(transaction),
                )
            })
            .collect();
        consensus_setup
            .consensus_handler
            .handle_consensus_commit_for_test(TestConsensusCommit::new(
                transactions,
                round,
                round - 1,
                0,
            ))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let scheduled = consensus_setup.captured_transactions.lock().remove(0);
        assert_eq!(scheduled.0.len(), 11, "prologue plus ten cancellations");
        assert!(
            scheduled
                .0
                .iter()
                .skip(1)
                .all(|(_, assigned)| { assigned.congested_gas_allowance == Some(allowance.0) })
        );
        assert_eq!(
            consensus_setup
                .metrics
                .consensus_handler_max_object_costs
                .with_label_values(&["regular_commit"])
                .get(),
            i64::try_from((3 + (round - 2) * 10) * 1_000).unwrap(),
        );
    }
    clear_fail_point("initial_congestion_tracker");
}
