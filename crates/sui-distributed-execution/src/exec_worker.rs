use core::panic;
use dashmap::DashMap;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_vm_runtime::move_vm::MoveVM;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use sui_adapter_latest::{adapter, execution_engine};
use sui_config::genesis::Genesis;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::transaction_input_checker::get_gas_status_no_epoch_store_experimental;
use sui_move_natives;
use sui_protocol_config::ProtocolConfig;
use sui_single_node_benchmark::benchmark_context::BenchmarkContext;
use sui_single_node_benchmark::workload::Workload;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::committee::EpochId;
use sui_types::digests::{ChainIdentifier, ObjectDigest, TransactionDigest};
use sui_types::effects::TransactionEffects;
use sui_types::epoch_data::EpochData;
use sui_types::error::SuiError;
use sui_types::execution_mode;
use sui_types::gas::{GasCharger, SuiGasStatus};
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::metrics::LimitsMetrics;
use sui_types::object::Object;
use sui_types::storage::{
    BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectStore, ParentSync, WriteKind,
};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use sui_types::temporary_store::TemporaryStore;
use sui_types::transaction::{InputObjectKind, InputObjects, SenderSignedData, TransactionDataAPI};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio::time::{sleep, Duration};

use crate::seqn_worker::{COMPONENT, WORKLOAD};
use crate::storage::WritableObjectStore;

use super::types::*;

const MANAGER_CHANNEL_SIZE: usize = 1024;

pub struct QueuesManager {
    tx_store: HashMap<TransactionDigest, TransactionWithEffects>,
    writing_tx: HashMap<ObjectID, TransactionDigest>,
    wait_table: HashMap<TransactionDigest, HashSet<TransactionDigest>>,
    reverse_wait_table: HashMap<TransactionDigest, HashSet<TransactionDigest>>,
    ready: mpsc::Sender<TransactionDigest>,
}

// The methods of the QueuesManager are called from a single thread, so no need for locks
impl QueuesManager {
    fn new(manager_sender: mpsc::Sender<TransactionDigest>) -> QueuesManager {
        QueuesManager {
            tx_store: HashMap::new(),
            writing_tx: HashMap::new(),
            wait_table: HashMap::new(),
            reverse_wait_table: HashMap::new(),
            ready: manager_sender,
        }
    }

    /// Enqueues a transaction on the manager
    async fn queue_tx(&mut self, full_tx: TransactionWithEffects) {
        let txid = full_tx.tx.digest();

        // Get RW set
        let r_set = full_tx.get_read_set();
        let w_set = full_tx.get_write_set();
        let mut wait_ctr = 0;

        // Add tx to wait lists
        for obj in r_set.union(&w_set) {
            let prev_write = self.writing_tx.insert(*obj, txid);
            if let Some(other_txid) = prev_write {
                self.wait_table.entry(txid).or_default().insert(other_txid);
                self.reverse_wait_table
                    .entry(other_txid)
                    .or_default()
                    .insert(txid);
                wait_ctr += 1;
            }
        }

        // Set this transaction as the current writer
        for obj in &w_set {
            self.writing_tx.insert(*obj, txid);
        }

        // Store tx
        self.tx_store.insert(txid, full_tx);

        // Set the wait table and check if tx is ready
        if wait_ctr == 0 {
            self.ready.send(txid).await.expect("send failed");
        }
    }

    /// Cleans up after a completed transaction
    async fn clean_up(&mut self, txid: &TransactionDigest) {
        let completed_tx = self.tx_store.remove(txid).unwrap();
        assert!(self.wait_table.get(txid).is_none());

        // Remove tx itself from objects where it is still marked as their current writer
        for obj in completed_tx.get_read_write_set().iter() {
            if let Some(t) = self.writing_tx.get(obj) {
                if t == txid {
                    self.writing_tx.remove(obj);
                }
            }
        }

        if let Some(waiting_txs) = self.reverse_wait_table.remove(txid) {
            for other_txid in waiting_txs {
                self.wait_table.get_mut(&other_txid).unwrap().remove(txid);
                if self.wait_table.get(&other_txid).unwrap().is_empty() {
                    self.wait_table.remove(&other_txid);
                    self.ready.send(other_txid).await.expect("send failed");
                }
            }
        }
    }

