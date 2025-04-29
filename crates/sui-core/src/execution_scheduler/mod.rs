// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::{authority_per_epoch_store::AuthorityPerEpochStore, AuthorityMetrics},
    execution_cache::{ObjectCacheRead, TransactionCacheRead},
};
use mysten_metrics::spawn_monitored_task;
use overload_tracker::OverloadTracker;
use parking_lot::RwLock;
use std::{collections::HashSet, sync::Arc};
use sui_config::node::AuthorityOverloadConfig;
use sui_types::{
    base_types::FullObjectID,
    digests::{TransactionDigest, TransactionEffectsDigest},
    error::SuiResult,
    executable_transaction::VerifiedExecutableTransaction,
    storage::InputKey,
    transaction::{SenderSignedData, TransactionDataAPI, VerifiedCertificate},
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use tracing::warn;

mod overload_tracker;

#[derive(Clone, Debug)]
pub struct PendingCertificateStats {
    // The time this certificate enters execution scheduler.
    #[allow(unused)]
    pub enqueue_time: Instant,
    // The time this certificate becomes ready for execution.
    pub ready_time: Instant,
}

#[derive(Clone, Debug)]
pub struct PendingCertificate {
    // Certified transaction to be executed.
    pub certificate: VerifiedExecutableTransaction,
    // When executing from checkpoint, the certified effects digest is provided, so that forks can
    // be detected prior to committing the transaction.
    pub expected_effects_digest: Option<TransactionEffectsDigest>,
    // Stores stats about this transaction.
    pub stats: PendingCertificateStats,
}

#[derive(Clone)]
pub struct ExecutionScheduler {
    object_cache_read: Arc<dyn ObjectCacheRead>,
    transaction_cache_read: Arc<dyn TransactionCacheRead>,
    pending_certificates: Arc<RwLock<HashSet<TransactionDigest>>>,
    overload_tracker: Arc<OverloadTracker>,
    tx_ready_certificates: UnboundedSender<PendingCertificate>,
    metrics: Arc<AuthorityMetrics>,
}

impl ExecutionScheduler {
    pub fn new(
        object_cache_read: Arc<dyn ObjectCacheRead>,
        transaction_cache_read: Arc<dyn TransactionCacheRead>,
        tx_ready_certificates: UnboundedSender<PendingCertificate>,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        Self {
            object_cache_read,
            transaction_cache_read,
            pending_certificates: Arc::new(RwLock::new(HashSet::new())),
            overload_tracker: Arc::new(OverloadTracker::new()),
            tx_ready_certificates,
            metrics,
        }
    }

    pub(crate) fn enqueue_with_expected_effects_digest(
        &self,
        certs: Vec<(VerifiedExecutableTransaction, TransactionEffectsDigest)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        let certs = certs
            .into_iter()
            .map(|(cert, fx)| (cert, Some(fx)))
            .collect();
        self.enqueue_impl(certs, epoch_store)
    }

    pub(crate) fn enqueue_certificates(
        &self,
        certs: Vec<VerifiedCertificate>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        let executable_txns = certs
            .into_iter()
            .map(VerifiedExecutableTransaction::new_from_certificate)
            .collect();
        self.enqueue(executable_txns, epoch_store)
    }

    pub(crate) fn enqueue(
        &self,
        certs: Vec<VerifiedExecutableTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        let certs = certs.into_iter().map(|cert| (cert, None)).collect();
        self.enqueue_impl(certs, epoch_store)
    }

    fn enqueue_impl(
        &self,
        certs: Vec<(
            VerifiedExecutableTransaction,
            Option<TransactionEffectsDigest>,
        )>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        let certs = certs.into_iter().filter_map(|cert| {
            if cert.0.epoch() == epoch_store.epoch() {
                Some(cert)
            } else {
                warn!(
                    "Ignoring enqueued certificate from wrong epoch. Expected={} Certificate={:?}",
                    epoch_store.epoch(),
                    cert.0.epoch(),
                );
                None
            }
        });

        for cert in certs {
            let scheduler = self.clone();
            let epoch_store = epoch_store.clone();
            spawn_monitored_task!(
                epoch_store.within_alive_epoch(scheduler.schedule_transaction(
                    cert.0,
                    cert.1,
                    &epoch_store,
                ))
            );
        }

        self.metrics
            .transaction_manager_num_pending_certificates
            .set(self.pending_certificates.read().len() as i64);
    }

    async fn schedule_transaction(
        self,
        cert: VerifiedExecutableTransaction,
        expected_effects_digest: Option<TransactionEffectsDigest>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        if !self.pending_certificates.write().insert(*cert.digest()) {
            return;
        }
        let digest = cert.digest();
        tracing::debug!(?digest, "Schedule_transaction");

        self.overload_tracker.add_pending_certificate(cert.data());
        let enqueue_time = Instant::now();
        let tx_data = cert.transaction_data();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("input_objects() cannot fail");

        let input_object_keys: Vec<_> =
            match epoch_store.get_input_object_keys(&cert.key(), &input_object_kinds) {
                Ok(keys) => keys,
                Err(_) => {
                    // This is possible if the transaction is already executed.
                    // TODO: Eventually we could pass assigned shared object versions
                    // to the scheduler so that this call cannot return Err.
                    assert!(self.transaction_cache_read.is_tx_already_executed(digest));
                    return;
                }
            }
            .into_iter()
            .collect();
        let receiving_object_keys: HashSet<_> = tx_data
            .receiving_objects()
            .into_iter()
            .map(|entry| {
                InputKey::VersionedObject {
                    // TODO: Add support for receiving ConsensusV2 objects. For now this assumes fastpath.
                    id: FullObjectID::new(entry.0, None),
                    version: entry.1,
                }
            })
            .collect();

        let input_and_receiving_keys = [
            input_object_keys,
            receiving_object_keys.iter().cloned().collect(),
        ]
        .concat();

        let epoch = epoch_store.epoch();
        let digests = [*digest];
        let queue_time = Instant::now();
        tracing::trace!(
            ?digests,
            "Waiting for input objects: {:?}",
            input_and_receiving_keys
        );

        tokio::select! {
            _ = self.object_cache_read
                .notify_read_input_objects(&input_and_receiving_keys, &receiving_object_keys, &epoch)
                => {
                    self.metrics
                        .transaction_manager_transaction_queue_age_s
                        .observe(queue_time.elapsed().as_secs_f64());
                    tracing::debug!(?digests, "Input objects available");
                    // TODO: Eventually we could fold execution_driver into the scheduler.
                    let _ = self.tx_ready_certificates.send(PendingCertificate {
                        certificate: cert,
                        expected_effects_digest,
                        stats: PendingCertificateStats {
                            enqueue_time,
                            ready_time: Instant::now(),
                        },
                    });
                }
            _ = self.transaction_cache_read.notify_read_executed_effects(&digests) => {
                tracing::debug!(?digests, "Transaction already executed");
                // We need to remove the pending certificate information explicitly here,
                // because the transaction may have been executed before we enqueued it.
                // So we never get to call notify_commit() from the execution commit path.
                self.notify_commit(&cert);
            }
        };
    }

    pub(crate) fn notify_commit(&self, certificate: &VerifiedExecutableTransaction) {
        self.pending_certificates
            .write()
            .remove(certificate.digest());
        self.overload_tracker
            .remove_pending_certificate(certificate.data());
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(self.pending_certificates.read().len() as i64);
    }

    pub(crate) fn check_execution_overload(
        &self,
        overload_config: &AuthorityOverloadConfig,
        tx_data: &SenderSignedData,
    ) -> SuiResult {
        let inflight_queue_len = self.pending_certificates.read().len();
        self.overload_tracker
            .check_execution_overload(overload_config, tx_data, inflight_queue_len)
    }

    #[cfg(test)]
    fn num_pending_certificates(&self) -> usize {
        self.pending_certificates.read().len()
    }
}

#[cfg(test)]
mod test {
    use std::{time::Duration, vec};

    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::executable_transaction::VerifiedExecutableTransaction;
    use sui_types::object::Owner;
    use sui_types::transaction::VerifiedTransaction;
    use sui_types::{
        base_types::{ObjectID, SequenceNumber},
        crypto::deterministic_random_account_key,
        object::Object,
        transaction::{CallArg, ObjectArg},
        SUI_FRAMEWORK_PACKAGE_ID,
    };
    use tokio::time::Instant;
    use tokio::{
        sync::mpsc::{error::TryRecvError, unbounded_channel, UnboundedReceiver},
        time::sleep,
    };

    use crate::authority::{authority_tests::init_state_with_objects, AuthorityState};

    use super::{ExecutionScheduler, PendingCertificate};

    #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
    fn make_execution_scheduler(
        state: &AuthorityState,
    ) -> (ExecutionScheduler, UnboundedReceiver<PendingCertificate>) {
        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
        let execution_scheduler = ExecutionScheduler::new(
            state.get_object_cache_reader().clone(),
            state.get_transaction_cache_reader().clone(),
            tx_ready_certificates,
            state.metrics.clone(),
        );

        (execution_scheduler, rx_ready_certificates)
    }

    fn make_transaction(gas_object: Object, input: Vec<CallArg>) -> VerifiedExecutableTransaction {
        // Use fake module, function, package and gas prices since they are irrelevant for testing
        // execution scheduler.
        let rgp = 100;
        let (sender, keypair) = deterministic_random_account_key();
        let transaction =
            TestTransactionBuilder::new(sender, gas_object.compute_object_reference(), rgp)
                .move_call(SUI_FRAMEWORK_PACKAGE_ID, "counter", "assert_value", input)
                .build_and_sign(&keypair);
        VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(transaction),
            0,
        )
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn execution_scheduler_basics() {
        // Initialize an authority state.
        let (owner, _keypair) = deterministic_random_account_key();
        let gas_objects: Vec<Object> = (0..10)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
        let state = init_state_with_objects(gas_objects.clone()).await;

        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (execution_scheduler, mut rx_ready_certificates) = make_execution_scheduler(&state);
        // scheduler should output no transaction.
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));
        // scheduler should be empty at the beginning.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);

        // Enqueue empty vec should not crash.
        execution_scheduler.enqueue(vec![], &state.epoch_store_for_testing());
        // scheduler should output no transaction.
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));

        // Enqueue a transaction with existing gas object, empty input.
        let transaction = make_transaction(gas_objects[0].clone(), vec![]);
        let tx_start_time = Instant::now();
        execution_scheduler.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
        // scheduler should output the transaction eventually.
        let pending_certificate = rx_ready_certificates.recv().await.unwrap();

        // Tests that pending certificate stats are recorded properly.
        assert!(pending_certificate.stats.enqueue_time >= tx_start_time);
        assert!(pending_certificate.stats.ready_time >= pending_certificate.stats.enqueue_time);

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Notify scheduler about transaction commit
        execution_scheduler.notify_commit(&transaction);

        // scheduler should be empty.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);

        // Enqueue a transaction with a new gas object, empty input.
        let gas_object_new = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            0.into(),
            Owner::AddressOwner(owner),
        );
        let transaction = make_transaction(gas_object_new.clone(), vec![]);
        let tx_start_time = Instant::now();
        execution_scheduler.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
        // scheduler should output no transaction yet.
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Duplicated enqueue is allowed.
        execution_scheduler.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Notify scheduler about availability of the gas object.
        state
            .get_cache_writer()
            .write_object_entry_for_test(gas_object_new);
        // scheduler should output the transaction eventually.
        let pending_certificate = rx_ready_certificates.recv().await.unwrap();

        // Tests that pending certificate stats are recorded properly. The ready time should be
        // 2 seconds apart from the enqueue time.
        assert!(pending_certificate.stats.enqueue_time >= tx_start_time);
        assert!(
            pending_certificate.stats.ready_time - pending_certificate.stats.enqueue_time
                >= Duration::from_secs(2)
        );

        // Re-enqueue the same transaction should not result in another output.
        execution_scheduler.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));

        // Notify scheduler about transaction commit
        execution_scheduler.notify_commit(&transaction);

        // scheduler should be empty at the end.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);
    }

    // Tests when objects become available, correct set of transactions can be sent to execute.
    // Specifically, we have following setup,
    //         shared_object     shared_object_2
    //       /    |    \     \    /
    //    tx_0  tx_1  tx_2    tx_3
    //     r      r     w      r
    // And when shared_object is available, tx_0, tx_1, and tx_2 can be executed. And when
    // shared_object_2 becomes available, tx_3 can be executed.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn execution_scheduler_object_dependency() {
        telemetry_subscribers::init_for_testing();
        // Initialize an authority state, with gas objects and a shared object.
        let (owner, _keypair) = deterministic_random_account_key();
        let gas_objects: Vec<Object> = (0..10)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
        let shared_object = Object::shared_for_testing();
        let initial_shared_version = shared_object.owner().start_version().unwrap();
        let shared_object_2 = Object::shared_for_testing();
        let initial_shared_version_2 = shared_object_2.owner().start_version().unwrap();

        let state = init_state_with_objects(
            [
                gas_objects.clone(),
                vec![shared_object.clone(), shared_object_2.clone()],
            ]
            .concat(),
        )
        .await;

        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (execution_scheduler, mut rx_ready_certificates) = make_execution_scheduler(&state);
        // scheduler should output no transaction.
        assert!(rx_ready_certificates.try_recv().is_err());

        // Enqueue two transactions with the same shared object input in read-only mode.
        let shared_version = 1000.into();
        let shared_object_arg_read = ObjectArg::SharedObject {
            id: shared_object.id(),
            initial_shared_version,
            mutable: false,
        };
        let transaction_read_0 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(shared_object_arg_read)],
        );
        let transaction_read_1 = make_transaction(
            gas_objects[1].clone(),
            vec![CallArg::Object(shared_object_arg_read)],
        );
        state
            .epoch_store_for_testing()
            .set_shared_object_versions_for_testing(
                transaction_read_0.digest(),
                &[(
                    (
                        shared_object.id(),
                        shared_object.owner().start_version().unwrap(),
                    ),
                    shared_version,
                )],
            )
            .unwrap();
        state
            .epoch_store_for_testing()
            .set_shared_object_versions_for_testing(
                transaction_read_1.digest(),
                &[(
                    (
                        shared_object.id(),
                        shared_object.owner().start_version().unwrap(),
                    ),
                    shared_version,
                )],
            )
            .unwrap();

        // Enqueue one transaction with the same shared object in mutable mode.
        let shared_object_arg_default = ObjectArg::SharedObject {
            id: shared_object.id(),
            initial_shared_version,
            mutable: true,
        };
        let transaction_default = make_transaction(
            gas_objects[2].clone(),
            vec![CallArg::Object(shared_object_arg_default)],
        );
        state
            .epoch_store_for_testing()
            .set_shared_object_versions_for_testing(
                transaction_default.digest(),
                &[(
                    (
                        shared_object.id(),
                        shared_object.owner().start_version().unwrap(),
                    ),
                    shared_version,
                )],
            )
            .unwrap();

        // Enqueue one transaction with two readonly shared object inputs, `shared_object` and `shared_object_2`.
        let shared_version_2 = 1000.into();
        let shared_object_arg_read_2 = ObjectArg::SharedObject {
            id: shared_object_2.id(),
            initial_shared_version: initial_shared_version_2,
            mutable: false,
        };
        let transaction_read_2 = make_transaction(
            gas_objects[3].clone(),
            vec![
                CallArg::Object(shared_object_arg_default),
                CallArg::Object(shared_object_arg_read_2),
            ],
        );
        state
            .epoch_store_for_testing()
            .set_shared_object_versions_for_testing(
                transaction_read_2.digest(),
                &[
                    (
                        (
                            shared_object.id(),
                            shared_object.owner().start_version().unwrap(),
                        ),
                        shared_version,
                    ),
                    (
                        (
                            shared_object_2.id(),
                            shared_object_2.owner().start_version().unwrap(),
                        ),
                        shared_version_2,
                    ),
                ],
            )
            .unwrap();

        execution_scheduler.enqueue(
            vec![
                transaction_read_0.clone(),
                transaction_read_1.clone(),
                transaction_default.clone(),
                transaction_read_2.clone(),
            ],
            &state.epoch_store_for_testing(),
        );

        // scheduler should output no transaction yet.
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());

        assert_eq!(execution_scheduler.num_pending_certificates(), 4);

        // Notify scheduler about availability of the first shared object.
        let mut new_shared_object = shared_object.clone();
        new_shared_object
            .data
            .try_as_move_mut()
            .unwrap()
            .increment_version_to(shared_version_2);
        state
            .get_cache_writer()
            .write_object_entry_for_test(new_shared_object);

        // scheduler should output the 3 transactions that are only waiting for this object.
        let tx_0 = rx_ready_certificates.recv().await.unwrap().certificate;
        let tx_1 = rx_ready_certificates.recv().await.unwrap().certificate;
        let tx_2 = rx_ready_certificates.recv().await.unwrap().certificate;
        {
            let mut want_digests = vec![
                transaction_read_0.digest(),
                transaction_read_1.digest(),
                transaction_default.digest(),
            ];
            want_digests.sort();
            let mut got_digests = vec![tx_0.digest(), tx_1.digest(), tx_2.digest()];
            got_digests.sort();
            assert_eq!(want_digests, got_digests);
        }

        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());

        assert_eq!(execution_scheduler.num_pending_certificates(), 4);

        // Notify scheduler about read-only transaction commit
        execution_scheduler.notify_commit(&tx_0);
        execution_scheduler.notify_commit(&tx_1);
        execution_scheduler.notify_commit(&tx_2);

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Make shared_object_2 available.
        let mut new_shared_object_2 = shared_object_2.clone();
        new_shared_object_2
            .data
            .try_as_move_mut()
            .unwrap()
            .increment_version_to(shared_version_2);
        state
            .get_cache_writer()
            .write_object_entry_for_test(new_shared_object_2);

        // Now, the transaction waiting for both shared objects can be executed.
        let tx_3 = rx_ready_certificates.recv().await.unwrap().certificate;
        assert_eq!(transaction_read_2.digest(), tx_3.digest());

        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Notify scheduler about tx_3.
        execution_scheduler.notify_commit(&tx_3);

        assert_eq!(execution_scheduler.num_pending_certificates(), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn execution_scheduler_receiving_notify_commit() {
        telemetry_subscribers::init_for_testing();
        // Initialize an authority state.
        let (owner, _keypair) = deterministic_random_account_key();
        let gas_objects: Vec<Object> = (0..10)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
        let state = init_state_with_objects(gas_objects.clone()).await;

        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (execution_scheduler, mut rx_ready_certificates) = make_execution_scheduler(&state);
        // scheduler should output no transaction.
        assert!(rx_ready_certificates.try_recv().is_err());
        // scheduler should be empty at the beginning.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);

        let obj_id = ObjectID::random();
        let object_arguments: Vec<_> = (0..10)
            .map(|i| {
                let object = Object::with_id_owner_version_for_testing(
                    obj_id,
                    i.into(),
                    Owner::AddressOwner(owner),
                );
                // Every other transaction receives the object, and we create a run of multiple receives in
                // a row at the beginning to test that the scheduler doesn't get stuck in either configuration of:
                // ImmOrOwnedObject => Receiving,
                // Receiving => Receiving
                // Receiving => ImmOrOwnedObject
                // ImmOrOwnedObject => ImmOrOwnedObject is already tested as the default case on mainnet.
                let object_arg = if i % 2 == 0 || i == 3 {
                    ObjectArg::Receiving(object.compute_object_reference())
                } else {
                    ObjectArg::ImmOrOwnedObject(object.compute_object_reference())
                };
                let txn =
                    make_transaction(gas_objects[0].clone(), vec![CallArg::Object(object_arg)]);
                (object, txn)
            })
            .collect();

        for (i, (_, txn)) in object_arguments.iter().enumerate() {
            // scheduler should output no transaction yet since waiting on receiving object or
            // ImmOrOwnedObject input.
            execution_scheduler.enqueue(vec![txn.clone()], &state.epoch_store_for_testing());
            sleep(Duration::from_secs(1)).await;
            assert!(rx_ready_certificates.try_recv().is_err());
            assert_eq!(execution_scheduler.num_pending_certificates(), i + 1);
        }

        // Now start to unravel the transactions by notifying that each subsequent
        // transaction has been processed.
        for (i, (object, txn)) in object_arguments.iter().enumerate() {
            // Mark the object as available.
            // We should now eventually see the transaction as ready.
            state
                .get_cache_writer()
                .write_object_entry_for_test(object.clone());

            // scheduler should output the transaction eventually now that the receiving object has become
            // available.
            rx_ready_certificates.recv().await.unwrap();

            // Only one transaction at a time should become available though. So if we try to get
            // another one it should fail.
            sleep(Duration::from_secs(1)).await;
            assert!(rx_ready_certificates.try_recv().is_err());

            // Notify the scheduler that the transaction has been processed.
            execution_scheduler.notify_commit(txn);

            // scheduler should now output another transaction to run since it the next version of that object
            // has become available.
            assert_eq!(
                execution_scheduler.num_pending_certificates(),
                object_arguments.len() - i - 1
            );
        }

        // After everything scheduler should be empty.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn execution_scheduler_receiving_object_ready_notifications() {
        telemetry_subscribers::init_for_testing();
        // Initialize an authority state.
        let (owner, _keypair) = deterministic_random_account_key();
        let gas_objects: Vec<Object> = (0..10)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
        let state = init_state_with_objects(gas_objects.clone()).await;

        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (execution_scheduler, mut rx_ready_certificates) = make_execution_scheduler(&state);
        // scheduler should output no transaction.
        assert!(rx_ready_certificates.try_recv().is_err());
        // scheduler should be empty at the beginning.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);

        let obj_id = ObjectID::random();
        let receiving_object_new0 =
            Object::with_id_owner_version_for_testing(obj_id, 0.into(), Owner::AddressOwner(owner));
        let receiving_object_new1 =
            Object::with_id_owner_version_for_testing(obj_id, 1.into(), Owner::AddressOwner(owner));
        let receiving_object_arg0 =
            ObjectArg::Receiving(receiving_object_new0.compute_object_reference());
        let receive_object_transaction0 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(receiving_object_arg0)],
        );

        let receiving_object_arg1 =
            ObjectArg::Receiving(receiving_object_new1.compute_object_reference());
        let receive_object_transaction1 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(receiving_object_arg1)],
        );

        // scheduler should output no transaction yet since waiting on receiving object.
        execution_scheduler.enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // scheduler should output no transaction yet since waiting on receiving object.
        execution_scheduler.enqueue(
            vec![receive_object_transaction1.clone()],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 2);

        // Duplicate enqueue of receiving object is allowed.
        execution_scheduler.enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 2);

        // Notify scheduler that the receiving object 0 is available.
        state
            .get_cache_writer()
            .write_object_entry_for_test(receiving_object_new0.clone());

        // scheduler should output the transaction eventually now that the receiving object has become
        // available.
        rx_ready_certificates.recv().await.unwrap();

        // Notify scheduler that the receiving object 0 is available.
        state
            .get_cache_writer()
            .write_object_entry_for_test(receiving_object_new1.clone());

        // scheduler should output the transaction eventually now that the receiving object has become
        // available.
        rx_ready_certificates.recv().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn execution_scheduler_receiving_object_ready_notifications_multiple_of_same_receiving() {
        telemetry_subscribers::init_for_testing();
        // Initialize an authority state.
        let (owner, _keypair) = deterministic_random_account_key();
        let gas_objects: Vec<Object> = (0..10)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
        let state = init_state_with_objects(gas_objects.clone()).await;

        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (execution_scheduler, mut rx_ready_certificates) = make_execution_scheduler(&state);
        // scheduler should output no transaction.
        assert!(rx_ready_certificates.try_recv().is_err());
        // scheduler should be empty at the beginning.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);

        let obj_id = ObjectID::random();
        let receiving_object_new0 =
            Object::with_id_owner_version_for_testing(obj_id, 0.into(), Owner::AddressOwner(owner));
        let receiving_object_new1 =
            Object::with_id_owner_version_for_testing(obj_id, 1.into(), Owner::AddressOwner(owner));
        let receiving_object_arg0 =
            ObjectArg::Receiving(receiving_object_new0.compute_object_reference());
        let receive_object_transaction0 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(receiving_object_arg0)],
        );

        let receive_object_transaction01 = make_transaction(
            gas_objects[1].clone(),
            vec![CallArg::Object(receiving_object_arg0)],
        );

        let receiving_object_arg1 =
            ObjectArg::Receiving(receiving_object_new1.compute_object_reference());
        let receive_object_transaction1 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(receiving_object_arg1)],
        );

        // Enqueuing a transaction with a receiving object that is available at the time it is enqueued
        // should become immediately available.
        let gas_receiving_arg = ObjectArg::Receiving(gas_objects[3].compute_object_reference());
        let tx1 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(gas_receiving_arg)],
        );

        // scheduler should output no transaction yet since waiting on receiving object.
        execution_scheduler.enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // scheduler should output no transaction yet since waiting on receiving object.
        execution_scheduler.enqueue(
            vec![receive_object_transaction1.clone()],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 2);

        // Different transaction with a duplicate receiving object reference is allowed.
        // Both transaction's will be outputted once the receiving object is available.
        execution_scheduler.enqueue(
            vec![receive_object_transaction01.clone()],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 3);

        // Notify scheduler that the receiving object 0 is available.
        state
            .get_cache_writer()
            .write_object_entry_for_test(receiving_object_new0.clone());

        // scheduler should output both transactions depending on the receiving object now that the
        // transaction's receiving object has become available.
        rx_ready_certificates.recv().await.unwrap();

        rx_ready_certificates.recv().await.unwrap();

        // Only two transactions that were dependent on the receiving object should be output.
        assert!(rx_ready_certificates.try_recv().is_err());

        // Enqueue a transaction with a receiving object that is available at the time it is enqueued.
        // This should be immediately available.
        execution_scheduler.enqueue(vec![tx1.clone()], &state.epoch_store_for_testing());
        sleep(Duration::from_secs(1)).await;
        rx_ready_certificates.recv().await.unwrap();

        // Notify scheduler that the receiving object 0 is available.
        state
            .get_cache_writer()
            .write_object_entry_for_test(receiving_object_new1.clone());

        // scheduler should output the transaction eventually now that the receiving object has become
        // available.
        rx_ready_certificates.recv().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn execution_scheduler_receiving_object_ready_if_current_version_greater() {
        telemetry_subscribers::init_for_testing();
        // Initialize an authority state.
        let (owner, _keypair) = deterministic_random_account_key();
        let mut gas_objects: Vec<Object> = (0..10)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
        let receiving_object = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            10.into(),
            Owner::AddressOwner(owner),
        );
        gas_objects.push(receiving_object.clone());
        let state = init_state_with_objects(gas_objects.clone()).await;

        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (execution_scheduler, mut rx_ready_certificates) = make_execution_scheduler(&state);
        // scheduler should output no transaction.
        assert!(rx_ready_certificates.try_recv().is_err());
        // scheduler should be empty at the beginning.
        assert_eq!(execution_scheduler.num_pending_certificates(), 0);

        let receiving_object_new0 = Object::with_id_owner_version_for_testing(
            receiving_object.id(),
            0.into(),
            Owner::AddressOwner(owner),
        );
        let receiving_object_new1 = Object::with_id_owner_version_for_testing(
            receiving_object.id(),
            1.into(),
            Owner::AddressOwner(owner),
        );
        let receiving_object_arg0 =
            ObjectArg::Receiving(receiving_object_new0.compute_object_reference());
        let receive_object_transaction0 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(receiving_object_arg0)],
        );

        let receive_object_transaction01 = make_transaction(
            gas_objects[1].clone(),
            vec![CallArg::Object(receiving_object_arg0)],
        );

        let receiving_object_arg1 =
            ObjectArg::Receiving(receiving_object_new1.compute_object_reference());
        let receive_object_transaction1 = make_transaction(
            gas_objects[0].clone(),
            vec![CallArg::Object(receiving_object_arg1)],
        );

        // scheduler should output no transaction yet since waiting on receiving object.
        execution_scheduler.enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        );
        execution_scheduler.enqueue(
            vec![receive_object_transaction01.clone()],
            &state.epoch_store_for_testing(),
        );
        execution_scheduler.enqueue(
            vec![receive_object_transaction1.clone()],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        rx_ready_certificates.recv().await.unwrap();
        rx_ready_certificates.recv().await.unwrap();
        rx_ready_certificates.recv().await.unwrap();
        assert!(rx_ready_certificates.try_recv().is_err());
    }

    // Tests transaction cancellation logic in execution scheduler. Mainly tests that for cancelled transaction,
    // execution scheduler only waits for all non-shared objects to be available before outputting the transaction.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn execution_scheduler_with_cancelled_transactions() {
        // Initialize an authority state, with gas objects and 3 shared objects.
        let (owner, _keypair) = deterministic_random_account_key();
        let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), owner);
        let shared_object_1 = Object::shared_for_testing();
        let initial_shared_version_1 = shared_object_1.owner().start_version().unwrap();
        let shared_object_2 = Object::shared_for_testing();
        let initial_shared_version_2 = shared_object_2.owner().start_version().unwrap();
        let owned_object = Object::with_id_owner_for_testing(ObjectID::random(), owner);

        let state = init_state_with_objects(vec![
            gas_object.clone(),
            shared_object_1.clone(),
            shared_object_2.clone(),
            owned_object.clone(),
        ])
        .await;

        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (execution_scheduler, mut rx_ready_certificates) = make_execution_scheduler(&state);
        // scheduler should output no transaction.
        assert!(rx_ready_certificates.try_recv().is_err());

        // Enqueue one transaction with 2 shared object inputs and 1 owned input.
        let shared_object_arg_1 = ObjectArg::SharedObject {
            id: shared_object_1.id(),
            initial_shared_version: initial_shared_version_1,
            mutable: true,
        };
        let shared_object_arg_2 = ObjectArg::SharedObject {
            id: shared_object_2.id(),
            initial_shared_version: initial_shared_version_2,
            mutable: true,
        };

        // Changes the desired owned object version to a higher version. We will make it available later.
        let owned_version = 2000.into();
        let mut owned_ref = owned_object.compute_object_reference();
        owned_ref.1 = owned_version;
        let owned_object_arg = ObjectArg::ImmOrOwnedObject(owned_ref);

        let cancelled_transaction = make_transaction(
            gas_object.clone(),
            vec![
                CallArg::Object(shared_object_arg_1),
                CallArg::Object(shared_object_arg_2),
                CallArg::Object(owned_object_arg),
            ],
        );
        state
            .epoch_store_for_testing()
            .set_shared_object_versions_for_testing(
                cancelled_transaction.digest(),
                &[
                    (
                        (
                            shared_object_1.id(),
                            shared_object_1.owner().start_version().unwrap(),
                        ),
                        SequenceNumber::CANCELLED_READ,
                    ),
                    (
                        (
                            shared_object_2.id(),
                            shared_object_2.owner().start_version().unwrap(),
                        ),
                        SequenceNumber::CONGESTED,
                    ),
                ],
            )
            .unwrap();

        execution_scheduler.enqueue(
            vec![cancelled_transaction.clone()],
            &state.epoch_store_for_testing(),
        );

        // scheduler should output no transaction yet.
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Notify scheduler about availability of the owned object.
        let mut new_owned_object = owned_object.clone();
        new_owned_object
            .data
            .try_as_move_mut()
            .unwrap()
            .increment_version_to(owned_version);
        state
            .get_cache_writer()
            .write_object_entry_for_test(new_owned_object);

        // scheduler should output the transaction as soon as the owned object is available.
        let available_txn = rx_ready_certificates.recv().await.unwrap().certificate;
        assert_eq!(available_txn.digest(), cancelled_transaction.digest());

        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Notify scheduler about read-only transaction commit
        execution_scheduler.notify_commit(&available_txn);

        assert_eq!(execution_scheduler.num_pending_certificates(), 0);
    }
}
