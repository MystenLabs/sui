use serde::{Deserialize, Serialize};

use super::{EnvironmentID, LocalDepInfo, ManifestGitDependency, OnChainDepInfo};

/// The name of an external resolver
pub type ResolverName = String;

#[derive(Deserialize)]
pub enum ResolverDependencyInfo {
    Local(LocalDepInfo),
    Git(ManifestGitDependency),
    OnChain(OnChainDepInfo),
}

/// Requests from the package mananger to the external resolver
#[derive(Serialize, Debug)]
pub struct ResolveRequest {
    pub env: EnvironmentID,

    #[serde(default)]
    pub data: toml::Value,
}

/// Responses from the external resolver back to the package manager
#[derive(Deserialize)]
pub struct ResolveResponse {
    pub result: ResolverDependencyInfo,
    pub warnings: Vec<String>,
}
