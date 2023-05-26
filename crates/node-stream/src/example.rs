// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter},
    str::FromStr,
    sync::Arc,
};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    event::Event,
    executable_transaction::VerifiedExecutableTransaction,
    gas::GasCostSummary,
    object::{Object, Owner},
    storage::ObjectStore,
    transaction::{TransactionData, TransactionDataAPI},
    Identifier,
};

use crate::types_ex::{NodeStreamPayload, NodeStreamPerEpochTopic, NodeStreamTopic};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TxInfoNodeStreamTopic {
    String,
    PackagePublish,
    ObjectChangeLight,
    ObjectChangeRaw,
    MoveCall,
    Transaction,
    Effects,
    GasCostSummary,
    ExecLatency,
    // TODO:
    // CoinBalanceChange
    // Epoch
    // Checkpoint
}

impl Display for TxInfoNodeStreamTopic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TxInfoNodeStreamTopic::String => write!(f, "string"),
            TxInfoNodeStreamTopic::PackagePublish => write!(f, "package_publish"),
            TxInfoNodeStreamTopic::ObjectChangeLight => write!(f, "object_change_light"),
            TxInfoNodeStreamTopic::ObjectChangeRaw => write!(f, "object_change_raw"),
            TxInfoNodeStreamTopic::MoveCall => write!(f, "move_call"),
            TxInfoNodeStreamTopic::Transaction => write!(f, "transaction"),
            TxInfoNodeStreamTopic::GasCostSummary => write!(f, "gas_cost_summary"),
            TxInfoNodeStreamTopic::Effects => write!(f, "effects"),
            TxInfoNodeStreamTopic::ExecLatency => write!(f, "exec_latency"),
        }
    }
}

impl FromStr for TxInfoNodeStreamTopic {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "string" => Ok(TxInfoNodeStreamTopic::String),
            "package_publish" => Ok(TxInfoNodeStreamTopic::PackagePublish),
            "object_change_light" => Ok(TxInfoNodeStreamTopic::ObjectChangeLight),
            "object_change_raw" => Ok(TxInfoNodeStreamTopic::ObjectChangeRaw),
            "move_call" => Ok(TxInfoNodeStreamTopic::MoveCall),
            "transaction" => Ok(TxInfoNodeStreamTopic::Transaction),
            "effects" => Ok(TxInfoNodeStreamTopic::Effects),
            "gas_cost_summary" => Ok(TxInfoNodeStreamTopic::GasCostSummary),
            "exec_latency" => Ok(TxInfoNodeStreamTopic::ExecLatency),
            _ => Err(anyhow::anyhow!("Invalid topic")),
        }
    }
}

impl NodeStreamPerEpochTopic<TxInfoData, TxInfoMetadata> for TxInfoNodeStreamTopic {
    type FromBytesError = anyhow::Error;
    type ToBytesError = anyhow::Error;

    fn topic_for_epoch(&self, epoch: u64) -> NodeStreamTopic {
        NodeStreamTopic::new(format!("{}-{}", epoch, self))
    }

    fn payload_from_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<NodeStreamPayload<TxInfoData, TxInfoMetadata>, Self::FromBytesError> {
        bcs::from_bytes(bytes).map_err(|e| e.into())
    }

