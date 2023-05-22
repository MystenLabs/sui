// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter},
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
    Identifier, transaction::TransactionDataAPI,
};

pub type RawTopic = String;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum ObjectChangeStatus {
    Created(ObjectRef, Owner),
    Mutated(ObjectRef, Owner),
    Deleted(ObjectRef),
    Wrapped(ObjectRef),
    Unwrapped(ObjectRef, Owner),
    UnwrappedThenDeleted(ObjectRef),
    LoadedChildObject(ObjectID, SequenceNumber),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStreamData {
    pub node_timestamp_ms: u64,
    pub epoch_id: u64,
    pub checkpoint_id: u64,
    pub sender: SuiAddress,
    pub tx_digest: TransactionDigest,
    pub data: NodeStreamInnerData,
}

impl NodeStreamData {
    pub fn decompose(self) -> (RawTopic, Vec<u8>) {
        let topic = self.data.topic().for_epoch(self.epoch_id);
        let bytes = bcs::to_bytes(&self).unwrap();
        (topic, bytes)
    }

    pub fn from_post_exec(
        node_timestamp_ms: u64,
        epoch_id: u64,
        checkpoint_id: u64,
        sender: &SuiAddress,
        tx_digest: &TransactionDigest,
        cert: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        loaded_child_objects: &BTreeMap<ObjectID, SequenceNumber>,
        store: &dyn ObjectStore,
    ) -> Vec<Self> {
        NodeStreamInnerData::from_post_exec(cert, effects.clone(), loaded_child_objects, store)
            .iter()
            .map(|data| Self {
                node_timestamp_ms,
                epoch_id,
                checkpoint_id,
                sender: *sender,
                tx_digest: *tx_digest,
                data: data.clone(),
            })
            .collect()
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeStreamInnerData {
    String(String),
    PackagePublish(Object),
    ObjectChangeLight(ObjectChangeStatus),
    ObjectChangeRaw(ObjectChangeStatus, Option<Object>),
    MoveCall(ObjectID, Identifier, Identifier),
    TransactionDigest,
    GasCostSummary(GasCostSummary),
    // TODO:
    // CoinBalanceChange
    // Epoch
    // Checkpoint
}

impl NodeStreamInnerData {
    pub fn topic(&self) -> TxInfoNodeStreamTopic {
        match self {
            NodeStreamInnerData::PackagePublish(_) => TxInfoNodeStreamTopic::PackagePublish,
            NodeStreamInnerData::ObjectChangeLight(_) => TxInfoNodeStreamTopic::ObjectChangeLight,
            NodeStreamInnerData::TransactionDigest => TxInfoNodeStreamTopic::TransactionDigest,
            NodeStreamInnerData::String(_) => TxInfoNodeStreamTopic::String,
            NodeStreamInnerData::ObjectChangeRaw(_, _) => TxInfoNodeStreamTopic::ObjectChangeRaw,
            NodeStreamInnerData::MoveCall(_, _, _) => TxInfoNodeStreamTopic::MoveCall,
            NodeStreamInnerData::GasCostSummary(_) => TxInfoNodeStreamTopic::GasCostSummary,
        }
    }

    pub fn from_post_exec(
        cert: &VerifiedExecutableTransaction,
        effects: TransactionEffects,
        loaded_child_objects: &BTreeMap<ObjectID, SequenceNumber>,
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
                if let NodeStreamInnerData::ObjectChangeLight(change) = q {
                    Some(match change {
                        ObjectChangeStatus::Created(r, _)
                        | ObjectChangeStatus::Mutated(r, _)
                        | ObjectChangeStatus::Deleted(r)
                        | ObjectChangeStatus::Wrapped(r)
                        | ObjectChangeStatus::Unwrapped(r, _)
                        | ObjectChangeStatus::UnwrappedThenDeleted(r) => {
                            NodeStreamInnerData::ObjectChangeRaw(*change, {
                                let obj = store
                                    .get_object_by_key(&r.0, r.1)
                                    .expect("DB read should not fail");
                                if let Some(o) = obj.clone() {
                                    if o.is_package() {
                                        packages.push(NodeStreamInnerData::PackagePublish(o));
                                    }
                                }
                                obj
                            })
                        }
                        ObjectChangeStatus::LoadedChildObject(id, seq) => {
                            NodeStreamInnerData::ObjectChangeRaw(
                                *change,
                                store
                                    .get_object_by_key(id, *seq)
                                    .expect("DB read should not fail"),
                            )
                        }
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
                NodeStreamInnerData::MoveCall(
                    **package,
                    module.to_owned().to_owned(),
                    function.to_owned().to_owned(),
                )
            },
        ));

        // Record this TX
        result.push(Self::TransactionDigest);
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TxInfoNodeStreamTopic {
    String,
    PackagePublish,
    ObjectChangeLight,
    ObjectChangeRaw,
    MoveCall,
    TransactionDigest,
    GasCostSummary,
    // TODO:
    // CoinBalanceChange
    // Epoch
    // Checkpoint
}

impl TxInfoNodeStreamTopic {
    pub fn for_epoch(&self, epoch: u64) -> RawTopic {
        format!("{}-{}", epoch, self)
    }
}

impl Display for TxInfoNodeStreamTopic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TxInfoNodeStreamTopic::String => write!(f, "string"),
            TxInfoNodeStreamTopic::PackagePublish => write!(f, "package_publish"),
            TxInfoNodeStreamTopic::ObjectChangeLight => write!(f, "object_change_light"),
            TxInfoNodeStreamTopic::ObjectChangeRaw => write!(f, "object_change_raw"),
            TxInfoNodeStreamTopic::MoveCall => write!(f, "move_call"),
            TxInfoNodeStreamTopic::TransactionDigest => write!(f, "transaction_digest"),
            TxInfoNodeStreamTopic::GasCostSummary => write!(f, "gas_cost_summary"),
        }
    }
}