    fn get_tx(&self, txid: &TransactionDigest) -> &TransactionWithEffects {
        self.tx_store.get(txid).unwrap()
    }
}

/*****************************************************************************************
 *                                    Execution Worker                                   *
 *****************************************************************************************/

pub struct ExecutionWorkerState<
    S: ObjectStore
        + WritableObjectStore
        + BackingPackageStore
        + ParentSync
        + ChildObjectResolver
        + GetModule<Error = SuiError, Item = CompiledModule>
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
    ) {
        // And now we mutate the store.
        // First delete:
        for (id, (ver, kind)) in deleted {
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
            let old_effects = ground_truth_effects.clone();
            println!("Past effects: {:?}", old_effects);
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
        let mut gas_charger = GasCharger::new(tx.digest(), gas, gas_status, &protocol_config);

        let temporary_store = TemporaryStore::new(
            self.memory_store.clone(),
            input_objects.clone(),
            tx.digest(),
            protocol_config,
        );

        let (inner_temp_store, effects, _execution_error) =
            execution_engine::execute_transaction_to_effects::<execution_mode::Normal>(
                shared_object_refs,
                temporary_store,
                kind,
                signer,
                &mut gas_charger,
                tx.digest(),
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
        );
    }

    async fn async_exec(
        full_tx: TransactionWithEffects,
        memory_store: Arc<S>,
        child_inputs: HashSet<ObjectID>,
        move_vm: Arc<MoveVM>,
        reference_gas_price: u64,
        epoch_id: EpochId,
        epoch_start_timestamp: u64,
        protocol_config: ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        my_id: u8,
        ew_ids: &Vec<UniqueId>,
    ) -> TransactionWithResults {
        let tx = &full_tx.tx;
        let txid = tx.digest();
        let tx_data = tx.transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let input_objects = Self::read_input_objects_from_store(memory_store.clone(), &tx).await;
        let gas_status =
            Self::get_gas_status(&tx, &input_objects, &protocol_config, reference_gas_price).await;
        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();
        let mut gas_charger = GasCharger::new(tx.digest(), gas, gas_status, &protocol_config);
        // println!(
        //     "Dependencies for tx {}: {:?}",
        //     txid, transaction_dependencies
        // );
        let temporary_store = TemporaryStore::new(
            memory_store.clone(),
            input_objects.clone(),
            txid,
            &protocol_config,
        );

        let (inner_temp_store, tx_effects, _execution_error) =
            execution_engine::execute_transaction_to_effects::<execution_mode::Normal>(
                shared_object_refs,
                temporary_store,
                kind,
                signer,
                &mut gas_charger,
                txid,
                transaction_dependencies,
                &move_vm,
                &epoch_id,
                epoch_start_timestamp,
                &protocol_config,
                metrics.clone(),
                false,
                &HashSet::new(),
            );

        let mut missing_objs = HashSet::new();
        let input_object_map = input_objects.into_object_map();
        for read_obj_id in &inner_temp_store.runtime_read_objects {
            if !input_object_map.contains_key(read_obj_id) && !child_inputs.contains(read_obj_id) {
                missing_objs.insert(*read_obj_id);
            }
        }

        if missing_objs.is_empty() && my_id as UniqueId == ew_ids[0] {
            Self::write_updates_to_store(
                memory_store,
                inner_temp_store.deleted.clone(),
                inner_temp_store.written.clone(),
            );
        }

        return TransactionWithResults {
            full_tx,
            tx_effects,
            deleted: BTreeMap::from_iter(inner_temp_store.deleted),
            written: BTreeMap::from_iter(inner_temp_store.written),
            missing_objs,
        };
    }

    /// Helper: Receive and process an EpochStart message.
    /// Returns new (move_vm, protocol_config, epoch_data, reference_gas_price)
    async fn process_epoch_start(
        &self,
        in_channel: &mut mpsc::Receiver<NetworkMessage>,
    ) -> (Arc<MoveVM>, ProtocolConfig, EpochData, u64) {
        let msg = in_channel.recv().await.expect("Receiving doesn't work");
        let SailfishMessage::EpochStart{
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
                dst: sw_id,
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

    // async fn process_genesis_objects(&self, in_channel: &mut mpsc::Receiver<NetworkMessage>) {
    //     let msg = in_channel.recv().await.expect("Receiving doesn't work");
    //     let SailfishMessage::GenesisObjects(genesis_objects) = msg.payload
    //     else {
    //         eprintln!("EW got unexpected message: {:?}", msg.payload);
    //         panic!("unexpected message");
    //     };
    //     println!("EW got genesis objects message");

    //     for obj in genesis_objects {
    //         self.memory_store
    //             .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
    //     }
    // }

    async fn init_genesis_objects(&self, tx_count: u64) {
        let workload = Workload::new(tx_count, WORKLOAD);
        println!("Setting up accounts and gas...");
        let start_time = std::time::Instant::now();
        let ctx = BenchmarkContext::new(workload, COMPONENT, 0).await;
        let elapsed = start_time.elapsed().as_millis() as f64;
        println!(
            "Benchmark setup finished in {}ms at a rate of {} accounts/s",
            elapsed,
            1000f64 * workload.num_accounts() as f64 / elapsed
        );
        let genesis_objects = ctx.get_genesis_objects();
        for obj in genesis_objects {
            self.memory_store
                .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
        }
    }

    /// ExecutionWorker main
    pub async fn run(
        &mut self,
        metrics: Arc<LimitsMetrics>,
        tx_count: u64,
        in_channel: &mut mpsc::Receiver<NetworkMessage>,
        out_channel: &mpsc::Sender<NetworkMessage>,
        ew_ids: Vec<UniqueId>,
        _sw_id: UniqueId,
        my_id: UniqueId,
    ) {
        // Initialize channels
        let (manager_sender, mut manager_receiver) = mpsc::channel(MANAGER_CHANNEL_SIZE);
        let mut manager = QueuesManager::new(manager_sender);
        let mut tasks_queue: JoinSet<TransactionWithResults> = JoinSet::new();

        let num_ews = ew_ids.len() as u8;

        /* Semaphore to keep track of un-executed transactions in the current epoch, used
        * to schedule epoch change:
            1. epoch_txs_semaphore increments receive from sw; decrements when finish executing some tx.
            2. epoch_change_tx = Some(tx) when receive an epoch change tx from sw
            3. Do epoch change when epoch_change_tx is Some, and epoch_txs_semaphore is 0
            4. Reset semaphore after epoch change
        */
        let mut epoch_txs_semaphore = 0;
        let mut epoch_change_tx: Option<TransactionWithEffects> = None;

        if self.mode == ExecutionMode::Channel {
            // self.process_genesis_objects(in_channel).await;
            self.init_genesis_objects(tx_count).await;
        }
        // Start timer for TPS computation
        let mut num_tx: u64 = 0;
        let now = Instant::now();

        // if we execute in channel mode, there is no need to wait for epoch start
        let (mut move_vm, mut protocol_config, mut epoch_data, mut reference_gas_price) =
            match self.mode {
                ExecutionMode::Database => self.process_epoch_start(in_channel).await,
                ExecutionMode::Channel => {
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
            };

        // Main loop
        loop {
            tokio::select! {
                biased;
                Some(tx_with_results) = tasks_queue.join_next() => {
                    let tx_with_results = tx_with_results.expect("tx task failed");
                    let txid = tx_with_results.full_tx.tx.digest();

                    if !tx_with_results.missing_objs.is_empty() {
                        self.waiting_child_objs.entry(txid).or_default().extend(tx_with_results.missing_objs.iter());
                        self.ready_txs.insert(txid, ());
                        // println!("Sending MissingObjects message for tx {}", txid);
                        for ew_id in &ew_ids {
                            let msg = NetworkMessage { src: 0, dst: *ew_id, payload: SailfishMessage::MissingObjects {
                                txid,
                                ew: my_id as u8,
                                missing_objects: tx_with_results.missing_objs.clone()
                            }};
                            if out_channel.send(msg).await.is_err() {
                                eprintln!("EW {} could not send MissingObjects; EW {} already stopped.", my_id, ew_id);
                            }
                        }
                        continue;
                    }

                    self.locked_exec_count.remove(&txid);
                    self.received_objs.remove(&txid);
                    self.received_child_objs.remove(&txid);
                    self.waiting_child_objs.remove(&txid);

                    num_tx += 1;
                    epoch_txs_semaphore -= 1;
                    assert!(epoch_txs_semaphore >= 0);

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
                    manager.clean_up(&txid).await;

                    // println!("Sending TxResults message for tx {}", txid);
                    for ew_id in &ew_ids {
                        if *ew_id == my_id {
                            continue;
                        }
                        let msg = NetworkMessage { src: 0, dst: *ew_id, payload: SailfishMessage::TxResults {
                            txid,
                            deleted: tx_with_results.deleted.clone(),
                            written: tx_with_results.written.clone(),
                        }};
                        if out_channel.send(msg).await.is_err() {
                            eprintln!("EW {} could not send LockedExec; EW {} already stopped.", my_id, ew_id);
                        }
                    }
                    if num_tx == tx_count {
                        println!("EW {} executed {} txs", my_id, num_tx);
                        break;
                    }
                },
                // Must poll from manager_receiver before sw_receiver, to avoid deadlock
                Some(txid) = manager_receiver.recv() => {
                    let full_tx = manager.get_tx(&txid);
                    self.ready_txs.insert(txid, ());

                    let mut locked_objs = Vec::new();
                    for obj_id in full_tx.get_read_set() {
                        if my_id != select_ew_for_object(obj_id, &ew_ids) {
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

                    let execute_on_ew = select_ew_for_execution(txid, full_tx,&ew_ids);
                    let msg = NetworkMessage{
                        src:0,
                        dst:execute_on_ew as u16,
                        payload: SailfishMessage::LockedExec { txid, objects: locked_objs.clone(), child_objects: Vec::new() }};
                    // println!("Sending LockedExec for tx {} to EW {}", txid, execute_on_ew);
                    if out_channel.send(msg).await.is_err() {
                        eprintln!("EW {} could not send LockedExec; EW {} already stopped.", my_id, execute_on_ew);
                    }
                },
                Some(msg) = in_channel.recv() => {
                    let msg = msg.payload;
                    // println!("EW {} received {:?}", my_id, msg);
                    if let SailfishMessage::MissingObjects { txid, ew, missing_objects } = msg {
                        let mut locked_objs = Vec::new();
                        for obj_id in &missing_objects {
                            if my_id != select_ew_for_object(*obj_id, &ew_ids) {
                                continue;
                            }
                            let obj_ref_opt = self.memory_store.get_latest_parent_entry_ref(*obj_id).unwrap();
                            let obj_opt = self.memory_store.get_object(obj_id).unwrap();
                            if let (Some(obj_ref), Some(obj)) = (obj_ref_opt, obj_opt) {
                                locked_objs.push(Some((obj_ref, obj)));
                            } else {
                                locked_objs.push(None);
                            }
                        }
                        // println!("Sending LockedExec for tx {} in response to MissingObjects", txid);
                        let msg = NetworkMessage{
                            src:0,
                            dst:ew as u16,
                            payload:SailfishMessage::LockedExec { txid, objects: Vec::new(), child_objects: locked_objs.clone() }};
                        if out_channel.send(msg).await.is_err() {
                            eprintln!("EW {} could not send LockedExec; EW {} already stopped.", my_id, ew);
                        }
                    } else if let SailfishMessage::LockedExec { txid, mut objects, mut child_objects } = msg {
                        // TODO: deal with possible duplicate LockedExec messages
                        let mut list = self.received_objs.entry(txid).or_default();
                        list.append(&mut objects);

                        let mut ctr = self.locked_exec_count.entry(txid).or_insert(0);
                        *ctr += 1;
                        // println!("EW {} received LockedExec for tx {} (ctr={})", my_id, txid, *ctr);

                        let mut child_list = self.received_child_objs.entry(txid).or_default();
                        child_list.append(&mut child_objects);

                        if *ctr == num_ews && self.ready_txs.contains_key(&txid) && (self.waiting_child_objs.get(&txid).is_none() || child_list.len() == self.waiting_child_objs.get(&txid).unwrap().len()) {
                            let tx = manager.get_tx(&txid).clone();
                            self.ready_txs.remove(&txid);
                            *ctr = 0;

                            let mem_store = self.memory_store.clone();
                            let list = list.clone();
                            let child_list = child_list.clone();
                            let move_vm = move_vm.clone();
                            let epoch_data = epoch_data.clone();
                            let protocol_config = protocol_config.clone();
                            let metrics = metrics.clone();
                            let child_inputs = self.waiting_child_objs.get(&txid)
                                .map(|r| r.clone())
                                .unwrap_or_default();
                            // Push execution task to futures queue
                            let ew_ids_copy = ew_ids.clone();
                            tasks_queue.spawn(async move {
                                if child_list.is_empty() {
                                    for entry_opt in list.into_iter() {
                                        assert!(entry_opt.is_some(), "tx {} aborted, missing obj", txid);
                                        let (obj_ref, obj) = entry_opt.unwrap();
                                        mem_store.insert(obj_ref.0, (obj_ref, obj));
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
                                        mem_store.insert(obj_ref.0, (obj_ref, obj));
                                    }
                                    for obj_id in children_to_delete {
                                        if mem_store.get_object(&obj_id).unwrap().is_some() {
                                            mem_store.remove(obj_id);
                                        }
                                    }
                                }

                                // println!("EW {} executing tx {}", my_id, txid);
                                Self::async_exec(
                                    tx,
                                    mem_store,
                                    child_inputs,
                                    move_vm,
                                    reference_gas_price,
                                    epoch_data.epoch_id(),
                                    epoch_data.epoch_start_timestamp(),
                                    protocol_config,
                                    metrics,
                                    my_id as u8,
                                    &ew_ids_copy,
                                ).await
                            });
                        }
                    } else if let SailfishMessage::TxResults { txid, deleted, written } = msg {
                        if let Some(_) = self.ready_txs.remove(&txid) {
                            if my_id == ew_ids[0] {
                                Self::write_updates_to_store(self.memory_store.clone(), deleted, written);
                            }
                            manager.clean_up(&txid).await;
                            num_tx += 1;
                            if num_tx == tx_count {
                                println!("EW {} executed {} txs", my_id, num_tx);
                                break;
                            }
                            epoch_txs_semaphore -= 1;
                            assert!(epoch_txs_semaphore >= 0);
                        } else {
                            unreachable!("tx already executed though we did not send LockedExec");
                        }
                    } else if let SailfishMessage::ProposeExec(full_tx) = msg {
                        if full_tx.is_epoch_change() {
                            // don't queue to manager, but store to epoch_change_tx
                            epoch_change_tx = Some(full_tx);
                        } else {
                            manager.queue_tx(full_tx).await;
                            epoch_txs_semaphore += 1;
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

                    if my_id == select_ew_for_execution(txid, full_tx, &ew_ids) {
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
        let elapsed = now.elapsed();
        let tps = num_tx as f64 / elapsed.as_secs_f64();
        println!(
            "EW {} finished, executed {} txs ({:.2} tps)",
            my_id, num_tx, tps
        );
        sleep(Duration::from_millis(10_000)).await;
    }
}

fn select_ew_for_execution(
    txid: TransactionDigest,
    tx: &TransactionWithEffects,
    ew_ids: &Vec<UniqueId>,
) -> UniqueId {
    if tx.is_epoch_change() || tx.get_read_set().contains(&ObjectID::from_single_byte(5)) {
        ew_ids[0]
    } else {
        ew_ids[(txid.inner()[0] % 4) as usize]
    }
}

fn select_ew_for_object(_obj_id: ObjectID, ew_ids: &Vec<UniqueId>) -> UniqueId {
    ew_ids[0]
}
