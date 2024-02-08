use core::panic;
use dashmap::DashMap;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_vm_runtime::move_vm::MoveVM;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use sui_adapter_latest::{adapter, execution_engine};
use sui_config::genesis::Genesis;
use sui_core::authority::authority_store_tables::LiveObject;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::transaction_input_checker::get_gas_status_no_epoch_store_experimental;
use sui_move_natives;
use sui_protocol_config::ProtocolConfig;
use sui_single_node_benchmark::benchmark_context::BenchmarkContext;
use sui_types::executable_transaction::VerifiedExecutableTransaction;

use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::digests::{ChainIdentifier, ObjectDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::epoch_data::EpochData;
use sui_types::error::SuiError;
use sui_types::execution_mode;
use sui_types::gas::{GasCharger, SuiGasStatus};
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::metrics::LimitsMetrics;
use sui_types::object::Object;
use sui_types::storage::{
    BackingPackageStore, ChildObjectResolver, DeleteKind, GetSharedLocks, ObjectStore, ParentSync,
    WriteKind,
};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use sui_types::temporary_store::TemporaryStore;
use sui_types::transaction::{
    InputObjectKind, InputObjects, SenderSignedData, Transaction, TransactionDataAPI,
    VerifiedTransaction,
};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

use super::types::*;
use crate::queue_manager::QueuesManager;
use crate::setup::generate_benchmark_ctx_workload;
use crate::setup::generate_benchmark_txs;
use crate::{metrics::Metrics, types::WritableObjectStore};

/*****************************************************************************************
 *                                    Execution Worker                                   *
 *****************************************************************************************/

pub struct ExecutionWorkerState<
    S: ObjectStore
        + WritableObjectStore
        + BackingPackageStore
        + ParentSync
        + ChildObjectResolver
        // + GetModule<Error = SuiError, Item = CompiledModule>
        + Send
        + Sync
        + 'static,
> {
    pub memory_store: Arc<S>,
    pub ready_txs: DashMap<TransactionDigest, ()>,
    pub waiting_child_objs: DashMap<TransactionDigest, HashSet<ObjectID>>,
    pub received_objs: DashMap<TransactionDigest, Vec<Option<(ObjectRef, Object)>>>,
    pub received_child_objs: DashMap<TransactionDigest, Vec<Option<(ObjectRef, Object)>>>,
    pub locked_exec_count: DashMap<TransactionDigest, u8>,
    pub genesis_digest: CheckpointDigest,
    pub mode: ExecutionMode,
}

impl<
        S: ObjectStore
            + WritableObjectStore
            + BackingPackageStore
            + ParentSync
            + ChildObjectResolver
            + GetModule<Error = SuiError, Item = CompiledModule>
            + GetSharedLocks
            + Send
            + Sync
            + 'static,
    > ExecutionWorkerState<S>
{
    pub fn new(new_store: S, genesis_digest: CheckpointDigest, mode: ExecutionMode) -> Self {
        Self {
            memory_store: Arc::new(new_store),
            ready_txs: DashMap::new(),
            waiting_child_objs: DashMap::new(),
            received_objs: DashMap::new(),
            received_child_objs: DashMap::new(),
            locked_exec_count: DashMap::new(),
            genesis_digest,
            mode,
        }
    }

    pub fn init_store(&mut self, genesis: Arc<&Genesis>) {
        for obj in genesis.objects() {
            self.memory_store
                .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
        }
    }

    // Helper: Returns Input objects by reading from the memory_store
    async fn read_input_objects_from_store(
        memory_store: Arc<S>,
        tx: &SenderSignedData,
    ) -> InputObjects {
        let tx_data = tx.transaction_data();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("Cannot get input object kinds");

        let mut input_object_data = Vec::new();
        for kind in &input_object_kinds {
            let obj = match kind {
                InputObjectKind::MovePackage(id)
                | InputObjectKind::SharedMoveObject { id, .. }
                | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                    memory_store.get_object(&id).unwrap().unwrap()
                }
            };
            input_object_data.push(obj);
        }

        InputObjects::new(
            input_object_kinds
                .into_iter()
                .zip(input_object_data.into_iter())
                .collect(),
        )
    }

    // Helper: Returns gas status
    async fn get_gas_status(
        tx: &SenderSignedData,
        input_objects: &InputObjects,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
    ) -> SuiGasStatus {
        let tx_data = tx.transaction_data();

        let input_object_data = input_objects
            .clone()
            .into_objects()
            .into_iter()
            .map(|(_kind, object)| object)
            .collect::<Vec<_>>();

        get_gas_status_no_epoch_store_experimental(
            &input_object_data,
            tx_data.gas(),
            protocol_config,
            reference_gas_price,
            &tx_data,
        )
        .await
        .expect("Could not get gas")
    }

    // Helper: Writes changes from inner_temp_store to memory store
    fn write_updates_to_store(
        memory_store: Arc<S>,
        deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
        written: BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
        my_id: UniqueId,
        ew_ids: &Vec<UniqueId>,
    ) {
        // And now we mutate the store.
        // First delete:
        for (id, (ver, kind)) in deleted {
            if get_ew_owner_for_object(id, ew_ids) != my_id as UniqueId {
                continue;
            }
            let old_obj_opt = memory_store.get_object(&id).unwrap();
            assert!(
                old_obj_opt.is_some(),
                "Trying to delete non-existant obj {}",
                id
            );
            let old_object = old_obj_opt.unwrap();
            match kind {
                sui_types::storage::DeleteKind::Wrap => {
                    // insert the old object with a wrapped tombstone
                    let wrap_tombstone = (id, ver, ObjectDigest::OBJECT_DIGEST_WRAPPED);
                    memory_store.insert(id, (wrap_tombstone, old_object));
                }
                _ => {
                    memory_store.remove(id);
                }
            }
        }
        for (id, (oref, obj, _)) in written {
            if get_ew_owner_for_object(id, ew_ids) != my_id as UniqueId {
                continue;
            }
            memory_store.insert(id, (oref, obj));
        }
    }

    fn check_effects_match(full_tx: &TransactionWithEffects, effects: &TransactionEffects) -> bool {
        let ground_truth_effects = &full_tx.ground_truth_effects.as_ref().unwrap();
        if effects.digest() != ground_truth_effects.digest() {
            println!(
                "EW effects mismatch for tx {} (CP {})",
                full_tx.tx.digest(),
                full_tx.checkpoint_seq.unwrap()
            );
            // let old_effects = ground_truth_effects.clone();
            println!("Past effects: {:?}", ground_truth_effects);
            println!("New effects: {:?}", effects);
            panic!("Effects digest mismatch");
        }
        return true;
    }

    /// Executes a transaction, used for sequential, in-order execution
    pub async fn execute_tx(
        &mut self,
        full_tx: &TransactionWithEffects,
        protocol_config: &ProtocolConfig,
        move_vm: &Arc<MoveVM>,
        epoch_data: &EpochData,
        reference_gas_price: u64,
        metrics: Arc<LimitsMetrics>,
    ) {
        let tx = &full_tx.tx;
        let tx_data = tx.transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let input_objects =
            Self::read_input_objects_from_store(self.memory_store.clone(), tx).await;
        let gas_status =
            Self::get_gas_status(tx, &input_objects, protocol_config, reference_gas_price).await;
        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();
        let mut gas_charger = GasCharger::new(*tx.digest(), gas, gas_status, &protocol_config);

        let temporary_store = TemporaryStore::new(
            self.memory_store.clone(),
            input_objects.clone(),
            *tx.digest(),
            protocol_config,
        );

        let (inner_temp_store, effects, _execution_error) =
            execution_engine::execute_transaction_to_effects::<execution_mode::Normal>(
                shared_object_refs,
                temporary_store,
                kind,
                signer,
                &mut gas_charger,
                *tx.digest(),
                transaction_dependencies,
                &move_vm,
                &epoch_data.epoch_id(),
                epoch_data.epoch_start_timestamp(),
                &protocol_config,
                metrics.clone(),
                false,
                &HashSet::new(),
            );

        // Critical check: are the effects the same?
        Self::check_effects_match(&full_tx, &effects);

        // And now we mutate the store.
        Self::write_updates_to_store(
            self.memory_store.clone(),
            inner_temp_store.deleted,
            inner_temp_store.written,
            0,
            &[0 as UniqueId].to_vec(),
        );
    }

    /// Helper: Receive and process an EpochStart message.
    /// Returns new (move_vm, protocol_config, epoch_data, reference_gas_price)
    async fn process_epoch_start(
        &self,
        in_channel: &mut mpsc::Receiver<NetworkMessage>,
    ) -> (Arc<MoveVM>, ProtocolConfig, EpochData, u64) {
        let msg = in_channel.recv().await.expect("Receiving doesn't work");
        let SailfishMessage::EpochStart {
            version: protocol_version,
            data: epoch_data,
            ref_gas_price: reference_gas_price,
        } = msg.payload
        else {
            eprintln!("EW got unexpected message: {:?}", msg.payload);
            panic!("unexpected message");
        };
        println!("EW got epoch start message");

        let native_functions = sui_move_natives::all_natives(/* silent */ true);
        let chain = ChainIdentifier::chain(&ChainIdentifier::from(self.genesis_digest));
        let conf = ProtocolConfig::get_for_version(protocol_version, chain);
        let move_vm = Arc::new(
            adapter::new_move_vm(native_functions, &conf, false)
                .expect("We defined natives to not fail here"),
        );
        return (move_vm, conf, epoch_data, reference_gas_price);
    }

    /// Helper: Process an epoch change
    async fn process_epoch_change(
        &self,
        out_channel: &mpsc::Sender<NetworkMessage>,
        in_channel: &mut mpsc::Receiver<NetworkMessage>,
        sw_id: UniqueId,
    ) -> (Arc<MoveVM>, ProtocolConfig, EpochData, u64) {
        // First send end of epoch message to sequence worker
        let latest_state = get_sui_system_state(&self.memory_store.clone())
            .expect("Read Sui System State object cannot fail");
        let new_epoch_start_state = latest_state.into_epoch_start_state();
        out_channel
            .send(NetworkMessage {
                src: 0,
                dst: vec![sw_id],
                payload: SailfishMessage::EpochEnd {
                    new_epoch_start_state,
                },
            })
            .await
            .expect("Sending doesn't work");

        // Then wait for start epoch message from sequence worker and update local state
        let _ = in_channel.recv().await.expect("Receiving doesn't work");
        let (new_move_vm, protocol_config, epoch_data, reference_gas_price) =
            self.process_epoch_start(in_channel).await;

        return (
            new_move_vm,
            protocol_config,
            epoch_data,
            reference_gas_price,
        );
    }

    async fn init_genesis_objects(
        &self,
        tx_count: u64,
        duration: Duration,
    ) -> (Vec<Transaction>, BenchmarkContext) {
        let (ctx, workload) = generate_benchmark_ctx_workload(tx_count, duration).await;
        let (ctx, _, txs) = generate_benchmark_txs(workload, ctx).await;

        let objects: HashMap<_, _> = ctx
            .validator()
            .get_validator()
            .database
            .iter_live_object_set(false)
            .map(|o| match o {
                LiveObject::Normal(object) => (object.id(), object),
                LiveObject::Wrapped(_) => unreachable!(),
            })
            .collect();

        for (id, obj) in objects {
            self.memory_store
                .insert(id, (obj.compute_object_reference(), obj));
        }

        (txs, ctx)
    }

    /// ExecutionWorker main
    pub async fn run(
        &mut self,
        metrics: Arc<LimitsMetrics>,
        tx_count: u64,
        duration: Duration,
        in_channel: &mut mpsc::Receiver<NetworkMessage>,
        out_channel: &mpsc::Sender<NetworkMessage>,
        ew_ids: Vec<UniqueId>,
        _sw_id: UniqueId,
        my_id: UniqueId,
        worker_metrics: Arc<Metrics>,
    ) {
        // Initialize channels
        let (ready_tx_sender, mut ready_tx_receiver) = mpsc::unbounded_channel();
        let (new_tx_sender, new_tx_receiver) = mpsc::unbounded_channel();
        let (done_tx_sender, done_tx_receiver) = mpsc::unbounded_channel();

        let mut manager = QueuesManager::new(new_tx_receiver, ready_tx_sender, done_tx_receiver);
        tokio::spawn(async move { manager.run().await });

        let mut tasks_queue: JoinSet<TransactionWithResults> = JoinSet::new();

        // let num_ews = ew_ids.len() as u8;

        /* Semaphore to keep track of un-executed transactions in the current epoch, used
        * to schedule epoch change:
            1. epoch_txs_semaphore increments receive from sw; decrements when finish executing some tx.
            2. epoch_change_tx = Some(tx) when receive an epoch change tx from sw
            3. Do epoch change when epoch_change_tx is Some, and epoch_txs_semaphore is 0
            4. Reset semaphore after epoch change
        */
        let epoch_txs_semaphore = 0;
        let mut epoch_change_tx: Option<TransactionWithEffects> = None;

        // Start timer for TPS computation
        let mut num_tx: u64 = 0;
        let mut before_count: u64 = 0;
        let mut after_count: u64 = 0;
        // let now = Instant::now();

        // if we execute in channel mode, there is no need to wait for epoch start
        let (mut move_vm, mut protocol_config, mut epoch_data, mut reference_gas_price) =
            match self.mode {
                ExecutionMode::Database => self.process_epoch_start(in_channel).await,
                ExecutionMode::Channel => init_execution_structures().await,
            };

        let context: Arc<BenchmarkContext> = if self.mode == ExecutionMode::Channel {
            // self.process_genesis_objects(in_channel).await;
            let (_, ctx) = self.init_genesis_objects(tx_count, duration).await;
            Arc::new(ctx)
        } else {
            unreachable!("Database mode not supported");
        };
        // Main loop
        loop {
            tokio::select! {
                biased;
                Some(tx_with_results) = tasks_queue.join_next() => {
                    let tx_with_results = tx_with_results.expect("tx task failed");
                    let txid = tx_with_results.full_tx.tx.digest();
                    // println!("EW {} executed tx {}", my_id, txid);
                    // TODO uncomment this when we have to send MissingObjects
                    // if !tx_with_results.missing_objs.is_empty() {
                    //     self.waiting_child_objs.entry(txid).or_default().extend(tx_with_results.missing_objs.iter());
                    //     self.ready_txs.insert(txid, ());
                    //     // println!("Sending MissingObjects message for tx {}", txid);
                    //     for ew_id in &ew_ids {
                    //         let msg = NetworkMessage { src: 0, dst: *ew_id, payload: SailfishMessage::MissingObjects {
                    //             txid,
                    //             ew: my_id as u8,
                    //             missing_objects: tx_with_results.missing_objs.clone()
                    //         }};
                    //         if out_channel.send(msg).await.is_err() {
                    //             eprintln!("EW {} could not send MissingObjects; EW {} already stopped.", my_id, ew_id);
                    //         }
                    //     }
                    //     continue;
                    // }

                    self.locked_exec_count.remove(&txid);
                    self.received_objs.remove(&txid);
                    self.received_child_objs.remove(&txid);
                    self.waiting_child_objs.remove(&txid);

                    num_tx += 1;
                    // epoch_txs_semaphore -= 1;
                    // assert!(epoch_txs_semaphore >= 0);

                    let full_tx = &tx_with_results.full_tx;
                    if full_tx.checkpoint_seq.is_some() {
                        println!("EW {} executed {}", my_id, full_tx.checkpoint_seq.unwrap());
                    }

                    // 1. Critical check: are the effects the same?
                    if full_tx.ground_truth_effects.is_some() {
                        let tx_effects = &tx_with_results.tx_effects;
                        Self::check_effects_match(full_tx, tx_effects);
                    }

                    // 2. Update object queues
                    done_tx_sender.send(*txid).expect("send failed");
                    // manager.clean_up(&txid).await;

                    // println!("Sending TxResults message for tx {}", txid);
                    let msg = NetworkMessage { src: 0, dst: ew_ids.iter()
                        .filter(|&&id| id != my_id)
                        .cloned()
                        .collect(), payload: SailfishMessage::TxResults {
                        txid: *txid,
                        deleted: tx_with_results.deleted.clone(),
                        written: tx_with_results.written.clone(),
                    }};
                    if out_channel.send(msg).await.is_err() {
                        eprintln!("EW {} could not send LockedExec.", my_id);
                    }

                    if num_tx % 10_000 == 0 {
                        println!("[task-queue] EW {my_id} executed {num_tx} txs");
                    }
                    if num_tx == 1 {
                        // Expose the start time as a metric. Should be done only once.
                        worker_metrics.register_start_time();
                    }
                    self.update_metrics(&tx_with_results.full_tx, &worker_metrics);
                },

                // Received a tx from the queue mananger -> the tx is ready to be executed
                // Must poll from manager_receiver before sw_receiver, to avoid deadlock
                // Some(txid) = manager_receiver.recv() => {
                //     let full_tx = manager.get_tx(&txid);
                Some(full_tx) = ready_tx_receiver.recv() => {
                    let txid = full_tx.tx.digest();
                    // println!("EW {} received ready tx {} from QM", my_id, txid);
                    self.ready_txs.insert(*txid, ());

                    let mut locked_objs = Vec::new();
                    for obj_id in full_tx.get_read_set() {
                        // println!("EW {} checking if obj {} is locked", my_id, obj_id);
                        if my_id != get_ew_owner_for_object(obj_id, &ew_ids) {
                            continue;
                        }
                        let obj_ref_opt = self.memory_store.get_latest_parent_entry_ref(obj_id).unwrap();
                        let obj_opt = self.memory_store.get_object(&obj_id).unwrap();
                        if let (Some(obj_ref), Some(obj)) = (obj_ref_opt, obj_opt) {
                            locked_objs.push(Some((obj_ref, obj)));
                        } else {
                            locked_objs.push(None);
                        }
                    }
                    // println!("Sending LockedExec for tx {}, locked_objs: {:?}", txid, locked_objs);

                    before_count += 1;
                    if before_count % 1_000 == 0 {
                        println!("[before_count] {before_count}");
                    }

                    let msg = NetworkMessage{
                        src:0,
                        dst:vec![get_designated_executor_for_tx(*txid, &full_tx,&ew_ids)],
                        payload: SailfishMessage::LockedExec { full_tx: full_tx.clone(), objects: locked_objs.clone(), child_objects: Vec::new() }};
                    // println!("EW {} Sending LockedExec for tx {} to EW {}", my_id, txid, execute_on_ew);
                    if out_channel.send(msg).await.is_err() {
                        eprintln!("EW {} could not send LockedExec; EW {} already stopped.", my_id, get_designated_executor_for_tx(*txid, &full_tx,&ew_ids));
                    }

                    after_count += 1;
                    if after_count % 1_000 == 0 {
                        println!("[after_count] {after_count}");
                    }
                },

                Some(msg) = in_channel.recv() => {
                    let msg = msg.payload;
                    // println!("EW {} received {:?}", my_id, msg);
                    if let SailfishMessage::MissingObjects { txid: _txid, ew: _ew, missing_objects: _missing_objects } = msg {
                        panic!("Should not receive MissingObjects with simple workload");
                        // let mut locked_objs = Vec::new();
                        // for obj_id in &missing_objects {
                        //     if my_id != get_ew_owner_for_object(*obj_id, &ew_ids) {
                        //         continue;
                        //     }
                        //     let obj_ref_opt = self.memory_store.get_latest_parent_entry_ref(*obj_id).unwrap();
                        //     let obj_opt = self.memory_store.get_object(obj_id).unwrap();
                        //     if let (Some(obj_ref), Some(obj)) = (obj_ref_opt, obj_opt) {
                        //         locked_objs.push(Some((obj_ref, obj)));
                        //     } else {
                        //         locked_objs.push(None);
                        //     }
                        // }
                        // // println!("Sending LockedExec for tx {} in response to MissingObjects", txid);
                        // let msg = NetworkMessage{
                        //     src:0,
                        //     dst:ew as u16,
                        //     payload:SailfishMessage::LockedExec { txid, objects: Vec::new(), child_objects: locked_objs.clone() }};
                        // if out_channel.send(msg).await.is_err() {
                        //     eprintln!("EW {} could not send LockedExec; EW {} already stopped.", my_id, ew);
                        // }
                    } else if let SailfishMessage::LockedExec { full_tx , mut objects, mut child_objects } = msg {

                        // TODO: deal with possible duplicate LockedExec messages
                        let tx = full_tx.clone();
                        let txid = tx.tx.digest().clone();
                        // println!("EW {} received LockedExec for tx {}", my_id, txid);
                        let mut list = self.received_objs.entry(txid).or_default();
                        list.append(&mut objects);

                        let mut ctr = self.locked_exec_count.entry(txid).or_insert(0);
                        *ctr += 1;

                        let mut child_list = self.received_child_objs.entry(txid).or_default();
                        child_list.append(&mut child_objects);

                        // if *ctr == num_ews && self.ready_txs.contains_key(&txid) && (self.waiting_child_objs.get(&txid).is_none() || child_list.len() == self.waiting_child_objs.get(&txid).unwrap().len()) {
                        if *ctr == get_ews_for_tx(&full_tx, &ew_ids).len() as u8 {
                            // let tx = manager.get_tx(&txid).clone();
                            // let tx = &full_tx;
                            *ctr = 0;

                            let memstore = self.memory_store.clone();
                            let list = list.clone();
                            let child_list = child_list.clone();
                            // let move_vm = move_vm.clone();
                            // let epoch_data = epoch_data.clone();
                            let protocol_config = protocol_config.clone();
                            // let metrics = metrics.clone();
                            let child_inputs = self.waiting_child_objs.get(&txid)
                            .map(|r| r.clone())
                            .unwrap_or_default();
                            // Push execution task to futures queue
                            let ew_ids_copy = ew_ids.clone();
                            let context_copy = context.clone();

                            self.ready_txs.remove(&txid);

                            tasks_queue.spawn(async move {
                                if child_list.is_empty() {
                                    for entry_opt in list.into_iter() {
                                        assert!(entry_opt.is_some(), "tx {} aborted, missing obj", txid);
                                        let (obj_ref, obj) = entry_opt.unwrap();
                                        memstore.insert(obj_ref.0, (obj_ref, obj));
                                    }
                                } else {
                                    // Ensure None values from child_inputs are deleted from mem_store!
                                    let mut children_to_delete = child_inputs.clone();
                                    for entry_opt in child_list.into_iter() {
                                        if entry_opt.is_none() {
                                            continue;
                                        }
                                        let (obj_ref, obj) = entry_opt.unwrap();
                                        children_to_delete.remove(&obj_ref.0);
                                        memstore.insert(obj_ref.0, (obj_ref, obj));
                                    }
                                    for obj_id in children_to_delete {
                                        if memstore.get_object(&obj_id).unwrap().is_some() {
                                            memstore.remove(obj_id);
                                        }
                                    }
                                }

                                Self::async_exec2(tx, memstore, &protocol_config, reference_gas_price, &context_copy, my_id, &ew_ids_copy)
                            });
                        }
                    } else if let SailfishMessage::TxResults { txid, deleted, written } = msg {
                        // println!("EW {} received TxResults for tx {}", my_id, txid);
                        if let Some(_) = self.ready_txs.remove(&txid) {
                            // manager.clean_up(&txid).await;
                            done_tx_sender.send(txid).expect("send failed");
                        }
                        // else {
                        //     unreachable!("tx already executed though we did not send LockedExec");
                        // }

                        if get_ews_for_deleted_written(&deleted, &written, &ew_ids).contains(&(my_id as UniqueId)) {
                            Self::write_updates_to_store(self.memory_store.clone(), deleted, written, my_id, &ew_ids);
                        }

                        num_tx += 1;
                        if num_tx % 10_000 == 0 {
                            tracing::debug!("[tx-results] EW {my_id} executed {num_tx} txs");
                        }
                        if num_tx == 1 {
                            // Expose the start time as a metric. Should be done only once.
                            worker_metrics.register_start_time();
                        }
                        // if num_tx == tx_count {
                        //     println!("EW {} executed {} txs", my_id, num_tx);
                        //     break;
                        // }
                        // epoch_txs_semaphore -= 1;
                        // assert!(epoch_txs_semaphore >= 0);
                    } else if let SailfishMessage::ProposeExec(full_tx) = msg {
                        // println!("EW {} received propose exec for tx {}", my_id, full_tx.tx.digest());
                        if full_tx.is_epoch_change() {
                            // don't queue to manager, but store to epoch_change_tx
                            epoch_change_tx = Some(full_tx);
                        } else {
                            // manager.queue_tx(full_tx).await;
                            new_tx_sender.send(full_tx).expect("send failed");
                            // epoch_txs_semaphore += 1;
                        }
                    } else {
                        eprintln!("EW {} received unexpected message from: {:?}", my_id, msg);
                        panic!("unexpected message");
                    }
                },
                else => {
                    eprintln!("EW error, abort");
                    break
                }
            }

            // Maybe do epoch change, if every other tx has completed in this epoch
            if epoch_change_tx.is_some() {
                if epoch_txs_semaphore == 0 {
                    let full_tx = epoch_change_tx.as_ref().unwrap();
                    let txid = full_tx.tx.digest();

                    if my_id == get_designated_executor_for_tx(*txid, full_tx, &ew_ids) {
                        self.execute_tx(
                            full_tx,
                            &protocol_config,
                            &move_vm,
                            &epoch_data,
                            reference_gas_price,
                            metrics.clone(),
                        )
                        .await;

                        num_tx += 1;
                        if full_tx.checkpoint_seq.unwrap() % 10_000 == 0 {
                            println!("EW {} executed {}", my_id, full_tx.checkpoint_seq.unwrap());
                        }

                        println!(
                            "EW {} END OF EPOCH at checkpoint {}",
                            my_id,
                            full_tx.checkpoint_seq.unwrap()
                        );
                        (move_vm, protocol_config, epoch_data, reference_gas_price) = self
                            .process_epoch_change(out_channel, in_channel, my_id)
                            .await;
                    } else {
                        (move_vm, protocol_config, epoch_data, reference_gas_price) =
                            self.process_epoch_start(in_channel).await;
                    }

                    epoch_change_tx = None; // reset for next epoch
                } else {
                    println!(
                        "Epoch change tx received, but semaphore is {}",
                        epoch_txs_semaphore
                    );
                }
            }
        }

        // Print TPS
        // let elapsed = now.elapsed().as_secs_f64();
        // let tps = num_tx as f64 / elapsed;
        // println!(
        //     "EW {} finished, executed {} txs ({:.2} tps)",
        //     my_id, num_tx, tps
        // );
        sleep(Duration::from_millis(1_000)).await;
    }

    fn update_metrics(&self, tx: &TransactionWithEffects, metrics: &Arc<Metrics>) {
        const WORKLOAD: &str = "default";
        metrics.register_transaction(tx.timestamp, WORKLOAD);
    }

    fn async_exec2(
        full_tx: TransactionWithEffects,
        memstore: Arc<S>,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        ctx: &BenchmarkContext,
        my_id: UniqueId,
        ew_ids: &Vec<UniqueId>,
    ) -> TransactionWithResults {
        let tx = full_tx.tx.clone();
        let executable = VerifiedExecutableTransaction::new_from_quorum_execution(
            VerifiedTransaction::new_unchecked(tx),
            0,
        );
        let (gas_status, input_objects) = sui_transaction_checks::check_certificate_input(
            &memstore,
            &*memstore,
            &executable,
            protocol_config,
            reference_gas_price,
        )
        .unwrap();
        let txid = executable.digest();
        let tx_data = executable.transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();
        let mut gas_charger =
            GasCharger::new(*executable.digest(), gas, gas_status, protocol_config);
        let temporary_store = TemporaryStore::new(
            memstore.clone(),
            input_objects.clone(),
            *txid,
            protocol_config,
        );

        let validator = ctx.validator();
        let (inner_temp_store, tx_effects, _) = validator
            .get_epoch_store()
            .executor()
            .execute_transaction_to_effects(
                validator.get_epoch_store().protocol_config(),
                validator.get_validator().metrics.limits_metrics.clone(),
                false,
                &HashSet::new(),
                &validator.get_epoch_store().epoch(),
                0,
                temporary_store,
                shared_object_refs,
                &mut gas_charger,
                kind,
                signer,
                *executable.digest(),
                transaction_dependencies,
            );
        assert!(tx_effects.status().is_ok());

        if get_ews_for_deleted_written(
            &inner_temp_store.deleted,
            &inner_temp_store.written,
            &ew_ids,
        )
        .contains(&(my_id as UniqueId))
        {
            Self::write_updates_to_store(
                memstore,
                inner_temp_store.deleted.clone(),
                inner_temp_store.written.clone(),
                my_id,
                ew_ids,
            );
        }

        TransactionWithResults {
            full_tx,
            tx_effects,
            deleted: BTreeMap::from_iter(inner_temp_store.deleted),
            written: BTreeMap::from_iter(inner_temp_store.written),
            missing_objs: HashSet::new(),
        }
    }
}

async fn init_execution_structures() -> (Arc<MoveVM>, ProtocolConfig, EpochData, u64) {
    let native_functions = sui_move_natives::all_natives(/* silent */ true);
    let validator = TestAuthorityBuilder::new().build().await;
    let epoch_store = validator.epoch_store_for_testing().clone();
    let protocol_config = epoch_store.protocol_config();
    let move_vm = Arc::new(
        adapter::new_move_vm(native_functions, protocol_config, false)
            .expect("We defined natives to not fail here"),
    );
    let epoch_data = EpochData::new_test();
    let reference_gas_price = epoch_store.reference_gas_price();
    (
        move_vm,
        protocol_config.clone(),
        epoch_data,
        reference_gas_price,
    )
}
