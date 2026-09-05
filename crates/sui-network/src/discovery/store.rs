// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::Path,
};

use anemo::{PeerId, types::PeerInfo};
use tracing::warn;

use super::{SignedVersionedNodeInfo, State, VerifiedSignedVersionedNodeInfo};

/// Trusted peers from `known_peers_v2`, sorted by peer id, together with the
/// snapshot's fingerprint.
pub struct TrustedPeersSnapshot<'a> {
    fingerprint: u64,
    entries: Vec<(PeerId, &'a VerifiedSignedVersionedNodeInfo)>,
}

impl<'a> TrustedPeersSnapshot<'a> {
    pub fn new(
        state: &'a State,
        configured_peers: &HashMap<PeerId, PeerInfo>,
        chain_peers: &HashSet<PeerId>,
    ) -> Self {
        let mut entries: Vec<_> = state
            .known_peers_v2
            .iter()
            .filter(|(peer_id, _)| {
                configured_peers.contains_key(peer_id) || chain_peers.contains(peer_id)
            })
            .map(|(peer_id, verified)| (*peer_id, verified))
            .collect();
        entries.sort_unstable_by_key(|(peer_id, _)| *peer_id);

        let mut hasher = DefaultHasher::new();
        for (peer_id, verified) in &entries {
            peer_id.hash(&mut hasher);
            verified.timestamp_ms().hash(&mut hasher);
        }
        Self {
            fingerprint: hasher.finish(),
            entries,
        }
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    pub fn cloned_records(&self) -> Vec<SignedVersionedNodeInfo> {
        self.entries
            .iter()
            .map(|(_, verified)| verified.inner().clone())
            .collect()
    }
}

pub fn load_stored_peers(path: &Path) -> Vec<SignedVersionedNodeInfo> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let reader = std::io::BufReader::new(file);
    match serde_yaml::from_reader(reader) {
        Ok(peers) => peers,
        Err(e) => {
            warn!(
                "Failed to parse stored peer cache at {}: {e}",
                path.display()
            );
            Vec::new()
        }
    }
}

pub fn save_stored_peers(path: &Path, peers: &[SignedVersionedNodeInfo]) {
    let tmp_path = path.with_extension("yaml.tmp");
    let write_result = (|| -> std::io::Result<()> {
        let file = std::fs::File::create(&tmp_path)?;
        let writer = std::io::BufWriter::new(file);
        serde_yaml::to_writer(writer, peers).map_err(std::io::Error::other)?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    })();
    if let Err(e) = write_result {
        warn!(
            "Failed to save stored peer cache to {}: {e}",
            path.display()
        );
    }
}
