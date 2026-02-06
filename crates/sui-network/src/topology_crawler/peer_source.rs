use async_trait::async_trait;

use crate::topology_crawler::model::NodeMeta;

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
pub struct MockPeerSource {}

impl MockPeerSource {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl PeerSource for MockPeerSource {
    async fn connect(&self, _target: &PeerTarget) -> Result<PeerHandle, PeerSourceError> {
        todo!("mock connect")
    }

    async fn get_known_peers(&self, _handle: &PeerHandle) -> Result<PeerResponse, PeerSourceError> {
        todo!("mock get_known_peers")
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
