// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use tracing::warn;

use super::SignedVersionedNodeInfo;

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
