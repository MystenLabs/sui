use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::net::IpAddr;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    digests::TransactionDigest,
    effects::TransactionEffects,
    epoch_data::EpochData,
    epoch_data::EpochData,
    messages::{InputObjectKind, TransactionDataAPI, TransactionKind, VerifiedTransaction},
    object::Object,
    sui_system_state::epoch_start_sui_system_state::EpochStartSystemState,
    sui_system_state::epoch_start_sui_system_state::EpochStartSystemState,
    transaction::{InputObjectKind, TransactionDataAPI, TransactionKind, VerifiedTransaction},
};

pub type UniqueId = u16;

#[derive(Clone, Deserialize, Debug)]
pub struct ServerConfig {
    pub kind: String,
    pub ip_addr: IpAddr,
    pub port: u16,
    pub attrs: HashMap<String, String>,
}

pub type GlobalConfig = HashMap<UniqueId, ServerConfig>;

pub trait Message {
    fn serialize(&self) -> String;
    fn deserialize(string: String) -> Self;
}

impl Message for std::string::String {
    fn serialize(&self) -> String {
        self.to_string()
    }

    fn deserialize(string: String) -> Self {
        string
    }
}

#[derive(Debug)]
pub struct NetworkMessage<M: Debug + Message> {
    pub src: UniqueId,
    pub dst: UniqueId,
    pub payload: M,
}

// TODO: Maybe serialize directly to bytes, rather than String and then to bytes
impl<M: Debug + Message> NetworkMessage<M> {
    pub fn serialize(&self) -> String {
        format!(
            "{}\t{}\t{}\t\n",
            self.src,
            self.dst,
            self.payload.serialize()
        )
    }

    pub fn deserialize(string: String) -> Self {
        let mut splitted = string.split("\t");
        let src = splitted.next().unwrap().parse().unwrap();
        let dst = splitted.next().unwrap().parse().unwrap();
        let payload = Message::deserialize(splitted.next().unwrap().to_string());
        NetworkMessage { src, dst, payload }
    }
}

#[derive(Debug)]
pub enum SailfishMessage {
    // Sequencing Worker <-> Execution Worker
    EpochStart {
        conf: ProtocolConfig,
        data: EpochData,
        ref_gas_price: u64,
    },
    EpochEnd {
        new_epoch_start_state: EpochStartSystemState,
    },
    ProposeExec(Transaction),

    // Execution Worker <-> Execution Worker
    LockedExec {
        tx: TransactionDigest,
        objects: Vec<(ObjectRef, Object)>,
    },

    // Execution Worker <-> Storage Engine
    StateUpdate(TransactionEffects),
    Checkpointed(u64),
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub tx: VerifiedTransaction,
    pub ground_truth_effects: TransactionEffects, // full effects of tx, as ground truth exec result
    pub checkpoint_seq: u64,
}

impl Transaction {
    pub fn is_epoch_change(&self) -> bool {
        match self.tx.data().transaction_data().kind() {
            TransactionKind::ChangeEpoch(_) => true,
            _ => false,
        }
    }

    /// Returns the read set of a transction.
    /// Specifically, this is the set of input objects to the transaction. It excludes
    /// child objects that are determined at runtime, but includes all owned objects inputs
    /// that must have their version numbers bumped.
    pub fn get_read_set(&self) -> HashSet<ObjectID> {
        let tx_data = self.tx.data().transaction_data();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("Cannot get input object kinds");

        let mut read_set = HashSet::new();
        for kind in &input_object_kinds {
            match kind {
                InputObjectKind::MovePackage(id)
                | InputObjectKind::SharedMoveObject { id, .. }
                | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => read_set.insert(*id),
            };
        }
        return read_set;
    }

    /// TODO: This makes use of ground_truth_effects, which is illegal; it is not something that is
    /// known a-priori before execution
    /// Returns the write set of a transction
    pub fn get_write_set(&self) -> HashSet<ObjectID> {
        let TransactionEffects::V1(tx_effects) = &self.ground_truth_effects;
        let total_writes = tx_effects.created.len()
            + tx_effects.mutated.len()
            + tx_effects.unwrapped.len()
            + tx_effects.deleted.len()
            + tx_effects.unwrapped_then_deleted.len()
            + tx_effects.wrapped.len();
        let mut write_set: HashSet<ObjectID> = HashSet::with_capacity(total_writes);

        write_set.extend(
            tx_effects
                .created
                .iter()
                .chain(tx_effects.mutated.iter())
                .chain(tx_effects.unwrapped.iter())
                .map(|(object_ref, _)| object_ref.0),
        );
        write_set.extend(
            tx_effects
                .deleted
                .iter()
                .chain(tx_effects.unwrapped_then_deleted.iter())
                .chain(tx_effects.wrapped.iter())
                .map(|object_ref| object_ref.0),
        );

        return write_set;
    }

    /// Returns the read-write set of the transaction
    pub fn get_read_write_set(&self) -> HashSet<ObjectID> {
        self.get_read_set()
            .union(&self.get_write_set())
            .copied()
            .collect()
    }
}

pub struct TransactionWithResults {
    pub full_tx: Transaction,
    pub tx_effects: TransactionEffects, // determined after execution
}
