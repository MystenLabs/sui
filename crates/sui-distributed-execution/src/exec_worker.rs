use core::panic;
use std::collections::{HashSet, HashMap, VecDeque};
use std::sync::Arc;

use sui_adapter_latest::{adapter, execution_engine};
use move_vm_runtime::move_vm::MoveVM;
use sui_types::error::SuiError;
use sui_types::execution_mode;
use move_binary_format::CompiledModule;
use sui_config::genesis::Genesis;
use sui_core::transaction_input_checker::get_gas_status_no_epoch_store_experimental;
use sui_protocol_config::ProtocolConfig;
use sui_types::storage::{BackingPackageStore, ParentSync, ChildObjectResolver, ObjectStore};
use sui_types::base_types::ObjectID;
use sui_types::epoch_data::EpochData;
use sui_types::message_envelope::Message;
use sui_types::transaction::{InputObjectKind, InputObjects, TransactionDataAPI, VerifiedTransaction};
use sui_types::metrics::LimitsMetrics;
use sui_types::temporary_store::{TemporaryStore, InnerTemporaryStore};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use sui_types::digests::{ObjectDigest, TransactionDigest};
use sui_types::effects::TransactionEffects;
use sui_types::gas::{SuiGasStatus, GasCharger};
use sui_move_natives;
use move_bytecode_utils::module_cache::GetModule;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::Instant;

use crate::storage::WritableObjectStore;

use super::types::*;

const MANAGER_CHANNEL_SIZE:usize = 1024;

enum QueueCell {
    Read(HashSet<TransactionDigest>),
    Write(TransactionDigest),
}

impl QueueCell {
    fn contains(&self, txid: &TransactionDigest) -> bool {
        match self {
            QueueCell::Read(txs) => txs.contains(txid),
            QueueCell::Write(tx) => tx == txid,
        }
    }
}

pub struct QueuesManager {
    tx_store: HashMap<TransactionDigest, Transaction>,
    obj_queues: HashMap<ObjectID, VecDeque<QueueCell>>,
    wait_table: HashMap<TransactionDigest, HashSet<ObjectID>>,
    ready: mpsc::Sender<Transaction>,
}

impl QueuesManager {
    fn new(manager_sender: mpsc::Sender<Transaction>) -> QueuesManager {
        QueuesManager { 
            tx_store: HashMap::new(), 
            obj_queues: HashMap::new(),
            wait_table: HashMap::new(),
            ready: manager_sender,
        }
    }

    /// Enqueues a transaction on the manager
    async fn queue_tx(&mut self, full_tx: Transaction) {
		// Store tx
        let txid = *full_tx.tx.digest();

        // Get read and write sets
        let r_set = full_tx.get_read_set();
        let w_set = full_tx.get_write_set();
        
        // Add tx to object queues
        for &obj in w_set.iter() {
            let cell = QueueCell::Write(txid);
            self.obj_queues.entry(obj).or_default().push_back(cell);
        }

        for obj in r_set.iter() {
            if w_set.contains(obj) {
                continue;
            }
            if let Some(q) = self.obj_queues.get_mut(obj) {
                match q.back_mut() {
                    // Limiting the number of reads per `QueueCell`
                    // prevents managing queues from becoming too expensive.
                    // Ideally, to still allow using the systems resources,
                    // this limit should be on the order of #threadsPerEW * #EWs.
                    Some(QueueCell::Read(txs)) if txs.len() < 64 => {
                        txs.insert(txid);
                    },
                    // if queue is empty or tail has write or tail has at least 64 reads
                    _ => q.push_back(QueueCell::Read([txid].into())),
                }
            } else {
                let cell = QueueCell::Read([txid].into());
                self.obj_queues.insert(*obj, [cell].into());
            }
        }

        // Update the wait table
        self.wait_table.insert(txid, HashSet::new());
        for obj in r_set.union(&w_set) {
            let queue = self.obj_queues.get(obj).unwrap();
            if !queue.is_empty() && !queue.front().unwrap().contains(&txid) {
                self.wait_table.get_mut(&txid).unwrap().insert(*obj);
            }
		}

        // Check if ready
        if self.wait_table.get_mut(&txid).unwrap().is_empty() {
			self.wait_table.remove(&txid);
            self.ready.send(full_tx).await.expect("send failed");
        } else {
            self.tx_store.insert(txid, full_tx);
        }
	}

