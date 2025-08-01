// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::{
        authority_per_epoch_store::AuthorityPerEpochStore,
        shared_object_version_manager::Schedulable, AuthorityMetrics, ExecutionEnv,
    },
    execution_cache::{ObjectCacheRead, TransactionCacheRead},
    execution_scheduler::{
        balance_withdraw_scheduler::{
            scheduler::BalanceWithdrawScheduler, BalanceSettlement, ScheduleStatus,
            TxBalanceWithdraw,
        },
        ExecutingGuard, PendingCertificateStats,
    },
};
use futures::stream::{FuturesUnordered, StreamExt};
use mysten_common::debug_fatal;
use mysten_metrics::spawn_monitored_task;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::Arc,
};
use sui_config::node::AuthorityOverloadConfig;
use sui_types::{
    base_types::{FullObjectID, SequenceNumber},
    error::SuiResult,
    executable_transaction::VerifiedExecutableTransaction,
    storage::InputKey,
    transaction::{SenderSignedData, TransactionDataAPI, TransactionKey},
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use tracing::{debug, error};

use super::{overload_tracker::OverloadTracker, ExecutionSchedulerAPI, PendingCertificate};

#[derive(Clone)]
pub struct ExecutionScheduler {
    object_cache_read: Arc<dyn ObjectCacheRead>,
    transaction_cache_read: Arc<dyn TransactionCacheRead>,
    overload_tracker: Arc<OverloadTracker>,
    tx_ready_certificates: UnboundedSender<PendingCertificate>,
    balance_withdraw_scheduler: Option<Arc<BalanceWithdrawScheduler>>,
    metrics: Arc<AuthorityMetrics>,
}

struct PendingGuard<'a> {
    scheduler: &'a ExecutionScheduler,
    cert: &'a VerifiedExecutableTransaction,
}

impl<'a> PendingGuard<'a> {
    pub fn new(scheduler: &'a ExecutionScheduler, cert: &'a VerifiedExecutableTransaction) -> Self {
        scheduler
            .metrics
            .transaction_manager_num_pending_certificates
            .inc();
        scheduler
            .overload_tracker
            .add_pending_certificate(cert.data());
        Self { scheduler, cert }
    }
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.scheduler
            .metrics
            .transaction_manager_num_pending_certificates
            .dec();
        self.scheduler
            .overload_tracker
            .remove_pending_certificate(self.cert.data());
    }
}

impl ExecutionScheduler {
    pub fn new(
        object_cache_read: Arc<dyn ObjectCacheRead>,
        transaction_cache_read: Arc<dyn TransactionCacheRead>,
        tx_ready_certificates: UnboundedSender<PendingCertificate>,
        balance_accumulator_enabled: bool,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        tracing::info!("Creating new ExecutionScheduler");
        let balance_withdraw_scheduler = if balance_accumulator_enabled {
            let starting_accumulator_version = object_cache_read
                .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
                .expect("Accumulator root object must be present if balance accumulator is enabled")
                .version();
            Some(BalanceWithdrawScheduler::new(
                Arc::new(object_cache_read.clone()),
                starting_accumulator_version,
            ))
        } else {
            None
        };
        Self {
            object_cache_read,
            transaction_cache_read,
            overload_tracker: Arc::new(OverloadTracker::new()),
            tx_ready_certificates,
            balance_withdraw_scheduler,
            metrics,
        }
    }

