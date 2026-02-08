// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::MultisetHash;
use fastcrypto::traits::KeyPair;
use sui_types::base_types::FullObjectRef;
use sui_types::crypto::{AccountKeyPair, AuthorityKeyPair};
use sui_types::utils::to_sender_signed_transaction;

use super::shared_object_version_manager::AssignedVersions;
use super::test_authority_builder::TestAuthorityBuilder;
use super::*;

#[cfg(test)]
use super::shared_object_version_manager::{AssignedTxAndVersions, Schedulable};

// =============================================================================
// MFP (Mysticeti Fast Path) Test Helpers
//
// The MFP transaction flow is:
//   1. Client signs transaction and submits to a validator.
//   2. The validator validates transaction and submits it to consensus.
//   3. Consensus finalizes the transaction and outputs it in a commit.
//   4. Transactions in the commit are filtered, sequenced and processed. Then they are sent to execution.
//
// =============================================================================

/// Validates a transaction.
/// This is the MFP "voting" phase - similar to what happens when a validator
/// receives a transaction before submitting to consensus.
///
/// Returns the verified transaction ready for consensus submission.
pub fn vote_transaction(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<VerifiedTransaction, SuiError> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    transaction.validity_check(&epoch_store.tx_validity_check_context())?;
    let verified_tx = epoch_store
        .verify_transaction_require_no_aliases(transaction)?
        .into_tx();

    // Validate the transaction.
    authority.handle_vote_transaction(&epoch_store, verified_tx.clone())?;

    Ok(verified_tx)
}

/// Creates a VerifiedExecutableTransaction from a signed transaction.
/// This validates the transaction, votes on it, and creates an executable
/// as if it came out of consensus.
pub fn create_executable_transaction(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<VerifiedExecutableTransaction, SuiError> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let verified_tx = vote_transaction(authority, transaction)?;
    Ok(VerifiedExecutableTransaction::new_from_consensus(
        verified_tx,
        epoch_store.epoch(),
    ))
}

/// Submits a transaction to consensus for ordering and version assignment.
/// This only simulates the consensus submission process by assigning versions
/// to shared objects.
///
/// Returns the executable transaction (now certified by consensus) and assigned versions.
/// The transaction is NOT automatically executed - use `execute_from_consensus` for that.
pub async fn submit_to_consensus(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<(VerifiedExecutableTransaction, AssignedVersions), SuiError> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();

    // First validate and vote
    let verified_tx = vote_transaction(authority, transaction)?;

    // Create executable - the transaction is now "certified" by consensus
    let executable =
        VerifiedExecutableTransaction::new_from_consensus(verified_tx, epoch_store.epoch());

    // Assign shared object versions
    let assigned_versions = authority
        .epoch_store_for_testing()
        .assign_shared_object_versions_for_tests(
            authority.get_object_cache_reader().as_ref(),
            &vec![executable.clone()],
        )?;

    let versions = assigned_versions
        .into_map()
        .get(&executable.key())
        .cloned()
        .unwrap_or_else(|| AssignedVersions::new(vec![], None));

    Ok((executable, versions))
}

/// Executes a transaction that has already been sequenced through consensus.
pub async fn execute_from_consensus(
    authority: &AuthorityState,
    executable: VerifiedExecutableTransaction,
    assigned_versions: AssignedVersions,
) -> (TransactionEffects, Option<ExecutionError>) {
    let env = ExecutionEnv::new().with_assigned_versions(assigned_versions);
    authority.execution_scheduler.enqueue(
        vec![(executable.clone().into(), env.clone())],
        &authority.epoch_store_for_testing(),
    );

    let (result, execution_error_opt) = authority
        .try_execute_executable_for_test(&executable, env)
        .await;
    let effects = result.inner().data().clone();
    (effects, execution_error_opt)
}

