use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    net::SocketAddr,
};
use std::{fmt::Debug, path::Path};
use std::{fs, net::IpAddr};
use sui_protocol_config::ProtocolVersion;
use sui_types::transaction::{Transaction, CertifiedTransaction};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    digests::TransactionDigest,
    effects::TransactionEffects,
    epoch_data::EpochData,
    object::Object,
    storage::{DeleteKind, WriteKind},
    sui_system_state::epoch_start_sui_system_state::EpochStartSystemState,
    transaction::{InputObjectKind, TransactionDataAPI, TransactionKind},
};

pub type UniqueId = u16;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ServerConfig {
    pub kind: String,
    pub ip_addr: IpAddr,
    pub port: u16,
    pub metrics_address: SocketAddr,
    pub attrs: HashMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GlobalConfig(pub HashMap<UniqueId, ServerConfig>);

impl GlobalConfig {
    pub const BENCHMARK_BASE_PORT: u16 = 1500;
    pub const DEFAULT_CONFIG_NAME: &'static str = "configs.json";

    pub fn get(&self, id: &UniqueId) -> Option<&ServerConfig> {
        self.0.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&UniqueId, &ServerConfig)> {
        self.0.iter()
    }

    /// Create a new global config for benchmarking. 
    /// 1 txn generator, 1 primary worker, and variable pre-executor
    pub fn new_for_benchmark(ips: Vec<IpAddr>, pre_exec_workers: usize) -> Self {
        assert!(ips.len() - 2 >= pre_exec_workers && pre_exec_workers > 0);
        let benchmark_port_offset = ips.len() as u16;
        let mut global_config = HashMap::new();
        for (i, ip) in ips.into_iter().enumerate() {
            let network_port = Self::BENCHMARK_BASE_PORT + i as u16;
            let metrics_port = benchmark_port_offset + network_port;
            let kind = match i {
                0 => "GEN",
                //1 => "PRI", // FIXME
                _ => "PRE",
            }.to_string();
            let metrics_address = SocketAddr::new(ip, metrics_port);
            let legacy_metrics = SocketAddr::new(ip, benchmark_port_offset + metrics_port);
            let config = ServerConfig {
                kind,
                ip_addr: ip,
                port: network_port,
                metrics_address,
                attrs: [
                    ("metrics-address".to_string(), legacy_metrics.to_string()),
                    ("execute".to_string(), 100.to_string()),
                    ("mode".to_string(), "channel".to_string()),
                    // ("duration".to_string(), 60.to_string()),
                ]
                .iter()
                .cloned()
                .collect(),
            };
            let id = i as UniqueId;
            global_config.insert(id, config);
        }
        Self(global_config)
    }

    /// Load a global config from a file.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        let config_json = fs::read_to_string(path).expect("Failed to read config file");
        serde_json::from_str(&config_json).expect("Failed to parse config file")
    }

    pub fn export<P: AsRef<Path>>(&self, path: P) {
        let config = serde_json::to_string_pretty(&self).expect("Failed to serialize config file");
        fs::write(path, config).expect("Failed to write config file");
    }

    /// Return the metrics address of all execution workers.
    pub fn execution_workers_metric_addresses(&self) -> Vec<SocketAddr> {
        self.0
            .iter()
            .filter(|(_, config)| config.kind == "EW")
            .map(|(_, config)| config.metrics_address)
            .collect()
    }
}

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

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkMessage {
    pub src: UniqueId,
    pub dst: Vec<UniqueId>,
    pub payload: RemoraMessage,
}

// TODO: Maybe serialize directly to bytes, rather than String and then to bytes
// impl<M: Debug + Serialize + DeserializeOwned> NetworkMessage<M> {
//     pub fn serialize(&self) -> String {
//         format!("{}${}${}$\n", self.src, self.dst, self.payload.serialize())
//     }

//     pub fn deserialize(string: String) -> Self {
//         let mut splitted = string.split("$");
//         let src = splitted.next().unwrap().parse().expect(string.as_str());
//         let dst = splitted.next().unwrap().parse().unwrap();
//         let payload = Message::deserialize(splitted.next().unwrap().to_string());
//         NetworkMessage { src, dst, payload }
//     }
// }

#[derive(Serialize, Deserialize, Debug)]
pub enum RemoraMessage {
    // Sequencing Worker <-> Execution Worker
    EpochStart {
        version: ProtocolVersion,
        data: EpochData,
        ref_gas_price: u64,
    },
    EpochEnd {
        new_epoch_start_state: EpochStartSystemState,
    },
    ProposeExec(TransactionWithEffects),
    // Execution Worker <-> Execution Worker
    //LockedExec { tx: TransactionDigest, objects: Vec<(ObjectRef, Object)> },
    LockedExec {
        // txid: TransactionDigest,
        full_tx: TransactionWithEffects,
        objects: Vec<Option<(ObjectRef, Object)>>,
        child_objects: Vec<Option<(ObjectRef, Object)>>,
    },
    MissingObjects {
        txid: TransactionDigest,
        ew: u8,
        missing_objects: HashSet<ObjectID>,
    },
    TxResults {
        txid: TransactionDigest,
        deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
        written: BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
    },

    // Execution Worker <-> Storage Engine
    StateUpdate(TransactionEffects),
    Checkpointed(u64),

    // For connection setup
    Handshake(),
}

