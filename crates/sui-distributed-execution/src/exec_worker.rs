use core::panic;
use std::collections::{HashSet, HashMap, VecDeque, BTreeMap};
use std::sync::Arc;
use sui_adapter_latest::{adapter, execution_engine};
use move_vm_runtime::move_vm::MoveVM;
use sui_types::error::SuiError;
use sui_types::execution_mode;
use move_binary_format::CompiledModule;
use dashmap::DashMap;
// use sui_adapter::{adapter, execution_engine, execution_mode, adapter::MoveVM};
use sui_config::genesis::Genesis;
use sui_core::transaction_input_checker::get_gas_status_no_epoch_store_experimental;
use sui_protocol_config::ProtocolConfig;
use sui_types::storage::{BackingPackageStore, ParentSync, ChildObjectResolver, ObjectStore, WriteKind, DeleteKind};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::epoch_data::EpochData;
use sui_types::message_envelope::Message;
use sui_types::transaction::{InputObjectKind, InputObjects, TransactionDataAPI, VerifiedTransaction};
use sui_types::metrics::LimitsMetrics;
use sui_types::object::Object;
use sui_types::temporary_store::TemporaryStore;
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use sui_types::digests::{ObjectDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_move_natives;
use move_bytecode_utils::module_cache::GetModule;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::Instant;

use crate::storage::WritableObjectStore;

use super::types::*;

const MANAGER_CHANNEL_SIZE: usize = 1024;

pub struct QueuesManager {
    tx_store: HashMap<TransactionDigest, Transaction>,
    writing_tx: HashMap<ObjectID, TransactionDigest>,
    wait_table: HashMap<TransactionDigest, HashSet<TransactionDigest>>,
    reverse_wait_table: HashMap<TransactionDigest, HashSet<TransactionDigest>>,
    ready: mpsc::Sender<TransactionDigest>,
}

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
    async fn queue_tx(&mut self, full_tx: Transaction) {
        let txid = *full_tx.tx.digest();

        // Get RW set
        let r_set = full_tx.get_read_set();
        let w_set = full_tx.get_write_set();
        let mut wait_ctr = 0;
        
        // Add tx to wait lists
        for obj in r_set.union(&w_set) {
            let prev_write = self.writing_tx.insert(*obj, txid);
            if let Some(other_txid) = prev_write {
                self.wait_table.entry(txid).or_default().insert(other_txid);
                self.reverse_wait_table.entry(other_txid).or_default().insert(txid);
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

    fn get_tx(&self, txid: &TransactionDigest) -> &Transaction {
        self.tx_store.get(txid).unwrap()
    }
}


/*****************************************************************************************
 *                                    Execution Worker                                   *
 *****************************************************************************************/

pub struct ExecutionWorkerState
    <S: ObjectStore 
        + WritableObjectStore 
        + BackingPackageStore 
        + ParentSync 
        + ChildObjectResolver 
        + GetModule<Error = SuiError, Item = CompiledModule>
        + Send
        + Sync
        + 'static> 
{
    pub memory_store: Arc<S>,
    pub ready_txs: DashMap<TransactionDigest, ()>,
    pub waiting_child_objs: DashMap<TransactionDigest, HashSet<ObjectID>>,
    pub received_objs: DashMap<TransactionDigest, Vec<Option<(ObjectRef, Object)>>>,
    pub received_child_objs: DashMap<TransactionDigest, Vec<Option<(ObjectRef, Object)>>>,
    pub locked_exec_count: DashMap<TransactionDigest, u8>,
}

impl<S: ObjectStore + WritableObjectStore + BackingPackageStore + ParentSync + ChildObjectResolver + GetModule<Error = SuiError, Item = CompiledModule> + Send + Sync + 'static> 
    ExecutionWorkerState<S> {
    pub fn new(new_store: S) -> Self {
        Self {
            memory_store: Arc::new(new_store),
            ready_txs: DashMap::new(),
            waiting_child_objs: DashMap::new(),
            received_objs: DashMap::new(),
            received_child_objs: DashMap::new(),
            locked_exec_count: DashMap::new(),
        }
    }

    pub fn init_store(&mut self, genesis: &Genesis) {
        for obj in genesis.objects() {
            self.memory_store
                .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
        }
    }

    // Helper: Returns Input objects by reading from the memory_store
    async fn read_input_objects_from_store(
        memory_store: Arc<S>, 
        tx: &VerifiedTransaction
    ) -> InputObjects {
        let tx_data = tx.data().transaction_data();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("Cannot get input object kinds");

        let mut input_object_data = Vec::new();
        for kind in &input_object_kinds {
            let obj = match kind {
                InputObjectKind::MovePackage(id)
                | InputObjectKind::SharedMoveObject {id, .. }
                | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                    memory_store
                        .get_object(&id)
                        .unwrap()
                        .unwrap()
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
        tx: &VerifiedTransaction,
        input_objects: &InputObjects,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
    ) -> SuiGasStatus
    {
        let tx_data = tx.data().transaction_data();

        let input_object_data = 
            input_objects.clone()
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
            assert!(old_obj_opt.is_some(), "Trying to delete non-existant obj {}", id);
            let old_object = old_obj_opt.unwrap();
            match kind {
                sui_types::storage::DeleteKind::Wrap => {
                    // insert the old object with a wrapped tombstone
                    let wrap_tombstone =
                        (id, ver, ObjectDigest::OBJECT_DIGEST_WRAPPED);
                    memory_store.insert(id, (wrap_tombstone, old_object));
                }
                _ => { memory_store.remove(id); }
            }
        }
        for (id, (oref, obj, _)) in written {
            memory_store.insert(id, (oref, obj));
        }
    }

    fn check_effects_match(full_tx: &Transaction, effects: &TransactionEffects) -> bool {
        let ground_truth_effects = &full_tx.ground_truth_effects;
        if effects.digest() != ground_truth_effects.digest() {
            println!("EW effects mismatch for tx {} (CP {})", full_tx.tx.digest(), full_tx.checkpoint_seq);
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
        full_tx: &Transaction,
        protocol_config: &ProtocolConfig,
        move_vm: &Arc<MoveVM>,
        epoch_data: &EpochData,
        reference_gas_price: u64,
        metrics: Arc<LimitsMetrics>,
        ew_id: u8,
    ) {
        let tx = &full_tx.tx;
        let tx_data = tx.data().transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let input_objects = Self::read_input_objects_from_store(self.memory_store.clone(), tx).await;
        let gas_status = Self::get_gas_status(tx, &input_objects, protocol_config, reference_gas_price).await;
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
        Self::write_updates_to_store(self.memory_store.clone(), inner_temp_store.deleted, inner_temp_store.written);
    }

    async fn async_exec(
        full_tx: Transaction,
        memory_store: Arc<S>,
        child_inputs: HashSet<ObjectID>,
        move_vm: Arc<MoveVM>,
        reference_gas_price: u64,
        epoch_data: EpochData,
        protocol_config: ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        ew_id: u8,
    ) -> TransactionWithResults
    {
        let tx = &full_tx.tx;
        let txid = *tx.digest();
        let tx_data = tx.data().transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let input_objects = Self::read_input_objects_from_store(memory_store.clone(), &tx).await;
        let gas_status = Self::get_gas_status(&tx, &input_objects, &protocol_config, reference_gas_price).await;
        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();
        let mut gas_charger = GasCharger::new(*tx.digest(), gas, gas_status, &protocol_config);

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
                &gas,
                txid,
                transaction_dependencies,
                &move_vm,
                &epoch_data.epoch_id(),
                epoch_data.epoch_start_timestamp(),
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

        if missing_objs.is_empty() && ew_id == 0 {
            Self::write_updates_to_store(memory_store, inner_temp_store.deleted.clone(), inner_temp_store.written.clone());
        }
        
        return TransactionWithResults {
            full_tx,
            tx_effects,
            deleted: BTreeMap::from_iter(inner_temp_store.deleted),
            written: BTreeMap::from_iter(inner_temp_store.written),
            missing_objs,
        }
    }

    /// Helper: Receive and process an EpochStart message.
    /// Returns new (move_vm, protocol_config, epoch_data, reference_gas_price)
    async fn process_epoch_start(&self,
        sw_receiver: &mut mpsc::Receiver<SailfishMessage>,
    ) -> (Arc<MoveVM>, ProtocolConfig, EpochData, u64)
    {
        let SailfishMessage::EpochStart{
            conf: protocol_config,
            data: epoch_data,
            ref_gas_price: reference_gas_price,
        } = sw_receiver.recv().await.unwrap() 
        else {
            panic!("unexpected message");
        };
        println!("EW got epoch start message");

        let native_functions = sui_move_natives::all_natives(/* silent */ true);
        let move_vm = Arc::new(
            adapter::new_move_vm(native_functions, &protocol_config, false)
                .expect("We defined natives to not fail here"),
        );
        return (move_vm, protocol_config, epoch_data, reference_gas_price)
    }

    /// Helper: Process an epoch change
    async fn process_epoch_change(&self,
        ew_sender: &mpsc::Sender<SailfishMessage>,
        sw_receiver: &mut mpsc::Receiver<SailfishMessage>,
    ) -> (Arc<MoveVM>, ProtocolConfig, EpochData, u64)
    {
        // First send end of epoch message to sequence worker
        let latest_state = get_sui_system_state(&self.memory_store.clone())
            .expect("Read Sui System State object cannot fail");
        let new_epoch_start_state = latest_state.into_epoch_start_state();
        ew_sender
            .send(SailfishMessage::EpochEnd{
                new_epoch_start_state,
            }).await
            .expect("Sending doesn't work");

        // Then wait for start epoch message from sequence worker and update local state
        let (new_move_vm, protocol_config, epoch_data, reference_gas_price)
            = self.process_epoch_start(sw_receiver).await;

        return (new_move_vm, protocol_config, epoch_data, reference_gas_price);
    }

    /// ExecutionWorker main
    pub async fn run(&mut self,
        metrics: Arc<LimitsMetrics>,
        exec_watermark: u64,
        mut sw_receiver: mpsc::Receiver<SailfishMessage>,
        sw_sender: mpsc::Sender<SailfishMessage>,
        mut ew_receiver: mpsc::Receiver<SailfishMessage>,
        ew_senders: Vec<mpsc::Sender<SailfishMessage>>,
        ew_id: u8,
    ) {
        // Initialize channels
        let (manager_sender, mut manager_receiver) = mpsc::channel(MANAGER_CHANNEL_SIZE);
        let mut manager = QueuesManager::new(manager_sender);
        let mut tasks_queue: JoinSet<TransactionWithResults> = JoinSet::new();

        let num_ews = ew_senders.len() as u8;

        /* Semaphore to keep track of un-executed transactions in the current epoch, used
        * to schedule epoch change:
            1. epoch_txs_semaphore increments receive from sw; decrements when finish executing some tx.
            2. epoch_change_tx = Some(tx) when receive an epoch change tx from sw
            3. Do epoch change when epoch_change_tx is Some, and epoch_txs_semaphore is 0
            4. Reset semaphore after epoch change
        */
        let mut epoch_txs_semaphore = 0;
        let mut epoch_change_tx: Option<Transaction> = None;

        // Start timer for TPS computation
        let mut num_tx: usize = 0;
        let now = Instant::now();

        // Start the initial epoch
        let (mut move_vm, mut protocol_config, mut epoch_data, mut reference_gas_price)
            = self.process_epoch_start(&mut sw_receiver).await;

        // Main loop
        loop {
            tokio::select! {
                biased;
                Some(tx_with_results) = tasks_queue.join_next() => {
                    let tx_with_results = tx_with_results.expect("tx task failed");
                    let txid = *tx_with_results.full_tx.tx.digest();

                    if !tx_with_results.missing_objs.is_empty() {
                        self.waiting_child_objs.entry(txid).or_default().extend(tx_with_results.missing_objs.iter());
                        self.ready_txs.insert(txid, ());

                        for (i, sender) in ew_senders.iter().enumerate() {
                            let msg = SailfishMessage::MissingObjects {
                                txid,
                                ew: ew_id,
                                missing_objects: tx_with_results.missing_objs.clone()
                            };
                            if sender.send(msg).await.is_err() {
                                eprintln!("EW {} could not send LockedExec; EW {} already stopped.", ew_id, i);
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
                    if full_tx.checkpoint_seq % 10_000 == 0 {
                        println!("EW executed {}", full_tx.checkpoint_seq);
                    }

                    // 1. Critical check: are the effects the same?
                    let tx_effects = &tx_with_results.tx_effects;
                    Self::check_effects_match(full_tx, tx_effects);

                    // 2. Update object queues
                    manager.clean_up(&txid).await;

                    for (i, sender) in ew_senders.iter().enumerate() {
                        if i as u8 == ew_id {
                            continue;
                        }
                        let msg = SailfishMessage::TxResults {
                            txid,
                            deleted: tx_with_results.deleted.clone(),
                            written: tx_with_results.written.clone(),
                        };
                        if sender.send(msg).await.is_err() {
                            eprintln!("EW {} could not send LockedExec; EW {} already stopped.", ew_id, i);
                        }
                    }

                    // Stop executing when I hit the watermark
                    // Note that this is the high watermark; there may be lower txns not 
                    // completed still left in the tasks_queue
                    if full_tx.checkpoint_seq == exec_watermark-1 {
                        break;
                    }
                },
                // Must poll from manager_receiver before sw_receiver, to avoid deadlock
                Some(txid) = manager_receiver.recv() => {
                    let full_tx = manager.get_tx(&txid);
                    self.ready_txs.insert(txid, ());

                    let mut locked_objs = Vec::new();
                    for obj_id in full_tx.get_read_set() {
                        if ew_id != select_ew_for_object(obj_id) {
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

                    let execute_on_ew = select_ew_for_execution(txid, full_tx);
                    let msg = SailfishMessage::LockedExec { txid, objects: locked_objs.clone(), child_objects: Vec::new() };
                    if ew_senders[execute_on_ew as usize].send(msg).await.is_err() {
                        eprintln!("EW {} could not send LockedExec; EW {} already stopped.", ew_id, execute_on_ew);
                    }
                },
                Some(msg) = ew_receiver.recv() => {
                    if let SailfishMessage::MissingObjects { txid, ew, missing_objects } = msg {
                        let mut locked_objs = Vec::new();
                        for obj_id in &missing_objects {
                            if ew_id != select_ew_for_object(*obj_id) {
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
                        let msg = SailfishMessage::LockedExec { txid, objects: Vec::new(), child_objects: locked_objs.clone() };
                        if ew_senders[ew as usize].send(msg).await.is_err() {
                            eprintln!("EW {} could not send LockedExec; EW {} already stopped.", ew_id, ew);
                        }
                    } else if let SailfishMessage::LockedExec { txid, mut objects, mut child_objects } = msg {
                        // TODO: deal with possible duplicate LockedExec messages
                        let mut list = self.received_objs.entry(txid).or_default();
                        list.append(&mut objects);

                        let mut ctr = self.locked_exec_count.entry(txid).or_insert(0);
                        *ctr += 1;

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
                            tasks_queue.spawn(Box::pin(async move {
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

                                Self::async_exec(
                                    tx,
                                    mem_store,
                                    child_inputs,
                                    move_vm,
                                    reference_gas_price,
                                    epoch_data,
                                    protocol_config,
                                    metrics,
                                    ew_id,
                                ).await
                            }));
                        }
                    } else if let SailfishMessage::TxResults { txid, deleted, written } = msg {
                        if let Some(_) = self.ready_txs.remove(&txid) {
                            if ew_id == 0 {
                                Self::write_updates_to_store(self.memory_store.clone(), deleted, written);
                            }
                            manager.clean_up(&txid).await;
                            num_tx += 1;
                            epoch_txs_semaphore -= 1;
                            assert!(epoch_txs_semaphore >= 0);
                        } else {
                            unreachable!("tx already executed though we did not send LockedExec");
                        }
                    }
                },
                Some(msg) = sw_receiver.recv() => {
                    // New tx from sequencer; enqueue to manager
                    if let SailfishMessage::ProposeExec(full_tx) = msg {
                        if full_tx.is_epoch_change() {
                            // don't queue to manager, but store to epoch_change_tx
                            epoch_change_tx = Some(full_tx);
                        } else {
                            manager.queue_tx(full_tx).await;
                            epoch_txs_semaphore += 1;
                        }
                    } else {
                        eprintln!("EW {} received unexpected message from SW: {:?}", ew_id, msg);
                        panic!("unexpected message");
                    }
                },
                else => {
                    eprintln!("EW error, abort");
                    break
                }
            }

            // Maybe do epoch change, if every other tx has completed in this epoch
            if epoch_change_tx.is_some() && epoch_txs_semaphore == 0 {
                let full_tx = epoch_change_tx.as_ref().unwrap();
                let txid = *full_tx.tx.digest();

                if ew_id == select_ew_for_execution(txid, full_tx) {
                    self.execute_tx(
                        full_tx,
                        &protocol_config,
                        &move_vm,
                        &epoch_data,
                        reference_gas_price,
                        metrics.clone(),
                        ew_id,
                    ).await;

                    num_tx += 1;
                    if full_tx.checkpoint_seq % 10_000 == 0 {
                        println!("EW executed {}", full_tx.checkpoint_seq);
                    }

                    println!("EW END OF EPOCH at checkpoint {}", full_tx.checkpoint_seq);
                    (move_vm, protocol_config, epoch_data, reference_gas_price) = 
                        self.process_epoch_change(&sw_sender, &mut sw_receiver).await;
                } else {
                    (move_vm, protocol_config, epoch_data, reference_gas_price) = 
                        self.process_epoch_start(&mut sw_receiver).await;
                }

                epoch_change_tx = None;  // reset for next epoch
            }
        }

        // Print TPS
        let elapsed = now.elapsed();
        let tps = num_tx as f64 / elapsed.as_secs_f64();
        println!("EW {} finished, executed {} txs ({:.2} tps)", ew_id, num_tx, tps);
    }
}

fn select_ew_for_execution(txid: TransactionDigest, tx: &Transaction) -> u8 {
    if tx.is_epoch_change() || tx.get_read_set().contains(&ObjectID::from_single_byte(5)) {
        0
    } else {
        txid.inner()[0] % 4
    }
}

fn select_ew_for_object(obj_id: ObjectID) -> u8 {
    0
}
