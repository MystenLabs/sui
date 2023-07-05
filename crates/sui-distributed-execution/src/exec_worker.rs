use core::panic;
use std::collections::{HashSet, HashMap, VecDeque};
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use futures::StreamExt;
use futures::stream::FuturesUnordered;

use sui_adapter::adapter::MoveVM;
use sui_adapter::{execution_engine, execution_mode};
use sui_config::genesis::Genesis;
use sui_core::transaction_input_checker::get_gas_status_no_epoch_store_experimental;
use sui_protocol_config::ProtocolConfig;
use sui_types::storage::ObjectStore;
use sui_types::digests::ObjectDigest;
use sui_types::base_types::ObjectID;
use sui_types::epoch_data::EpochData;
use sui_types::message_envelope::Message;
use sui_types::messages::{InputObjectKind, InputObjects, TransactionDataAPI, VerifiedTransaction, TransactionKind};
use sui_types::metrics::LimitsMetrics;
use sui_types::temporary_store::{TemporaryStore, InnerTemporaryStore};
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::gas::SuiGasStatus;
use sui_adapter::adapter;
use sui_move_natives;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::types::*;


const MANAGER_CHANNEL_SIZE:usize = 64;

/// Returns the read set of a transction
/// Specifically, this is the set of input objects to the transaction. It excludes 
/// child objects that are determined at runtime, but includes all owned objects inputs
/// that must have their version numbers bumped.
fn get_read_set(tx: &VerifiedTransaction) -> HashSet<ObjectID> {
    let tx_data = tx.data().transaction_data();
    let input_object_kinds = tx_data
        .input_objects()
        .expect("Cannot get input object kinds");

    let mut read_set = HashSet::new();
    for kind in &input_object_kinds {
        match kind {
            InputObjectKind::MovePackage(id)
            | InputObjectKind::SharedMoveObject { id, .. }
            | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                read_set.insert(*id)
            }
        };
    }
    return read_set;
}

/// TODO: This makes use of ground_truth_effects, which is illegal; it is not something that is 
/// known a-priori before execution
/// Returns the write set of a transction
fn get_write_set(_tx: &VerifiedTransaction, ground_truth_effects: &TransactionEffects) -> HashSet<ObjectID> {

    let mut write_set: HashSet<ObjectID> = HashSet::new();

    let TransactionEffects::V1(tx_effects) = ground_truth_effects;

    let created: Vec<ObjectID> = tx_effects.created.clone()
        .into_iter()
        .map(|(object_ref, _)| object_ref.0)
        .collect();
    let mutated: Vec<ObjectID> = tx_effects.mutated.clone()
        .into_iter()
        .map(|(object_ref, _)| object_ref.0)
        .collect();
    let unwrapped: Vec<ObjectID> = tx_effects.unwrapped.clone()
        .into_iter()
        .map(|(object_ref, _)| object_ref.0)
        .collect();
    let deleted: Vec<ObjectID> = tx_effects.deleted.clone()
        .into_iter()
        .map(|object_ref| object_ref.0)
        .collect();
    let unwrapped_then_deleted: Vec<ObjectID> = tx_effects.unwrapped_then_deleted.clone()
        .into_iter()
        .map(|object_ref| object_ref.0)
        .collect();
    let wrapped: Vec<ObjectID> = tx_effects.wrapped.clone()
        .into_iter()
        .map(|object_ref| object_ref.0)
        .collect();

    write_set.extend(created);
    write_set.extend(mutated);
    write_set.extend(unwrapped);
    write_set.extend(deleted);
    write_set.extend(unwrapped_then_deleted);
    write_set.extend(wrapped);
    return write_set;
}


/// Returns the read-write set of the transaction
fn get_read_write_set(tx: &VerifiedTransaction, tx_effects: &TransactionEffects) -> HashSet<ObjectID> {
    get_read_set(tx)
            .union(&mut get_write_set(tx, tx_effects))
            .copied()
            .collect()
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub tx: VerifiedTransaction,
    pub ground_truth_effects: TransactionEffects,  // full effects of tx, as ground truth exec result
    pub checkpoint_seq: u64,
}

pub struct TransactionWithResults {
    full_tx: Transaction,
    inner_temp_store: InnerTemporaryStore,  // determined after execution
    tx_effects: TransactionEffects,            // determined after execution
}


pub struct QueuesManager {
    tx_store: HashMap<TransactionDigest, Transaction>,
    obj_queues: HashMap<ObjectID, VecDeque<TransactionDigest>>,
    wait_table: HashMap<TransactionDigest, HashSet<ObjectID>>,
    ready: mpsc::Sender<Transaction>,
}

impl QueuesManager {
    fn new(manager_sender: mpsc::Sender<Transaction>) -> QueuesManager {
        QueuesManager { 
            tx_store: HashMap::new(), 
            obj_queues: HashMap::new(),
            wait_table: HashMap::new(),
            ready: manager_sender}
    }

