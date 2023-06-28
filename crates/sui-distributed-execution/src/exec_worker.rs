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
use sui_types::base_types::{ObjectID, ExecutionDigests};
use sui_types::epoch_data::EpochData;
use sui_types::message_envelope::Message;
use sui_types::messages::{InputObjectKind, InputObjects, TransactionDataAPI, VerifiedTransaction, TransactionKind};
use sui_types::metrics::LimitsMetrics;
use sui_types::temporary_store::TemporaryStore;
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::digests::TransactionDigest;
use sui_adapter::adapter;
use sui_move_natives;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::types::*;


const MANAGER_CHANNEL_SIZE:usize = 64;

// TODO: Returns the read set of a transction
fn get_read_set(_tx: &VerifiedTransaction) -> Vec<ObjectID> {
    Vec::new()
}

// TODO: Returns the write set of a transction
fn get_write_set(_tx: &VerifiedTransaction) -> Vec<ObjectID> {
    Vec::new()
}

#[derive(Debug, Clone)]
pub struct Transaction {
    tx: VerifiedTransaction,
    exec_digest: ExecutionDigests,
    checkpoint_seq: u64,
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
        let mut rw_set = get_read_set(&full_tx.tx);
        rw_set.append(&mut get_write_set(&full_tx.tx));
        
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
	async fn clean_up(&mut self, completed_tx: &VerifiedTransaction) {

        // Get digest and RW set
        let txid = *completed_tx.digest();
        let mut rw_set = get_read_set(&completed_tx);
        rw_set.append(&mut get_write_set(&completed_tx));
		
		// Remove tx from obj_queues
		for obj in rw_set.iter() {
            let queue = self.obj_queues.get_mut(obj).unwrap();
            assert!(*queue.front().unwrap() == txid);  // sanity check
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

pub struct ExecutionWorkerState {
    pub memory_store: MemoryBackedStore,
}

impl ExecutionWorkerState {
    pub fn new(// protocol_config: &'a ProtocolConfig,
    ) -> Self {
        Self {
            memory_store: MemoryBackedStore::new(),
        }
    }

    pub fn init_store(&mut self, genesis: &Genesis) {
        for obj in genesis.objects() {
            self.memory_store
                .objects
                .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
        }
    }

    /// Executes a transaction
    pub async fn execute_tx(
        &mut self,
        tx: &VerifiedTransaction,
        tx_digest: &ExecutionDigests,
        checkpoint_seq: u64,
        protocol_config: &ProtocolConfig,
        move_vm: &Arc<MoveVM>,
        epoch_data: &EpochData,
        reference_gas_price: u64,
        metrics: Arc<LimitsMetrics>,
    ) -> VerifiedTransaction 
    {
        let tx_data = tx.data().transaction_data();
        let (kind, signer, gas) = tx_data.execution_parts();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("Cannot get input object kinds");

        let mut input_object_data = Vec::new();
        for kind in &input_object_kinds {
            let obj = match kind {
                InputObjectKind::MovePackage(id)
                | InputObjectKind::SharedMoveObject { id, .. }
                | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                    self.memory_store.objects.get(&id).expect("Object missing?")
                }
            };
            input_object_data.push(obj.1.clone());
        }

        let gas_status = get_gas_status_no_epoch_store_experimental(
            &input_object_data,
            tx_data.gas(),
            protocol_config,
            reference_gas_price,
            &tx_data,
        )
        .await
        .expect("Could not get gas");

        let input_objects = InputObjects::new(
            input_object_kinds
                .into_iter()
                .zip(input_object_data.into_iter())
                .collect(),
        );
        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();

        let temporary_store = TemporaryStore::new(
            &self.memory_store,
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
        if effects.digest() != tx_digest.effects {
            println!("Effects mismatch at checkpoint {}", checkpoint_seq);
            let old_effects = tx_digest.effects;
            println!("Past effects: {:?}", old_effects);
            println!("New effects: {:?}", effects);
        }
        assert!(
            effects.digest() == tx_digest.effects,
            "Effects digest mismatch"
        );

        // And now we mutate the store.
        // First delete:
        for obj_del in &inner_temp_store.deleted {
            self.memory_store.objects.remove(obj_del.0);
        }
        for (obj_add_id, (oref, obj, _)) in inner_temp_store.written {
            self.memory_store.objects.insert(obj_add_id, (oref, obj));
        }

        return tx.clone();
    }


    /// TODO: Dispatch a transaction to the execution queue
    pub fn execute_tx_async(
        &mut self,
        tx: &VerifiedTransaction,
        // tx_digest: &ExecutionDigests,
        // checkpoint_seq: u64,
        // protocol_config: &ProtocolConfig,
        // move_vm: &Arc<MoveVM>,
        // epoch_data: &EpochData,
        // reference_gas_price: u64,
        // metrics: Arc<LimitsMetrics>,
        _tasks_queue: &mut FuturesUnordered::<Pin<Box<dyn Future<Output = VerifiedTransaction>>>>,
    ) -> VerifiedTransaction 
    {
        // TODO: dispatch execution to tasks_queue
        return tx.clone();
    }


    /// Helper function to receive and process an EpochStart message.
    /// Returns new (move_vm, protocol_config, epoch_data, reference_gas_price)
    async fn process_epoch_start(&mut self,
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

    /// Helper function to process an epoch change
    async fn process_epoch_change(&mut self,
        ew_sender: &mpsc::Sender<SailfishMessage>,
        sw_receiver: &mut mpsc::Receiver<SailfishMessage>,
    ) -> (Arc<MoveVM>, ProtocolConfig, EpochData, u64)
    {
        // First send end of epoch message to sequence worker
        let latest_state = get_sui_system_state(&&self.memory_store)
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
        let mut tasks = FuturesUnordered::<Pin<Box<dyn Future<Output = VerifiedTransaction>>>>::new();

        // Main loop
        loop {
            tokio::select! {
                biased;

                // Must poll from manager_receiver before sw_receiver, to avoid deadlock
                Some(full_tx) = manager_receiver.recv() => {
                    // Transaction is ready to execute
                    let Transaction{tx, exec_digest, checkpoint_seq} = full_tx;
                    self.execute_tx(
                        &tx,
                        &exec_digest,
                        checkpoint_seq,
                        &protocol_config,
                        &move_vm,
                        &epoch_data,
                        reference_gas_price,
                        metrics.clone(),
                    ).await;
                    
                    num_tx += 1;
                    if checkpoint_seq % 10000 == 0 {
                        println!("Executed {}", checkpoint_seq);
                    }
                    // println!("Executed {}", checkpoint_seq);
                    // manager.clean_up(&tx).await;

                    if let TransactionKind::ChangeEpoch(_) = tx.data().transaction_data().kind() {
                        // Change epoch
                        println!("END OF EPOCH at checkpoint {}", checkpoint_seq);
                        (move_vm, protocol_config, epoch_data, reference_gas_price) = 
                            self.process_epoch_change(&ew_sender, &mut sw_receiver).await;
                    }

                    // Stop executing when I hit the watermark
                    if checkpoint_seq == exec_watermark-1 {
                        break;
                    }
                },
                Some(msg) = sw_receiver.recv() => {
                    // New tx from sequencer; enqueue to manager
                    if let SailfishMessage::Transaction{tx, digest: exec_digest, checkpoint_seq} = msg {
                        let full_tx = Transaction{tx, exec_digest, checkpoint_seq};
                        manager.queue_tx(full_tx).await;
                    } else {
                        panic!("unexpected message");
                    }
                },
                Some(_tx) = tasks.next() => {
                    // TODO: to be used when using execute_tx_async            
                    // // Transation finished executing; run manager clean up
                    // manager.clean_up(tx).await;
                    // num_tx += 1;
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
        println!("Execution worker finished");
    }
}