    async fn schedule_transaction(
        self,
        cert: VerifiedExecutableTransaction,
        execution_env: ExecutionEnv,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        let enqueue_time = Instant::now();
        let tx_digest = cert.digest();
        let digests = [*tx_digest];

        let tx_data = cert.transaction_data();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("input_objects() cannot fail");
        let input_object_keys: Vec<_> = epoch_store
            .get_input_object_keys(
                &cert.key(),
                &input_object_kinds,
                &execution_env.assigned_versions,
            )
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
        debug!(?tx_digest, "Scheduled transaction in execution scheduler");
        tracing::trace!(
            ?tx_digest,
            "Waiting for input objects: {:?}",
            input_and_receiving_keys
        );

        let availability = self
            .object_cache_read
            .multi_input_objects_available_cache_only(&input_and_receiving_keys);
        // Most of the times, the transaction's input objects are already available.
        // We can check the availability of the input objects first, and only wait for the
        // missing input objects if necessary.
        let missing_input_keys: Vec<_> = input_and_receiving_keys
            .into_iter()
            .zip(availability)
            .filter_map(|(key, available)| if !available { Some(key) } else { None })
            .collect();
        if missing_input_keys.is_empty() {
            self.metrics
                .transaction_manager_num_enqueued_certificates
                .with_label_values(&["ready"])
                .inc();
            debug!(?tx_digest, "Input objects already available");
            self.send_transaction_for_execution(&cert, execution_env, enqueue_time);
            return;
        }

        let _pending_guard = PendingGuard::new(&self, &cert);
        self.metrics
            .transaction_manager_num_enqueued_certificates
            .with_label_values(&["pending"])
            .inc();
        tokio::select! {
            _ = self.object_cache_read
                .notify_read_input_objects(&missing_input_keys, &receiving_object_keys, epoch)
                => {
                    self.metrics
                        .transaction_manager_transaction_queue_age_s
                        .observe(enqueue_time.elapsed().as_secs_f64());
                    debug!(?tx_digest, "Input objects available");
                    // TODO: Eventually we could fold execution_driver into the scheduler.
                    self.send_transaction_for_execution(
                        &cert,
                        execution_env,
                        enqueue_time,
                    );
                }
            _ = self.transaction_cache_read.notify_read_executed_effects_digests(
                "ExecutionScheduler::notify_read_executed_effects_digests",
                &digests,
            ) => {
                debug!(?tx_digest, "Transaction already executed");
            }
        };
    }

    fn send_transaction_for_execution(
        &self,
        cert: &VerifiedExecutableTransaction,
        execution_env: ExecutionEnv,
        enqueue_time: Instant,
    ) {
        let pending_cert = PendingCertificate {
            certificate: cert.clone(),
            execution_env,
            waiting_input_objects: BTreeSet::new(),
            stats: PendingCertificateStats {
                enqueue_time,
                ready_time: Some(Instant::now()),
            },
            executing_guard: Some(ExecutingGuard::new(
                self.metrics
                    .transaction_manager_num_executing_certificates
                    .clone(),
            )),
        };
        let _ = self.tx_ready_certificates.send(pending_cert);
    }