    /// Enqueues a transaction on the manager
    async fn queue_tx(&mut self, full_tx: Transaction) {
       
		// Store tx
        let txid = *full_tx.tx.digest();
		self.tx_store.insert(txid, full_tx.clone());

        // Get RW set
        let rw_set = get_read_write_set(&full_tx.tx, &full_tx.ground_truth_effects);
        
        // Add tx to object queues
        for obj in rw_set.iter() {
            if let Some(q) = self.obj_queues.get_mut(obj) {
                q.push_back(txid);
            } else {
                self.obj_queues.insert(*obj, [txid].into());
            }
        }

        // Update the wait table
        self.wait_table.insert(txid, HashSet::from_iter(rw_set.clone()));
        for obj in rw_set.iter() {
            let queue = self.obj_queues.get(obj).unwrap();
            if let Some(&head) = queue.front() {
                if head == txid {
                    self.wait_table.get_mut(&txid).unwrap().remove(obj);
                }
            }
		}

        // Check if ready
        if self.wait_table.get_mut(&txid).unwrap().is_empty() {
			self.wait_table.remove(&txid);
            self.ready.send(full_tx).await.expect("send failed");
		}
	}

    /// Cleans up after a completed transaction
	async fn clean_up(&mut self, completed_tx: &Transaction, rw_set: &HashSet<ObjectID>) {

        // Get digest and RW set
        let txid = completed_tx.tx.digest();
		
		// Remove tx from obj_queues
		for obj in rw_set.iter() {
            let queue = self.obj_queues.get_mut(obj).unwrap();
            assert!(*queue.front().unwrap() == *txid);  // sanity check
            queue.pop_front();
			
			// Update wait_table; advance wait status of txs waiting on obj
            if let Some(next_txid) = queue.front() {
                self.wait_table.get_mut(next_txid).unwrap().remove(obj);

                // Check if next_txid ready
                if self.wait_table.get_mut(next_txid).unwrap().is_empty() {
                    self.wait_table.remove(next_txid);
                    let next_tx = self.tx_store.get(next_txid).unwrap();
                    self.ready.send(next_tx.clone()).await.expect("send failed");
                }
            }
		}
        self.tx_store.remove(&txid);
    }
}


/*****************************************************************************************
 *                                    Execution Worker                                   *
 *****************************************************************************************/

pub struct ExecutionWorkerState {
    pub memory_store: Arc<MemoryBackedStore>,
}

impl ExecutionWorkerState {
    pub fn new(// protocol_config: &'a ProtocolConfig,
    ) -> Self {
        Self {
            memory_store: Arc::new(MemoryBackedStore::new()),
        }
    }

    pub fn init_store(&mut self, genesis: &Genesis) {
        for obj in genesis.objects() {
            self.memory_store
                .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
        }
    }

    // Helper: Returns Input objects by reading from the memory_store
    async fn read_input_objects_from_store(&self, 
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
                    self.memory_store
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
        &self,
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
        &mut self,
        inner_temp_store: InnerTemporaryStore,
    ) {
        // And now we mutate the store.
        // First delete:
        for obj_del in inner_temp_store.deleted {
            match obj_del.1 .1 {
                sui_types::storage::DeleteKind::Wrap => {
                    let wrap_tombstone =
                        (obj_del.0, obj_del.1 .0, ObjectDigest::OBJECT_DIGEST_WRAPPED);
                    let old_object = self.memory_store
                            .get_object(&obj_del.0)
                            .unwrap().unwrap();
                        self.memory_store.insert(obj_del.0, (wrap_tombstone, old_object)); // insert the old object with a wrapped tombstone
                }
                _ => {
                    self.memory_store.remove(obj_del.0);
                }
            }
        }
        for (obj_add_id, (oref, obj, _)) in inner_temp_store.written {
            self.memory_store.insert(obj_add_id, (oref, obj));
        }
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
        let checkpoint_seq = full_tx.checkpoint_seq;
        let tx_effects = full_tx.ground_truth_effects.clone();
        let tx_data = tx.data().transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let input_objects = self.read_input_objects_from_store(tx).await;
        let gas_status = self.get_gas_status(tx, &input_objects, protocol_config, reference_gas_price).await;
        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();

        let temporary_store = TemporaryStore::new(
            self.memory_store.clone(),
            input_objects,
            *tx.digest(),
            protocol_config,
        );

        let (inner_temp_store, effects, _execution_error) =
            execution_engine::execute_transaction_to_effects::<execution_mode::Normal, _>(
                shared_object_refs,
                temporary_store,
                kind,
                signer,
                &gas,
                *tx.digest(),
                transaction_dependencies,
                move_vm,
                gas_status,
                epoch_data,
                protocol_config,
                metrics.clone(),
                false,
                &HashSet::new(),
            );

        // Critical check: are the effects the same?
        if effects.digest() != tx_effects.digest() {
            println!("Effects mismatch at checkpoint {}", checkpoint_seq);
            let old_effects = tx_effects.clone();
            println!("Past effects: {:?}", old_effects);
            println!("New effects: {:?}", effects);
        }
        assert!(
            effects.digest() == tx_effects.digest(),
            "Effects digest mismatch"
        );

        // And now we mutate the store.
        self.write_updates_to_store(inner_temp_store);
    }


