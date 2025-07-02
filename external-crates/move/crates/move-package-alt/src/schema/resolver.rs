use serde::{Deserialize, Serialize};

use super::{EnvironmentID, LocalDepInfo, ManifestGitDependency, OnChainDepInfo};

pub const EXTERNAL_RESOLVE_ARG: &str = "--resolve-deps";
pub const EXTERNAL_RESOLVE_METHOD: &str = "resolve";

/// The name of an external resolver
pub type ResolverName = String;

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ResolverDependencyInfo {
    Local(LocalDepInfo),
    Git(ManifestGitDependency),
    OnChain(OnChainDepInfo),
}

/// Requests from the package mananger to the external resolver
#[derive(Serialize, Debug)]
pub struct ResolveRequest {
    pub env: EnvironmentID,
    pub data: toml::Value,
}

/// Responses from the external resolver back to the package manager
#[derive(Deserialize)]
pub struct ResolveResponse(pub ResolverDependencyInfo);
