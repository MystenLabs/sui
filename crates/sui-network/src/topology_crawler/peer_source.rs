use async_trait::async_trait;

use std::collections::HashMap;

use crate::topology_crawler::model::{NodeMeta, TopologySnapshot};

#[derive(Clone, Debug)]
pub struct PeerTarget {
    pub peer_id: Option<String>,
    pub address: String,
}

#[derive(Clone, Debug)]
pub struct PeerHandle {
    pub peer_id: String,
}

#[derive(Clone, Debug)]
pub struct PeerResponse {
    pub own_info: NodeMeta,
    pub known_peers: Vec<NodeMeta>,
}

#[derive(Debug)]
pub struct PeerSourceError {
    pub message: String,
}

#[async_trait]
pub trait PeerSource: Send + Sync {
    async fn connect(&self, target: &PeerTarget) -> Result<PeerHandle, PeerSourceError>;
    async fn get_known_peers(&self, handle: &PeerHandle) -> Result<PeerResponse, PeerSourceError>;
}

#[derive(Default)]
pub struct MockPeerSource {
    nodes: HashMap<String, NodeMeta>,
    adjacency: HashMap<String, Vec<String>>,
}

impl MockPeerSource {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            adjacency: HashMap::new(),
        }
    }

    pub fn from_snapshot(snapshot: TopologySnapshot) -> Self {
        let mut nodes = HashMap::new();
        for node in snapshot.nodes {
            nodes.insert(node.peer_id.clone(), node);
        }

        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        for edge in snapshot.edges {
            adjacency
                .entry(edge.from)
                .or_default()
                .push(edge.to);
        }

        Self { nodes, adjacency }
    }
}

#[async_trait]
impl PeerSource for MockPeerSource {
    async fn connect(&self, target: &PeerTarget) -> Result<PeerHandle, PeerSourceError> {
        let peer_id = target
            .peer_id
            .clone()
            .ok_or_else(|| PeerSourceError {
                message: "mock connect requires peer_id".to_string(),
            })?;

        if !self.nodes.contains_key(&peer_id) {
            return Err(PeerSourceError {
                message: format!("unknown peer_id: {peer_id}"),
            });
        }

        Ok(PeerHandle { peer_id })
    }

    async fn get_known_peers(&self, handle: &PeerHandle) -> Result<PeerResponse, PeerSourceError> {
        let own_info = self
            .nodes
            .get(&handle.peer_id)
            .cloned()
            .ok_or_else(|| PeerSourceError {
                message: format!("unknown peer_id: {}", handle.peer_id),
            })?;

        let mut known_peers = Vec::new();
        if let Some(neighbors) = self.adjacency.get(&handle.peer_id) {
            for peer_id in neighbors {
                if let Some(info) = self.nodes.get(peer_id) {
                    known_peers.push(info.clone());
                }
            }
        }

        Ok(PeerResponse {
            own_info,
            known_peers,
        })
    }
}

#[derive(Default)]
pub struct AnemoPeerSource {}

impl AnemoPeerSource {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl PeerSource for AnemoPeerSource {
    async fn connect(&self, _target: &PeerTarget) -> Result<PeerHandle, PeerSourceError> {
        todo!("anemo connect")
    }

    async fn get_known_peers(&self, _handle: &PeerHandle) -> Result<PeerResponse, PeerSourceError> {
        todo!("anemo get_known_peers")
    }
}
