use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopologySnapshot {
    pub generated_at_ms: u64,
    pub nodes: Vec<NodeMeta>,
    pub edges: Vec<Edge>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeMeta {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub access_type: String,
    pub timestamp_ms: u64,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}
