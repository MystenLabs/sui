use std::path::PathBuf;

use serde::Serialize;

use crate::schema::{EnvironmentID, PackageName, PublishAddresses};

/// The output for the `cache-package` command
#[derive(Serialize)]
pub struct CachedPackageInfo {
    pub name: PackageName,
    pub path: PathBuf,

    #[serde(flatten)]
    pub addresses: Option<PublishAddresses>,

    // Serialized as `chain_id` rather than `chain-id`; can't change for backwards compatibility.
    pub chain_id: EnvironmentID,
}