    fn payload_to_bytes(
        &self,
        payload: &NodeStreamPayload<TxInfoData, TxInfoMetadata>,
    ) -> Result<Vec<u8>, Self::ToBytesError> {
        bcs::to_bytes(payload).map_err(|e| e.into())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ObjectChangeStatus {
    Created(ObjectRef, Owner),
    Mutated(ObjectRef, Owner),
    Deleted(ObjectRef),
    Wrapped(ObjectRef),
    Unwrapped(ObjectRef, Owner),
    UnwrappedThenDeleted(ObjectRef),
    LoadedChildObject(ObjectID, SequenceNumber),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TxInfoData {
    String(String),
    PackagePublish(Object),
    ObjectChangeLight(ObjectChangeStatus),
    ObjectChangeRaw(ObjectChangeStatus, Option<Object>),
    MoveCall(ObjectID, Identifier, Identifier),
    Transaction(TransactionData),
    Effects(TransactionEffects),
    GasCostSummary(GasCostSummary),
    ExecLatency(ExecLatency),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TxInfoMetadata {
    pub message_process_timestamp_ms: u64,
    pub checkpoint_id: u64,
    pub sender: SuiAddress,
    pub tx_digest: TransactionDigest,
}

pub fn from_post_exec(
    message_process_timestamp_ms: u64,
    checkpoint_id: u64,
    sender: &SuiAddress,
    tx_digest: &TransactionDigest,
    cert: &VerifiedExecutableTransaction,
    effects: &TransactionEffects,
    loaded_child_objects: &BTreeMap<ObjectID, SequenceNumber>,
    tx_stats: ExecLatency,
    store: &dyn ObjectStore,
) -> Vec<(
    NodeStreamPayload<TxInfoData, TxInfoMetadata>,
    TxInfoNodeStreamTopic,
)> {
    TxInfoData::from_post_exec(cert, effects.clone(), loaded_child_objects, tx_stats, store)
        .into_iter()
        .map(|data| {
            let topic = data.to_topic();
            (
                NodeStreamPayload {
                    metdata: TxInfoMetadata {
                        message_process_timestamp_ms,
                        checkpoint_id,
                        sender: *sender,
                        tx_digest: *tx_digest,
                    },
                    data,
                },
                topic,
            )
        })
        .collect()
}

impl TxInfoData {
    pub fn from_post_exec(
        cert: &VerifiedExecutableTransaction,
        effects: TransactionEffects,
        loaded_child_objects: &BTreeMap<ObjectID, SequenceNumber>,
        tx_stats: ExecLatency,
        store: &dyn ObjectStore,
    ) -> Vec<Self> {
        let mut result = vec![];
        // Objects
        result.extend(
            effects
                .created()
                .iter()
                .map(|q| Self::ObjectChangeLight(ObjectChangeStatus::Created(q.0, q.1))),
        );
        result.extend(
            effects
                .mutated()
                .iter()
                .map(|q| Self::ObjectChangeLight(ObjectChangeStatus::Mutated(q.0, q.1))),
        );
        result.extend(
            effects
                .deleted()
                .iter()
                .map(|q| Self::ObjectChangeLight(ObjectChangeStatus::Deleted(*q))),
        );
        result.extend(
            effects
                .wrapped()
                .iter()
                .map(|q| Self::ObjectChangeLight(ObjectChangeStatus::Wrapped(*q))),
        );
        result.extend(
            effects
                .unwrapped()
                .iter()
                .map(|q| Self::ObjectChangeLight(ObjectChangeStatus::Unwrapped(q.0, q.1))),
        );
        result.extend(
            effects
                .unwrapped_then_deleted()
                .iter()
                .map(|q| Self::ObjectChangeLight(ObjectChangeStatus::UnwrappedThenDeleted(*q))),
        );
        result.extend(
            loaded_child_objects.iter().map(|q| {
                Self::ObjectChangeLight(ObjectChangeStatus::LoadedChildObject(*q.0, *q.1))
            }),
        );

        // Get the objects
        let mut packages = vec![];
        let mut objects = result
            .iter()
            .filter_map(|q| {
                if let Self::ObjectChangeLight(change) = q {
                    Some(match change {
                        ObjectChangeStatus::Created(r, _)
                        | ObjectChangeStatus::Mutated(r, _)
                        | ObjectChangeStatus::Deleted(r)
                        | ObjectChangeStatus::Wrapped(r)
                        | ObjectChangeStatus::Unwrapped(r, _)
                        | ObjectChangeStatus::UnwrappedThenDeleted(r) => {
                            Self::ObjectChangeRaw(change.clone(), {
                                let obj = store
                                    .get_object_by_key(&r.0, r.1)
                                    .expect("DB read should not fail");
                                if let Some(o) = obj.clone() {
                                    if o.is_package() {
                                        packages.push(Self::PackagePublish(o));
                                    }
                                }
                                obj
                            })
                        }
                        ObjectChangeStatus::LoadedChildObject(id, seq) => Self::ObjectChangeRaw(
                            change.clone(),
                            store
                                .get_object_by_key(id, *seq)
                                .expect("DB read should not fail"),
                        ),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        result.append(&mut packages);
        result.append(&mut objects);

        // Gas
        result.push(Self::GasCostSummary(effects.gas_cost_summary().clone()));

        // Move calls
        result.extend(cert.intent_message().value.move_calls().iter().map(
            |(package, module, function)| {
                Self::MoveCall(
                    **package,
                    module.to_owned().to_owned(),
                    function.to_owned().to_owned(),
                )
            },
        ));

        // Effects
        result.push(Self::Effects(effects));

        // Exec stats
        result.push(Self::ExecLatency(tx_stats));

        // Record this TX
        result.push(Self::Transaction(cert.intent_message().value.clone()));
        result
    }

    pub fn to_topic(&self) -> TxInfoNodeStreamTopic {
        match self {
            Self::String(_) => TxInfoNodeStreamTopic::String,
            Self::PackagePublish(_) => TxInfoNodeStreamTopic::PackagePublish,
            Self::ObjectChangeLight(_) => TxInfoNodeStreamTopic::ObjectChangeLight,
            Self::ObjectChangeRaw(_, _) => TxInfoNodeStreamTopic::ObjectChangeRaw,
            Self::MoveCall(_, _, _) => TxInfoNodeStreamTopic::MoveCall,
            Self::Transaction(_) => TxInfoNodeStreamTopic::Transaction,
            Self::GasCostSummary(_) => TxInfoNodeStreamTopic::GasCostSummary,
            Self::Effects(_) => TxInfoNodeStreamTopic::Effects,
            Self::ExecLatency(_) => TxInfoNodeStreamTopic::ExecLatency,
        }
    }
}

// pub struct AuthorityMetricsSimplified {
//     pub tx_orders: u64,
//     pub total_certs: u64,
//     pub total_cert_attempts: u64,
//     pub total_effects: u64,
//     pub shared_obj_tx: u64,
//     pub sponsored_tx: u64,
//     pub tx_already_processed: u64,
//     pub num_input_objs: u64,
//     pub num_shared_objects: u64,
//     pub batch_size: u64,

//     pub handle_transaction_latency: f64,

//     pub execute_certificate_latency: f64,

//     pub execute_certificate_with_effects_latency: f64,
//     pub internal_execution_latency: f64,
//     pub prepare_certificate_latency: f64,
//     pub commit_certificate_latency: f64,
//     pub db_checkpoint_latency: f64,

//     pub transaction_manager_num_enqueued_certificates: BTreeMap<String, Vec<u64>>,
//     pub transaction_manager_num_missing_objects: u64,
//     pub transaction_manager_num_pending_certificates: u64,
//     pub transaction_manager_num_executing_certificates: u64,
//     pub transaction_manager_num_ready: u64,

//     pub execution_driver_executed_transactions: u64,
//     pub execution_driver_dispatch_queue: u64,

//     pub skipped_consensus_txns: u64,
//     pub skipped_consensus_txns_cache_hit: u64,

//     pub post_processing_total_events_emitted: u64,
//     pub post_processing_total_tx_indexed: u64,
//     pub post_processing_total_tx_had_event_processed: u64,

//     pub pending_notify_read: u64,

//     /// Consensus handler metrics
//     pub consensus_handler_processed_batches: u64,
//     pub consensus_handler_processed_bytes: u64,

//     pub consensus_handler_processed: BTreeMap<String, Vec<u64>>,
//     pub consensus_handler_num_low_scoring_authorities: u64,
//     pub consensus_handler_scores: BTreeMap<String, Vec<u64>>,
//     pub consensus_committed_subdags: BTreeMap<String, Vec<u64>>,
//     pub consensus_committed_certificates: BTreeMap<String, Vec<u64>>,

//     // Verifier
//     pub verifier_runtime_per_module: u64,
//     pub verifier_runtime_per_ptb: u64,
// }

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ExecLatency {
    // pub execute_certificate_latency: u64,

    // pub execute_certificate_with_effects_latency: u64,
    // pub internal_execution_latency: u64,
    pub prepare_certificate_latency: u64,
    pub commit_certificate_latency: u64,
}