impl Message for RemoraMessage {
    fn serialize(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    fn deserialize(string: String) -> Self {
        serde_json::from_str(&string).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionWithEffects {
    pub tx: Transaction,
    pub ground_truth_effects: Option<TransactionEffects>, // full effects of tx, as ground truth exec result
    pub child_inputs: Option<Vec<ObjectID>>,              // TODO: mark mutable
    pub checkpoint_seq: Option<u64>,
    pub timestamp: f64,
}

impl TransactionWithEffects {
    pub fn is_epoch_change(&self) -> bool {
        match self.tx.transaction_data().kind() {
            TransactionKind::ChangeEpoch(_) => true,
            _ => false,
        }
    }

    /// Returns the read set of a transction.
    /// Specifically, this is the set of input objects to the transaction.
    /// It excludes child objects that are determined at runtime,
    /// but includes all owned objects inputs that must have their version numbers bumped.
    pub fn get_read_set(&self) -> HashSet<ObjectID> {
        let tx_data = self.tx.transaction_data();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("Cannot get input object kinds");

        let mut read_set = HashSet::new();
        for kind in &input_object_kinds {
            match kind {
                // InputObjectKind::MovePackage(id) |
                InputObjectKind::SharedMoveObject { id, .. }
                | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => read_set.insert(*id),
                _ => false,
            };
        }

        for (gas_obj_id, _, _) in tx_data.gas().iter() {
            // skip genesis gas objects
            if *gas_obj_id != ObjectID::from_single_byte(0) {
                read_set.insert(*gas_obj_id);
            }
        }

        // for (&package_obj_id, _, _) in tx_data.move_calls() {
        //     read_set.insert(package_obj_id);
        // }

        return read_set;
    }

    /// TODO: This makes use of ground_truth_effects, which is illegal for validators;
    /// it is not something that is known a-priori before execution.
    /// Returns the write set of a transction.
    pub fn get_write_set(&self) -> HashSet<ObjectID> {
        match &self.ground_truth_effects {
            Some(fx) => {
                let TransactionEffects::V1(tx_effects) = fx else { todo!() };
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

                write_set
            }
            None => self.get_read_set(),
        }
        // assert!(self.ground_truth_effects.is_some());
    }

    /// Returns the read-write set of the transaction.
    pub fn get_read_write_set(&self) -> HashSet<ObjectID> {
        self.get_read_set()
            .union(&self.get_write_set())
            .copied()
            .collect()
    }

    pub fn get_relevant_ews(&self, num_ews: u8) -> HashSet<u8> {
        let rw_set = self.get_read_write_set();
        if rw_set.contains(&ObjectID::from_single_byte(5)) || self.is_epoch_change() {
            (0..num_ews).collect()
        } else {
            rw_set
                .into_iter()
                .map(|obj_id| obj_id[0] % num_ews)
                .collect()
        }
    }
}

pub struct TransactionWithResults {
    pub full_tx: TransactionWithEffects,
    pub tx_effects: TransactionEffects, // determined after execution
    pub deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    pub written: BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
    pub missing_objs: HashSet<ObjectID>,
}

#[derive(PartialEq)]
pub enum ExecutionMode {
    Channel,
    Database,
}

pub fn get_designated_executor_for_tx(
    txid: TransactionDigest,
    tx: &TransactionWithEffects,
    ew_ids: &Vec<UniqueId>,
) -> UniqueId {
    if tx.is_epoch_change() || tx.get_read_set().contains(&ObjectID::from_single_byte(5)) {
        ew_ids[0]
    } else {
        ew_ids[(txid.inner()[0] % ew_ids.len() as u8) as usize]
    }
}

pub fn get_ew_owner_for_object(_obj_id: ObjectID, ew_ids: &Vec<UniqueId>) -> UniqueId {
    // ew_ids[0]
    ew_ids[(_obj_id[0] % ew_ids.len() as u8) as usize]
}

pub fn get_ews_for_tx(tx: &TransactionWithEffects, ew_ids: &Vec<UniqueId>) -> HashSet<UniqueId> {
    let rw_set = tx.get_read_write_set();

    rw_set
        .into_iter()
        .map(|obj_id| get_ew_owner_for_object(obj_id, ew_ids))
        .collect()
}

pub fn get_ews_for_tx_results(
    tx_results: &TransactionWithResults,
    ew_ids: &Vec<UniqueId>,
) -> HashSet<UniqueId> {
    // get deleted and written objects
    let rw_set: Vec<_> = tx_results
        .deleted
        .keys()
        .chain(tx_results.written.keys())
        .cloned()
        .collect();

    rw_set
        .into_iter()
        .map(|obj_id| get_ew_owner_for_object(obj_id, ew_ids))
        .collect()
}

pub fn get_ews_for_deleted_written(
    deleted: &BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    written: &BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
    ew_ids: &Vec<UniqueId>,
) -> HashSet<UniqueId> {
    let rw_set: Vec<_> = deleted.keys().chain(written.keys()).cloned().collect();

    rw_set
        .into_iter()
        .map(|obj_id| get_ew_owner_for_object(obj_id, ew_ids))
        .collect()
}

pub trait WritableObjectStore {
    fn insert(&self, k: ObjectID, v: (ObjectRef, Object)) -> Option<(ObjectRef, Object)>;

    fn remove(&self, k: ObjectID) -> Option<(ObjectRef, Object)>;
}