    /// Cleans up after a completed transaction
	async fn clean_up(&mut self, completed_tx: &Transaction) {
        // Get digest and RW set
        let txid = completed_tx.tx.digest();
		
		// Remove tx from obj_queues
		for obj in completed_tx.get_read_write_set().iter() {
            let queue = self.obj_queues.get_mut(obj).unwrap();
            assert!(queue.front().unwrap().contains(txid));  // sanity check
            let unblocked = match queue.front_mut().unwrap() {
                QueueCell::Read(txs) if txs.len() > 1 => {
                    txs.remove(&txid);
                    false
                },
                _ => {
                    queue.pop_front();
                    true
                },
            };

            if !unblocked {
                continue;
            }
			
			// Update wait_table; advance wait status of txs waiting on obj
            if let Some(cell) = queue.front() {
                let next_txs = match cell {
                    QueueCell::Read(txs) => txs.clone(),
                    QueueCell::Write(tx) => [*tx].into(),
                };

                for next_txid in next_txs {
                    self.wait_table.get_mut(&next_txid).unwrap().remove(obj);

                    // Check if next_txid ready
                    if self.wait_table.get_mut(&next_txid).unwrap().is_empty() {
                        self.wait_table.remove(&next_txid);
                        let next_tx = self.tx_store.remove(&next_txid).unwrap();
                        self.ready.send(next_tx).await.expect("send failed");
                    }
                }
            }
		}
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
}

impl<S: ObjectStore + WritableObjectStore + BackingPackageStore + ParentSync + ChildObjectResolver + GetModule<Error = SuiError, Item = CompiledModule> + Send + Sync + 'static> 
    ExecutionWorkerState<S> {
    pub fn new(new_store: S) -> Self {
        Self {
            memory_store: Arc::new(new_store)
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
    ) -> SuiGasStatus {
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
        inner_temp_store: InnerTemporaryStore,
    ) {
        // And now we mutate the store.
        // First delete:
        for obj_del in inner_temp_store.deleted {
            match obj_del.1 .1 {
                sui_types::storage::DeleteKind::Wrap => {
                    let wrap_tombstone =
                        (obj_del.0, obj_del.1 .0, ObjectDigest::OBJECT_DIGEST_WRAPPED);
                    let old_object = memory_store
                            .get_object(&obj_del.0)
                            .unwrap().unwrap();
                    memory_store.insert(obj_del.0, (wrap_tombstone, old_object)); // insert the old object with a wrapped tombstone
                }
                _ => {
                    memory_store.remove(obj_del.0);
                }
            }
        }
        for (obj_add_id, (oref, obj, _)) in inner_temp_store.written {
            memory_store.insert(obj_add_id, (oref, obj));
        }
    }

    fn check_effects_match(full_tx: &Transaction, effects: &TransactionEffects) -> bool {
        let ground_truth_effects = &full_tx.ground_truth_effects;
        if effects.digest() != ground_truth_effects.digest() {
            println!("EW effects mismatch at checkpoint {}", full_tx.checkpoint_seq);
            let old_effects = ground_truth_effects.clone();
            println!("Past effects: {:?}", old_effects);
            println!("New effects: {:?}", effects);
        }
        assert!(
            effects.digest() == ground_truth_effects.digest(),
            "Effects digest mismatch"
        );
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
            input_objects,
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
        debug_assert!(Self::check_effects_match(&full_tx, &effects));

        // And now we mutate the store.
        Self::write_updates_to_store(self.memory_store.clone(), inner_temp_store);
    }


    async fn async_exec(
        full_tx: Transaction,
        memory_store: Arc<S>,
        move_vm: Arc<MoveVM>,
        reference_gas_price: u64,
        epoch_data: EpochData,
        protocol_config: ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
    ) -> TransactionWithResults {
        let tx = &full_tx.tx;
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
            *tx.digest(),
            &protocol_config,
        );
    
        let (inner_temp_store, tx_effects, _execution_error) =
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

        Self::write_updates_to_store(memory_store, inner_temp_store);
        
        return TransactionWithResults {
            full_tx,
            tx_effects,
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
            .send(SailfishMessage::EpochEnd {
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
        ew_sender: mpsc::Sender<SailfishMessage>,
    ) {
        // Initialize channels
        let (manager_sender, mut manager_receiver) = mpsc::channel(MANAGER_CHANNEL_SIZE);
        let mut manager = QueuesManager::new(manager_sender);
        let mut tasks_queue: JoinSet<TransactionWithResults> = JoinSet::new();

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
                    num_tx += 1;
                    epoch_txs_semaphore -= 1;
                    assert!(epoch_txs_semaphore >= 0);
                    
                    let full_tx = &tx_with_results.full_tx;
                    if full_tx.checkpoint_seq % 10_000 == 0 {
                        println!("EW executed {}", full_tx.checkpoint_seq);
                    }

                    // 1. Critical check: are the effects the same?
                    let tx_effects = &tx_with_results.tx_effects;
                    debug_assert!(Self::check_effects_match(full_tx, tx_effects));

                    // 2. Update object queues
                    manager.clean_up(&full_tx).await;

                    // Stop executing when I hit the watermark
                    // Note that this is the high watermark; there may be lower txns not 
                    // completed still left in the tasks_queue
                    if full_tx.checkpoint_seq == exec_watermark-1 {
                        break;
                    }
                },
                // Must poll from manager_receiver before sw_receiver, to avoid deadlock
                Some(full_tx) = manager_receiver.recv() => {
                    let mem_store = self.memory_store.clone();
                    let move_vm = move_vm.clone();
                    let epoch_data = epoch_data.clone();
                    let protocol_config = protocol_config.clone();
                    let metrics = metrics.clone();

                    // Push execution task to futures queue
                    tasks_queue.spawn(Box::pin(async move {
                        Self::async_exec(
                            full_tx,
                            mem_store,
                            move_vm,
                            reference_gas_price,
                            epoch_data,
                            protocol_config,
                            metrics,
                        ).await
                    }));
                },
                Some(msg) = sw_receiver.recv() => {
                    // New tx from sequencer; enqueue to manager
                    if let SailfishMessage::Transaction(full_tx) = msg {
                        if full_tx.is_epoch_change() {
                            // don't queue to manager, but store to epoch_change_tx
                            epoch_change_tx = Some(full_tx);
                        } else {
                            manager.queue_tx(full_tx).await;
                            epoch_txs_semaphore += 1;
                        }
                    } else {
                        panic!("unexpected message");
                    }
                },
                else => {
                    println!("EW error, abort");
                    break
                }
            }

            // Maybe do epoch change, if every other tx has completed in this epoch
            if epoch_change_tx.is_some() && epoch_txs_semaphore == 0 {
                let full_tx = epoch_change_tx.unwrap();
                self.execute_tx(
                    &full_tx,
                    &protocol_config,
                    &move_vm,
                    &epoch_data,
                    reference_gas_price,
                    metrics.clone(),
                ).await;

                num_tx += 1;
                if full_tx.checkpoint_seq % 10_000 == 0 {
                    println!("EW executed {}", full_tx.checkpoint_seq);
                }

                println!("EW END OF EPOCH at checkpoint {}", full_tx.checkpoint_seq);
                (move_vm, protocol_config, epoch_data, reference_gas_price) = 
                    self.process_epoch_change(&ew_sender, &mut sw_receiver).await;

                epoch_change_tx = None;  // reset for next epoch
            }
        }

        // Print TPS
        let elapsed = now.elapsed();
        println!("Execution worker finished");
        // self.sanity_check(manager);   
        println!(
            "Execution worker num executed: {}", num_tx);
        println!(
            "Execution worker TPS: {}",
            1000.0 * num_tx as f64 / elapsed.as_millis() as f64
        );
    }

    fn _sanity_check(&self, qm: QueuesManager) {
        println!("EW running sanity check...");

        // obj_queues should be empty
        for (obj, queue) in qm.obj_queues {
            assert!(queue.is_empty(), "Queue for {} isn't empty", obj);
        }

        // wait_table should be empty
        assert!(qm.wait_table.is_empty(), "Wait table isn't empty");
        
        println!("Passed!");
    }
}


