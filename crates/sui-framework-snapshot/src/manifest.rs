use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fs, path::PathBuf};
use sui_types::base_types::ObjectID;

pub type SnapshotManifest = BTreeMap<u64, SingleSnapshot>;

#[derive(Serialize, Deserialize)]
pub struct SingleSnapshot {
    /// Git revision that this snapshot is taken on.
    pub(crate) git_revision: String,
    /// List of file names (also identical to object ID) of the bytecode package files.
    pub(crate) package_ids: Vec<ObjectID>,
}

impl SingleSnapshot {
    pub fn git_revision(&self) -> &str {
        &self.git_revision
    }
    pub fn package_ids(&self) -> &[ObjectID] {
        &self.package_ids
    }
}

pub fn load_bytecode_snapshot_manifest() -> SnapshotManifest {
    let Ok(bytes) = fs::read(manifest_path()) else {
        return SnapshotManifest::default();
    };
    serde_json::from_slice::<SnapshotManifest>(&bytes)
        .expect("Could not deserialize SnapshotManifest")
}

pub fn update_bytecode_snapshot_manifest(git_revision: &str, version: u64, files: Vec<ObjectID>) {
    let mut snapshot = load_bytecode_snapshot_manifest();

    snapshot.insert(
        version,
        SingleSnapshot {
            git_revision: git_revision.to_string(),
            package_ids: files,
        },
    );

    let json =
        serde_json::to_string_pretty(&snapshot).expect("Could not serialize SnapshotManifest");
    fs::write(manifest_path(), json).expect("Could not update manifest file");
}

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest.json")
}