    async fn async_exec(
        full_tx: Transaction,
        input_objects: InputObjects,
        temporary_store: TemporaryStore<Arc<MemoryBackedStore>>,
        move_vm: Arc<MoveVM>,
        gas_status: SuiGasStatus,
        epoch_data: EpochData,
        protocol_config: ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
    ) -> TransactionWithResults
    {
        let tx = &full_tx.tx;
        let tx_data = tx.data().transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();
    
        let (inner_temp_store, tx_effects, _execution_error) =
            execution_engine::execute_transaction_to_effects::<execution_mode::Normal, _>(
                shared_object_refs,
                temporary_store,
                kind,
                signer,
                &gas,
                *tx.digest(),
                transaction_dependencies,
                &move_vm,
                gas_status,
                &epoch_data,
                &protocol_config,
                metrics.clone(),
                false,
                &HashSet::new(),
            );
        
        return TransactionWithResults {
            full_tx,
            inner_temp_store,
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
        println!("Got epoch start message");

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
        ew_sender: mpsc::Sender<SailfishMessage>,
    ){
        // Start the initial epoch
        let (mut move_vm, mut protocol_config, mut epoch_data, mut reference_gas_price)
            = self.process_epoch_start(&mut sw_receiver).await;

        // Start timer for TPS computation
        let now = Instant::now();
        let mut num_tx: usize = 0;

        // Initialize channels
        let (manager_sender, mut manager_receiver) = mpsc::channel(MANAGER_CHANNEL_SIZE);
        let mut manager = QueuesManager::new(manager_sender);
        let mut tasks_queue = FuturesUnordered::<Pin<Box<dyn Future<Output = TransactionWithResults>>>>::new();

        // Main loop
        loop {
            tokio::select! {
                biased;

                // Must poll from manager_receiver before sw_receiver, to avoid deadlock
                Some(full_tx) = manager_receiver.recv() => {
                    
                    let tx = full_tx.tx.clone();
                    let input_objects = self.read_input_objects_from_store(&tx).await;
                    let gas_status = self.get_gas_status(&tx, &input_objects, &protocol_config, reference_gas_price).await;

                    // TODO Why does temp store has reference to memory store?
                    let temporary_store = TemporaryStore::new(
                        self.memory_store.clone(),
                        input_objects.clone(),
                        *tx.digest(),
                        &protocol_config,
                    );

                    // Push to FuturesUnordered queue
                    tasks_queue.push(Box::pin(
                        Self::async_exec(
                            full_tx,
                            input_objects,
                            temporary_store,
                            move_vm.clone(),
                            gas_status,
                            epoch_data.clone(),
                            protocol_config.clone(),
                            metrics.clone(),
                        ))
                    );
                },
                Some(msg) = sw_receiver.recv() => {
                    // New tx from sequencer; enqueue to manager
                    if let SailfishMessage::Transaction{
                        tx, 
                        tx_effects, 
                        checkpoint_seq,
                    } = msg {
                        let full_tx = Transaction{tx, ground_truth_effects: tx_effects, checkpoint_seq};
                        manager.queue_tx(full_tx).await;
                    } else {
                        panic!("unexpected message");
                    }
                },
                Some(tx_with_results) = tasks_queue.next() => {
                    num_tx += 1;

                    // 1. Critical check: are the effects the same?
                    let full_tx = &tx_with_results.full_tx;
                    let ground_truth_effects = &full_tx.ground_truth_effects;
                    let tx_effects = &tx_with_results.tx_effects;
                    if ground_truth_effects.digest() != tx_effects.digest() {
                        println!("Effects mismatch at checkpoint {}", full_tx.checkpoint_seq);
                        println!("Past effects: {:?}", ground_truth_effects);
                        println!("New effects: {:?}", tx_effects);
                    }
                    assert!(
                        ground_truth_effects.digest() == tx_effects.digest(),
                        "Effects digest mismatch"
                    );

                    // 2. Update the store
                    self.write_updates_to_store(tx_with_results.inner_temp_store);

                    // 3. Update object queues
                    let rw_set = get_read_write_set(&full_tx.tx, &tx_with_results.tx_effects);
                    manager.clean_up(&full_tx, &rw_set).await;


                    // 1. Check for effects match (done)
                    // 2. Update memory store
                    // 3. manager.clean_up()
                    // 4. deal with end-of-epoch


                    // Transation finished executing; run manager clean up
                    // manager.clean_up(tx).await;
                },
                else => {
                    println!("Error, abort");
                    break
                }
            }
        }

        // Print TPS
        let elapsed = now.elapsed();
        println!(
            "Execution worker TPS: {}",
            1000.0 * num_tx as f64 / elapsed.as_millis() as f64
        );

        self.sanity_check(manager);

        println!("Execution worker finished");
    }

    fn sanity_check(&self, qm: QueuesManager) {
        println!("Running sanity check...");

        // obj_queues should be empty
        for (obj, queue) in qm.obj_queues {
            assert!(queue.is_empty(), "Queue for {} isn't empty", obj);
        }

        // wait_table should be empty
        assert!(qm.wait_table.is_empty(), "Wait table isn't empty");
        
        println!("Done!");
    }
}