/// This is the primary test helper for executing transactions end-to-end.
///
/// Returns the executable transaction and signed effects.
pub async fn submit_and_execute(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<(VerifiedExecutableTransaction, SignedTransactionEffects), SuiError> {
    submit_and_execute_with_options(authority, None, transaction, false).await
}

/// Options:
/// - `fullnode`: Optionally sync and execute on a fullnode as well
/// - `with_shared`: Whether the transaction involves shared objects (triggers version assignment)
pub async fn submit_and_execute_with_options(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: Transaction,
    with_shared: bool,
) -> Result<(VerifiedExecutableTransaction, SignedTransactionEffects), SuiError> {
    let (exec, effects, _) =
        submit_and_execute_with_error(authority, fullnode, transaction, with_shared).await?;
    Ok((exec, effects))
}

/// Complete MFP flow returning execution error if any.
pub async fn submit_and_execute_with_error(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: Transaction,
    with_shared: bool,
) -> Result<
    (
        VerifiedExecutableTransaction,
        SignedTransactionEffects,
        Option<ExecutionError>,
    ),
    SuiError,
> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();

    // Vote on the transaction.
    let verified_tx = vote_transaction(authority, transaction)?;

    // Create executable - transaction is now certified by consensus
    let executable =
        VerifiedExecutableTransaction::new_from_consensus(verified_tx, epoch_store.epoch());

    // Assign shared object versions if needed
    let assigned_versions = if with_shared {
        let versions = authority
            .epoch_store_for_testing()
            .assign_shared_object_versions_for_tests(
                authority.get_object_cache_reader().as_ref(),
                &vec![executable.clone()],
            )?;
        versions
            .into_map()
            .get(&executable.key())
            .cloned()
            .unwrap_or_else(|| AssignedVersions::new(vec![], None))
    } else {
        AssignedVersions::new(vec![], None)
    };

    // State accumulator for validation
    let state_acc =
        GlobalStateHasher::new_for_tests(authority.get_global_state_hash_store().clone());
    let include_wrapped_tombstone = !authority
        .epoch_store_for_testing()
        .protocol_config()
        .simplified_unwrap_then_delete();
    let mut state =
        state_acc.accumulate_cached_live_object_set_for_testing(include_wrapped_tombstone);

    // Execute
    let env = ExecutionEnv::new().with_assigned_versions(assigned_versions.clone());
    let (result, execution_error_opt) = authority
        .try_execute_executable_for_test(&executable, env.clone())
        .await;

    // Validate state accumulation
    let state_after =
        state_acc.accumulate_cached_live_object_set_for_testing(include_wrapped_tombstone);
    let effects_acc = state_acc.accumulate_effects(
        &[result.inner().data().clone()],
        epoch_store.protocol_config(),
    );
    state.union(&effects_acc);
    assert_eq!(state_after.digest(), state.digest());

    // Execute on fullnode if provided
    if let Some(fullnode) = fullnode {
        fullnode
            .try_execute_executable_for_test(&executable, env)
            .await;
    }

    Ok((executable, result.into_inner(), execution_error_opt))
}

/// Enqueues multiple transactions for execution after they've been through consensus.
pub async fn enqueue_and_execute_all(
    authority: &AuthorityState,
    executables: Vec<(VerifiedExecutableTransaction, ExecutionEnv)>,
) -> Result<Vec<TransactionEffects>, SuiError> {
    authority.execution_scheduler.enqueue(
        executables
            .iter()
            .map(|(exec, env)| (exec.clone().into(), env.clone()))
            .collect(),
        &authority.epoch_store_for_testing(),
    );
    let mut output = Vec::new();
    for (exec, _) in executables {
        let effects = authority.notify_read_effects("", *exec.digest()).await?;
        output.push(effects);
    }
    Ok(output)
}

/// Submits a transaction to consensus and schedules for execution.
/// Returns assigned versions. Execution happens asynchronously.
pub async fn submit_and_schedule(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<AssignedVersions, SuiError> {
    let (executable, versions) = submit_to_consensus(authority, transaction).await?;

    let env = ExecutionEnv::new().with_assigned_versions(versions.clone());
    authority.execution_scheduler().enqueue_transactions(
        vec![(executable, env)],
        &authority.epoch_store_for_testing(),
    );

    Ok(versions)
}

pub async fn init_state_validator_with_fullnode() -> (Arc<AuthorityState>, Arc<AuthorityState>) {
    use sui_types::crypto::get_authority_key_pair;

    let validator = TestAuthorityBuilder::new().build().await;
    let fullnode_key_pair = get_authority_key_pair().1;
    let fullnode = TestAuthorityBuilder::new()
        .with_keypair(&fullnode_key_pair)
        .build()
        .await;
    (validator, fullnode)
}

pub async fn init_state_with_committee(
    genesis: &Genesis,
    authority_key: &AuthorityKeyPair,
) -> Arc<AuthorityState> {
    TestAuthorityBuilder::new()
        .with_genesis_and_keypair(genesis, authority_key)
        .build()
        .await
}

pub async fn init_state_with_ids<I: IntoIterator<Item = (SuiAddress, ObjectID)>>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = TestAuthorityBuilder::new().build().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state.insert_genesis_object(obj).await;
    }
    state
}

pub async fn init_state_with_ids_and_versions<
    I: IntoIterator<Item = (SuiAddress, ObjectID, SequenceNumber)>,
>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = TestAuthorityBuilder::new().build().await;
    for (address, object_id, version) in objects {
        let obj = Object::with_id_owner_version_for_testing(
            object_id,
            version,
            Owner::AddressOwner(address),
        );
        state.insert_genesis_object(obj).await;
    }
    state
}

