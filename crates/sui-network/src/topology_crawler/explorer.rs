use std::time::Duration;

use crate::topology_crawler::model::TopologySnapshot;
use crate::topology_crawler::peer_source::{PeerSource, PeerSourceError, PeerTarget};

#[derive(Debug)]
pub struct ExplorerError {
    pub message: String,
}

pub struct Explorer {
    sources: Vec<Box<dyn PeerSource>>,
    max_peers: usize,
    max_duration: Duration,
}

impl Explorer {
    pub fn new(sources: Vec<Box<dyn PeerSource>>) -> Self {
        Self {
            sources,
            max_peers: 10_000,
            max_duration: Duration::from_secs(60),
        }
    }

    pub fn with_limits(mut self, max_peers: usize, max_duration: Duration) -> Self {
        self.max_peers = max_peers;
        self.max_duration = max_duration;
        self
    }

    pub async fn explore(&self, _seeds: Vec<PeerTarget>) -> Result<TopologySnapshot, ExplorerError> {
        let _ = &self.sources;
        let _ = self.max_peers;
        let _ = self.max_duration;
        todo!("explore topology")
    }
}

impl From<PeerSourceError> for ExplorerError {
    fn from(error: PeerSourceError) -> Self {
        Self {
            message: error.message,
        }
    }
}
