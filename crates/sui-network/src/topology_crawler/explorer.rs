use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::topology_crawler::model::{Edge, NodeMeta, TopologySnapshot};
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
        if self.sources.is_empty() {
            return Err(ExplorerError {
                message: "no peer sources configured".to_string(),
            });
        }

        let source = &self.sources[0];
        let start = Instant::now();

        let mut nodes: HashMap<String, NodeMeta> = HashMap::new();
        let mut edges: HashSet<(String, String)> = HashSet::new();
        let mut errors: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<PeerTarget> = VecDeque::new();

        for seed in _seeds {
            if let Some(peer_id) = seed.peer_id.clone() {
                if visited.insert(peer_id) {
                    queue.push_back(seed);
                }
            } else {
                errors.push("seed missing peer_id".to_string());
            }
        }

        while let Some(target) = queue.pop_front() {
            if nodes.len() >= self.max_peers {
                break;
            }
            if start.elapsed() > self.max_duration {
                break;
            }

            let handle = match source.connect(&target).await {
                Ok(handle) => handle,
                Err(err) => {
                    errors.push(err.message);
                    continue;
                }
            };

            let response = match source.get_known_peers(&handle).await {
                Ok(response) => response,
                Err(err) => {
                    errors.push(err.message);
                    continue;
                }
            };

            let own_peer_id = response.own_info.peer_id.clone();
            nodes.insert(own_peer_id.clone(), response.own_info.clone());

            for peer in response.known_peers {
                let peer_id = peer.peer_id.clone();
                nodes.insert(peer_id.clone(), peer.clone());
                edges.insert((own_peer_id.clone(), peer_id.clone()));

                if visited.insert(peer_id.clone()) {
                    let address = peer
                        .addresses
                        .first()
                        .cloned()
                        .unwrap_or_default();
                    queue.push_back(PeerTarget {
                        peer_id: Some(peer_id),
                        address,
                    });
                }
            }
        }

        let mut node_list: Vec<NodeMeta> = nodes.into_values().collect();
        node_list.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));

        let mut edge_list: Vec<Edge> = edges
            .into_iter()
            .map(|(from, to)| Edge { from, to })
            .collect();
        edge_list.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));

        Ok(TopologySnapshot {
            generated_at_ms: 0,
            nodes: node_list,
            edges: edge_list,
            errors,
        })
    }
}

impl From<PeerSourceError> for ExplorerError {
    fn from(error: PeerSourceError) -> Self {
        Self {
            message: error.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Explorer;
    use crate::topology_crawler::peer_source::{MockPeerSource, PeerTarget};
    use crate::topology_crawler::model::TopologySnapshot;

    #[tokio::test]
    async fn explorer_matches_tier_snapshot() {
        let snapshot_json = include_str!("testdata/tier_topology_snapshot.json");
        let snapshot: TopologySnapshot = serde_json::from_str(snapshot_json).unwrap();

        let seeds = {
            use std::collections::{HashSet, BTreeSet};

            let mut to_set: HashSet<String> = HashSet::new();
            for edge in &snapshot.edges {
                to_set.insert(edge.to.clone());
            }

            let mut seed_ids: BTreeSet<String> = BTreeSet::new();
            for node in &snapshot.nodes {
                if !to_set.contains(&node.peer_id) {
                    seed_ids.insert(node.peer_id.clone());
                }
            }

            seed_ids
                .into_iter()
                .map(|peer_id| PeerTarget {
                    peer_id: Some(peer_id),
                    address: String::new(),
                })
                .collect::<Vec<_>>()
        };

        let source = MockPeerSource::from_snapshot(snapshot.clone());
        let explorer = Explorer::new(vec![Box::new(source)]);
        let output = explorer.explore(seeds).await.unwrap();

        assert_eq!(output, snapshot);
    }
}