    fn schedule_balance_withdraws(
        &self,
        certs: Vec<(VerifiedExecutableTransaction, SequenceNumber, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        if certs.is_empty() {
            return;
        }
        let scheduler = self
            .balance_withdraw_scheduler
            .as_ref()
            .expect("Balance withdraw scheduler must be enabled if there are withdraws");
        let mut withdraws = BTreeMap::new();
        let mut prev_version = None;
        for (cert, version, _) in &certs {
            let tx_withdraws = cert
                .transaction_data()
                .process_balance_withdraws()
                .expect("Balance withdraws should have already been checked");
            assert!(!tx_withdraws.is_empty());
            if let Some(prev_version) = prev_version {
                // Transactions must be in order.
                assert!(prev_version <= *version);
            }
            prev_version = Some(*version);
            let tx_digest = *cert.digest();
            withdraws
                .entry(*version)
                .or_insert(Vec::new())
                .push(TxBalanceWithdraw {
                    tx_digest,
                    reservations: tx_withdraws,
                });
        }
        let mut receivers = FuturesUnordered::new();
        for (version, tx_withdraws) in withdraws {
            receivers.extend(scheduler.schedule_withdraws(version, tx_withdraws));
        }
        let scheduler = self.clone();
        let epoch_store = epoch_store.clone();
        spawn_monitored_task!(epoch_store.clone().within_alive_epoch(async move {
            let mut cert_map = HashMap::new();
            for (cert, _, env) in certs {
                cert_map.insert(*cert.digest(), (cert, env));
            }
            while let Some(result) = receivers.next().await {
                match result {
                    Ok(result) => match result.status {
                        ScheduleStatus::InsufficientBalance => {
                            let tx_digest = result.tx_digest;
                            debug!(
                                ?tx_digest,
                                "Balance withdraw scheduling result: Insufficient balance"
                            );
                            let (cert, env) = cert_map.remove(&tx_digest).expect("cert must exist");
                            let env = env.with_insufficient_balance();
                            scheduler.enqueue_transactions(vec![(cert, env)], &epoch_store);
                        }
                        ScheduleStatus::SufficientBalance => {
                            let tx_digest = result.tx_digest;
                            debug!(?tx_digest, "Balance withdraw scheduling result: Success");
                            let (cert, env) = cert_map.remove(&tx_digest).expect("cert must exist");
                            let env = env.with_sufficient_balance();
                            scheduler.enqueue_transactions(vec![(cert, env)], &epoch_store);
                        }
                        ScheduleStatus::AlreadyExecuted => {
                            let tx_digest = result.tx_digest;
                            debug!(?tx_digest, "Withdraw already executed");
                        }
                    },
                    Err(e) => {
                        error!("Withdraw scheduler stopped: {:?}", e);
                    }
                }
            }
        }));
    }

    fn schedule_settlement_transactions(
        &self,
        settlement_txns: Vec<(TransactionKey, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        if !settlement_txns.is_empty() {
            let scheduler = self.clone();
            let epoch_store = epoch_store.clone();

            spawn_monitored_task!(epoch_store.clone().within_alive_epoch(async move {
                let mut futures: FuturesUnordered<_> =
                        settlement_txns
                            .into_iter()
                            .map(|(key, env)| {
                                let epoch_store = epoch_store.clone();
                                async move {
                                    (epoch_store.wait_for_settlement_transactions(key).await, env)
                                }
                            })
                            .collect();

                while let Some((txns, env)) = futures.next().await {
                    let txns = txns
                        .into_iter()
                        .map(|tx| (tx, env.clone()))
                        .collect::<Vec<_>>();
                    scheduler.enqueue_transactions(txns, &epoch_store);
                }
            }));
        }
    }

    fn schedule_tx_keys(
        &self,
        tx_with_keys: Vec<(TransactionKey, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        if tx_with_keys.is_empty() {
            return;
        }

        let scheduler = self.clone();
        let epoch_store = epoch_store.clone();
        spawn_monitored_task!(epoch_store.clone().within_alive_epoch(async move {
            let tx_keys: Vec<_> = tx_with_keys.iter().map(|(key, _)| key).cloned().collect();
            let digests = epoch_store
                .notify_read_tx_key_to_digest(&tx_keys)
                .await
                .expect("db error");
            let transactions = scheduler
                .transaction_cache_read
                .multi_get_transaction_blocks(&digests)
                .into_iter()
                .map(|tx| {
                    let tx = tx.expect("tx must exist").as_ref().clone();
                    VerifiedExecutableTransaction::new_system(tx, epoch_store.epoch())
                })
                .zip(tx_with_keys.into_iter().map(|(_, env)| env))
                .collect::<Vec<_>>();
            scheduler.enqueue_transactions(transactions, &epoch_store);
        }));
    }

    /// When we schedule a certificate, it should be impossible for it to have been executed in a
    /// previous epoch.
    #[cfg(debug_assertions)]
    fn assert_cert_not_executed_previous_epochs(&self, cert: &VerifiedExecutableTransaction) {
        let epoch = cert.epoch();
        let digest = *cert.digest();
        let digests = [digest];
        let executed = self
            .transaction_cache_read
            .multi_get_executed_effects(&digests)
            .pop()
            .unwrap();
        // Due to pruning, we may not always have an executed effects for the certificate
        // even if it was executed. So this is a best-effort check.
        if let Some(executed) = executed {
            use sui_types::effects::TransactionEffectsAPI;

            assert_eq!(
                executed.executed_epoch(),
                epoch,
                "Transaction {} was executed in epoch {}, but scheduled again in epoch {}",
                digest,
                executed.executed_epoch(),
                epoch
            );
        }
    }
}

impl ExecutionSchedulerAPI for ExecutionScheduler {
    fn enqueue(
        &self,
        certs: Vec<(Schedulable, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        // schedule all transactions immediately
        let mut ordinary_txns = Vec::with_capacity(certs.len());
        let mut tx_with_keys = Vec::new();
        let mut tx_with_withdraws = Vec::new();
        let mut settlement_txns = Vec::new();

        for (schedulable, env) in certs {
            match schedulable {
                Schedulable::Transaction(tx) => {
                    ordinary_txns.push((tx, env));
                }
                s @ Schedulable::RandomnessStateUpdate(..) => {
                    tx_with_keys.push((s.key(), env));
                }
                Schedulable::Withdraw(tx, version) => {
                    tx_with_withdraws.push((tx, version, env));
                }
                Schedulable::AccumulatorSettlement(_, _) => {
                    settlement_txns.push((schedulable.key(), env));
                }
            }
        }

        self.enqueue_transactions(ordinary_txns, epoch_store);
        self.schedule_tx_keys(tx_with_keys, epoch_store);
        self.schedule_balance_withdraws(tx_with_withdraws, epoch_store);
        self.schedule_settlement_transactions(settlement_txns, epoch_store);
    }

    fn enqueue_transactions(
        &self,
        certs: Vec<(VerifiedExecutableTransaction, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        // Filter out certificates from wrong epoch.
        let certs: Vec<_> = certs
            .into_iter()
            .filter_map(|cert| {
                if cert.0.epoch() == epoch_store.epoch() {
                    #[cfg(debug_assertions)]
                    self.assert_cert_not_executed_previous_epochs(&cert.0);

                    Some(cert)
                } else {
                    debug_fatal!(
                        "We should never enqueue certificate from wrong epoch. Expected={} Certificate={:?}",
                        epoch_store.epoch(),
                        cert.0.epoch()
                    );
                    None
                }
            })
            .collect();
        let digests: Vec<_> = certs.iter().map(|(cert, _)| *cert.digest()).collect();
        let executed = self
            .transaction_cache_read
            .multi_get_executed_effects_digests(&digests);
        let mut already_executed_certs_num = 0;
        let pending_certs =
            certs
                .into_iter()
                .zip(executed)
                .filter_map(|((cert, execution_env), executed)| {
                    if executed.is_none() {
                        Some((cert, execution_env))
                    } else {
                        already_executed_certs_num += 1;
                        None
                    }
                });

        for (cert, execution_env) in pending_certs {
            let scheduler = self.clone();
            let epoch_store = epoch_store.clone();
            spawn_monitored_task!(
                epoch_store.within_alive_epoch(scheduler.schedule_transaction(
                    cert,
                    execution_env,
                    &epoch_store,
                ))
            );
        }

        self.metrics
            .transaction_manager_num_enqueued_certificates
            .with_label_values(&["already_executed"])
            .inc_by(already_executed_certs_num);
    }

    fn settle_balances(&self, settlement: BalanceSettlement) {
        self.balance_withdraw_scheduler
            .as_ref()
            .expect("Balance withdraw scheduler must be enabled if there are settlements")
            .settle_balances(settlement);
    }

    fn check_execution_overload(
        &self,
        overload_config: &AuthorityOverloadConfig,
        tx_data: &SenderSignedData,
    ) -> SuiResult {
        let inflight_queue_len = self.num_pending_certificates();
        self.overload_tracker
            .check_execution_overload(overload_config, tx_data, inflight_queue_len)
    }

    fn num_pending_certificates(&self) -> usize {
        (self
            .metrics
            .transaction_manager_num_pending_certificates
            .get()
            + self
                .metrics
                .transaction_manager_num_executing_certificates
                .get()) as usize
    }

    #[cfg(test)]
    fn check_empty_for_testing(&self) {
        assert_eq!(self.num_pending_certificates(), 0);
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

    use crate::authority::ExecutionEnv;
    use crate::authority::{authority_tests::init_state_with_objects, AuthorityState};
    use crate::execution_scheduler::{
        ExecutionSchedulerAPI, ExecutionSchedulerWrapper, SchedulingSource,
    };

    use super::{ExecutionScheduler, PendingCertificate};

    #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
    fn make_execution_scheduler(
        state: &AuthorityState,
    ) -> (
        ExecutionSchedulerWrapper,
        UnboundedReceiver<PendingCertificate>,
    ) {
        // Create a new execution scheduler instead of reusing the authority's, to examine
        // execution_scheduler output from rx_ready_certificates.
        let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
        // Do not call ExecutionSchedulerWrapper::new() here, because we want to always
        // construct an ExecutionScheduler in the tests here, not TransactionManager.
        let execution_scheduler =
            ExecutionSchedulerWrapper::ExecutionScheduler(ExecutionScheduler::new(
                state.get_object_cache_reader().clone(),
                state.get_transaction_cache_reader().clone(),
                tx_ready_certificates,
                false,
                state.metrics.clone(),
            ));

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
        execution_scheduler.enqueue_transactions(vec![], &state.epoch_store_for_testing());
        // scheduler should output no transaction.
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));

        // Enqueue a transaction with existing gas object, empty input.
        let transaction = make_transaction(gas_objects[0].clone(), vec![]);
        let tx_start_time = Instant::now();
        execution_scheduler.enqueue_transactions(
            vec![(
                transaction.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        // scheduler should output the transaction eventually.
        let pending_certificate = rx_ready_certificates.recv().await.unwrap();

        // Tests that pending certificate stats are recorded properly.
        assert!(pending_certificate.stats.enqueue_time >= tx_start_time);
        assert!(
            pending_certificate.stats.ready_time.unwrap() >= pending_certificate.stats.enqueue_time
        );

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Predent we have just executed the transaction.
        drop(pending_certificate);

        // scheduler should be empty.
        execution_scheduler.check_empty_for_testing();

        // Enqueue a transaction with a new gas object, empty input.
        let gas_object_new = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            0.into(),
            Owner::AddressOwner(owner),
        );
        let transaction = make_transaction(gas_object_new.clone(), vec![]);
        let tx_start_time = Instant::now();
        execution_scheduler.enqueue_transactions(
            vec![(
                transaction.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        // scheduler should output no transaction yet.
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));

        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // Duplicated enqueue is allowed.
        execution_scheduler.enqueue_transactions(
            vec![(
                transaction.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates
            .try_recv()
            .is_err_and(|err| err == TryRecvError::Empty));

        assert_eq!(execution_scheduler.num_pending_certificates(), 2);

        // Notify scheduler about availability of the gas object.
        state
            .get_cache_writer()
            .write_object_entry_for_test(gas_object_new);
        // scheduler should output the transaction eventually.
        // We will see both the original and the duplicated transaction.
        let pending_certificate = rx_ready_certificates.recv().await.unwrap();
        let pending_certificate2 = rx_ready_certificates.recv().await.unwrap();
        assert_eq!(
            pending_certificate.certificate.digest(),
            pending_certificate2.certificate.digest()
        );

        // Tests that pending certificate stats are recorded properly. The ready time should be
        // 2 seconds apart from the enqueue time.
        assert!(pending_certificate.stats.enqueue_time >= tx_start_time);
        assert!(
            pending_certificate.stats.ready_time.unwrap() - pending_certificate.stats.enqueue_time
                >= Duration::from_secs(2)
        );

        // Predent we have just executed the transaction.
        drop(pending_certificate);
        drop(pending_certificate2);

        // scheduler should be empty at the end.
        execution_scheduler.check_empty_for_testing();
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
        let tx_read_0_assigned_versions = vec![(
            (
                shared_object.id(),
                shared_object.owner().start_version().unwrap(),
            ),
            shared_version,
        )];
        let tx_read_1_assigned_versions = vec![(
            (
                shared_object.id(),
                shared_object.owner().start_version().unwrap(),
            ),
            shared_version,
        )];

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
        let tx_default_assigned_versions = vec![(
            (
                shared_object.id(),
                shared_object.owner().start_version().unwrap(),
            ),
            shared_version,
        )];

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
        let tx_read_2_assigned_versions = vec![
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
        ];

        execution_scheduler.enqueue_transactions(
            vec![
                (
                    transaction_read_0.clone(),
                    ExecutionEnv::new().with_assigned_versions(tx_read_0_assigned_versions),
                ),
                (
                    transaction_read_1.clone(),
                    ExecutionEnv::new().with_assigned_versions(tx_read_1_assigned_versions),
                ),
                (
                    transaction_default.clone(),
                    ExecutionEnv::new().with_assigned_versions(tx_default_assigned_versions),
                ),
                (
                    transaction_read_2.clone(),
                    ExecutionEnv::new().with_assigned_versions(tx_read_2_assigned_versions),
                ),
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

        execution_scheduler.check_empty_for_testing();
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
        execution_scheduler.check_empty_for_testing();

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
            execution_scheduler.enqueue_transactions(
                vec![(
                    txn.clone(),
                    ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
                )],
                &state.epoch_store_for_testing(),
            );
            sleep(Duration::from_secs(1)).await;
            assert!(rx_ready_certificates.try_recv().is_err());
            assert_eq!(execution_scheduler.num_pending_certificates(), i + 1);
        }

        // Now start to unravel the transactions by notifying that each subsequent
        // transaction has been processed.
        let len = object_arguments.len();
        for (i, (object, txn)) in object_arguments.into_iter().enumerate() {
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
            drop(txn);

            // scheduler should now output another transaction to run since it the next version of that object
            // has become available.
            assert_eq!(execution_scheduler.num_pending_certificates(), len - i - 1);
        }

        // After everything scheduler should be empty.
        execution_scheduler.check_empty_for_testing();
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
        execution_scheduler.check_empty_for_testing();

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
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction0.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // scheduler should output no transaction yet since waiting on receiving object.
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction1.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 2);

        // Duplicate enqueue of receiving object is allowed.
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction0.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 3);

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
        execution_scheduler.check_empty_for_testing();

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
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction0.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 1);

        // scheduler should output no transaction yet since waiting on receiving object.
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction1.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(execution_scheduler.num_pending_certificates(), 2);

        // Different transaction with a duplicate receiving object reference is allowed.
        // Both transaction's will be outputted once the receiving object is available.
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction01.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
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
        execution_scheduler.enqueue_transactions(
            vec![(
                tx1.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
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
        execution_scheduler.check_empty_for_testing();

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
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction0.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction01.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
            &state.epoch_store_for_testing(),
        );
        execution_scheduler.enqueue_transactions(
            vec![(
                receive_object_transaction1.clone(),
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            )],
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
        let assigned_versions = vec![
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
        ];

        execution_scheduler.enqueue_transactions(
            vec![(
                cancelled_transaction.clone(),
                ExecutionEnv::new().with_assigned_versions(assigned_versions),
            )],
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

        execution_scheduler.check_empty_for_testing();
    }
}
