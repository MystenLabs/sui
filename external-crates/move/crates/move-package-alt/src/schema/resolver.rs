use std::fmt::Display;

use anyhow::ensure;
use serde::{Deserialize, Serialize};

use super::{EnvironmentID, LocalDepInfo, ManifestGitDependency, OnChainDepInfo};

pub const EXTERNAL_RESOLVE_ARG: &str = "--resolve-deps";
pub const EXTERNAL_RESOLVE_METHOD: &str = "resolve";

/// The name of an external resolver. Guaranteed not to contain path separator characters
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ResolverName(String);

#[derive(Debug, Deserialize, Clone, PartialEq)]
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

impl TryFrom<String> for ResolverName {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        ensure!(
            !value.contains(std::path::is_separator),
            "invalid character in external resolver name `{value}`"
        );
        Ok(Self(value))
    }
}

impl From<ResolverName> for String {
    fn from(value: ResolverName) -> Self {
        value.0
    }
}

impl Display for ResolverName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
