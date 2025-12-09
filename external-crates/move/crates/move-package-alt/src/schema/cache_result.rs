use serde::Serialize;

use crate::schema::{EnvironmentID, PackageName, PublishAddresses};

/// The output for the `cache-package` command
#[derive(Serialize)]
#[serde(rename = "kebab-case")]
pub struct CachedPackageInfo {
    pub name: PackageName,

    #[serde(flatten)]
    pub addresses: Option<PublishAddresses>,
    pub chain_id: EnvironmentID,
}
