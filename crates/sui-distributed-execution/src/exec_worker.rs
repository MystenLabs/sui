use core::panic;
use std::collections::HashSet;
use std::sync::Arc;

use sui_adapter::adapter::MoveVM;
use sui_adapter::{execution_engine, execution_mode};
use sui_config::genesis::Genesis;
use sui_core::transaction_input_checker::get_gas_status_no_epoch_store_experimental;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::ExecutionDigests;
use sui_types::epoch_data::EpochData;
use sui_types::message_envelope::Message;
use sui_types::messages::{InputObjectKind, InputObjects, TransactionDataAPI, VerifiedTransaction, TransactionKind};
use sui_types::metrics::LimitsMetrics;
use sui_types::temporary_store::TemporaryStore;
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_adapter::adapter;
use sui_move_natives;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::types::*;

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
    ) {
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
    }

    pub async fn run(&mut self,
        metrics: Arc<LimitsMetrics>,
        exec_watermark: u64,
        mut epoch_start_receiver: mpsc::Receiver<SailfishMessage>,
        mut tx_receiver: mpsc::Receiver<SailfishMessage>,
        epoch_end_sender: mpsc::Sender<SailfishMessage>,
    ){
        let mut epoch_data: EpochData;
        let mut protocol_config: ProtocolConfig;
        let mut reference_gas_price: u64;
        // Wait for epoch start message
        let SailfishMessage::EpochStart{
            conf: protocol_config_,
            data: epoch_data_,
            ref_gas_price: reference_gas_price_,
        } = epoch_start_receiver.recv().await.unwrap() 
        else {
            panic!("unexpected message");
        };
        println!("Got epoch start message");

        protocol_config = protocol_config_;
        epoch_data = epoch_data_;
        reference_gas_price = reference_gas_price_;

        let native_functions = sui_move_natives::all_natives(/* silent */ true);
        let mut move_vm = Arc::new(
            adapter::new_move_vm(native_functions.clone(), &protocol_config, false)
                .expect("We defined natives to not fail here"),
        );

        // start timer for TPS computation
        let now = Instant::now();
        let mut num_tx: usize = 0;
        // receive txs
        while let Some(SailfishMessage::Transaction{
            tx,
            digest: tx_digest,
            checkpoint_seq,
        }) = tx_receiver.recv().await
        {
            self
                .execute_tx(
                    &tx,
                    &tx_digest,
                    checkpoint_seq,
                    &protocol_config,
                    &move_vm,
                    &epoch_data,
                    reference_gas_price,
                    metrics.clone(),
                )
                .await;

            num_tx += 1;
            if checkpoint_seq % 10000 == 0 {
                println!("Executed {}", checkpoint_seq);
            }

            if let TransactionKind::ChangeEpoch(_) = tx.data().transaction_data().kind() {
                // First send end of epoch message to sequence worker
                println!("END OF EPOCH at checkpoint {}", checkpoint_seq);
                let latest_state = get_sui_system_state(&&self.memory_store)
                    .expect("Read Sui System State object cannot fail");
                let new_epoch_start_state = latest_state.into_epoch_start_state();
                epoch_end_sender
                    .send(SailfishMessage::EpochEnd{
                        new_epoch_start_state,
                    })
                    .await
                    .expect("Sending doesn't work");

                // Then wait for start epoch message from sequence worker and update local state
                let SailfishMessage::EpochStart{
                    conf: protocol_config_,
                    data: epoch_data_,
                    ref_gas_price: reference_gas_price_,
                } = epoch_start_receiver.recv().await.unwrap()
                else {
                    panic!("unexpected message");
                };
                move_vm = Arc::new(
                    adapter::new_move_vm(native_functions.clone(), &protocol_config, false)
                        .expect("We defined natives to not fail here"),
                );
                protocol_config = protocol_config_;
                epoch_data = epoch_data_;
                reference_gas_price = reference_gas_price_;
            }

            // Stop executing when I hit the watermark
            if checkpoint_seq == exec_watermark-1 {
                break;
            }
        }

        // print TPS
        let elapsed = now.elapsed();
        println!(
            "Execution worker TPS: {}",
            1000.0 * num_tx as f64 / elapsed.as_millis() as f64
        );
        println!("Execution worker finished");
    }
}