pub async fn init_state_with_objects<I: IntoIterator<Item = Object>>(
    objects: I,
) -> Arc<AuthorityState> {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_swarm_config::network_config_builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();
    init_state_with_objects_and_committee(objects, &genesis, &keypair).await
}

pub async fn init_state_with_objects_and_committee<I: IntoIterator<Item = Object>>(
    objects: I,
    genesis: &Genesis,
    authority_key: &AuthorityKeyPair,
) -> Arc<AuthorityState> {
    let state = init_state_with_committee(genesis, authority_key).await;
    for o in objects {
        state.insert_genesis_object(o).await;
    }
    state
}

pub async fn init_state_with_object_id(
    address: SuiAddress,
    object: ObjectID,
) -> Arc<AuthorityState> {
    init_state_with_ids(std::iter::once((address, object))).await
}

pub async fn init_state_with_ids_and_expensive_checks<
    I: IntoIterator<Item = (SuiAddress, ObjectID)>,
>(
    objects: I,
    config: ExpensiveSafetyCheckConfig,
) -> Arc<AuthorityState> {
    let state = TestAuthorityBuilder::new()
        .with_expensive_safety_checks(config)
        .build()
        .await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state.insert_genesis_object(obj).await;
    }
    state
}

pub fn init_transfer_transaction(
    authority_state: &AuthorityState,
    sender: SuiAddress,
    secret: &AccountKeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> VerifiedTransaction {
    let data = TransactionData::new_transfer(
        recipient,
        FullObjectRef::from_fastpath_ref(object_ref),
        sender,
        gas_object_ref,
        gas_budget,
        gas_price,
    );
    let tx = to_sender_signed_transaction(data, secret);
    authority_state
        .epoch_store_for_testing()
        .verify_transaction_require_no_aliases(tx)
        .unwrap()
        .into_tx()
}

#[cfg(test)]
pub async fn submit_batch_to_consensus<C>(
    authority: &AuthorityState,
    transactions: &[Transaction],
    consensus_handler: &mut crate::consensus_handler::ConsensusHandler<C>,
    captured_transactions: &crate::consensus_test_utils::CapturedTransactions,
) -> (Vec<Schedulable>, AssignedTxAndVersions)
where
    C: crate::checkpoints::CheckpointServiceNotify + Send + Sync + 'static,
{
    use crate::consensus_test_utils::TestConsensusCommit;
    use sui_types::messages_consensus::ConsensusTransaction;

    let consensus_transactions: Vec<ConsensusTransaction> = transactions
        .iter()
        .map(|tx| ConsensusTransaction::new_user_transaction_message(&authority.name, tx.clone()))
        .collect();

    let epoch_store = authority.epoch_store_for_testing();
    let round = epoch_store.get_highest_pending_checkpoint_height() + 1;
    let timestamp_ms = epoch_store.epoch_start_state().epoch_start_timestamp_ms();
    let sub_dag_index = 0;

    let commit =
        TestConsensusCommit::new(consensus_transactions, round, timestamp_ms, sub_dag_index);

    consensus_handler
        .handle_consensus_commit_for_test(commit)
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (scheduled_txns, assigned_tx_and_versions) = {
        let mut captured = captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected transactions to be scheduled"
        );
        let (scheduled_txns, assigned_tx_and_versions, _) = captured.remove(0);
        (scheduled_txns, assigned_tx_and_versions)
    };

    (scheduled_txns, assigned_tx_and_versions)
}

pub async fn assign_versions_and_schedule(
    authority: &AuthorityState,
    executable: &VerifiedExecutableTransaction,
) -> AssignedVersions {
    let assigned_versions = authority
        .epoch_store_for_testing()
        .assign_shared_object_versions_for_tests(
            authority.get_object_cache_reader().as_ref(),
            &vec![executable.clone()],
        )
        .unwrap();

    let versions = assigned_versions
        .into_map()
        .get(&executable.key())
        .cloned()
        .unwrap_or_else(|| AssignedVersions::new(vec![], None));

    let env = ExecutionEnv::new().with_assigned_versions(versions.clone());
    authority.execution_scheduler().enqueue_transactions(
        vec![(executable.clone(), env)],
        &authority.epoch_store_for_testing(),
    );

    versions
}

/// Assigns shared object versions for an executable without scheduling for execution.
/// This is used when you need version assignment but want to control execution separately.
pub async fn assign_shared_object_versions(
    authority: &AuthorityState,
    executable: &VerifiedExecutableTransaction,
) -> AssignedVersions {
    let assigned_versions = authority
        .epoch_store_for_testing()
        .assign_shared_object_versions_for_tests(
            authority.get_object_cache_reader().as_ref(),
            &vec![executable.clone()],
        )
        .unwrap();

    assigned_versions
        .into_map()
        .get(&executable.key())
        .cloned()
        .unwrap_or_else(|| AssignedVersions::new(vec![], None))
}
