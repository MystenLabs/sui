// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::process::ExitStatus;

use thiserror::Error;

use crate::{
    dependency::external::ResolverName,
    package::{EnvironmentName, PackageName},
};

#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("I/O Error when running external resolver {resolver}")]
    IoError {
        resolver: ResolverName,

        #[source]
        source: std::io::Error,
    },

    /// This indicates that the resolver was faulty
    #[error("{resolver} did not follow the external resolver protocol: {message}")]
    BadResolver {
        resolver: ResolverName,
        message: String,
    },

    /// This indicates that the resolver returned a non-successful exit code
    #[error("{resolver} returned error code: {code}")]
    ResolverUnsuccessful {
        resolver: ResolverName,
        code: ExitStatus,
    },

    /// This indicates that the resolver executed successfully but returned an error
    #[error("{resolver} couldn't resolve {dep} in {env_str}: {message}")]
    ResolverFailed {
        resolver: ResolverName,
        dep: PackageName,
        env_str: String,
        message: String,
    },
}

impl ResolverError {
    pub fn io_error(resolver: &ResolverName, source: std::io::Error) -> Self {
        Self::IoError {
            resolver: resolver.clone(),
            source,
        }
    }

    pub fn bad_resolver(resolver: &ResolverName, message: impl AsRef<str>) -> Self {
        Self::BadResolver {
            resolver: resolver.clone(),
            message: message.as_ref().to_string(),
        }
    }

    pub fn nonzero_exit(resolver: &ResolverName, code: ExitStatus) -> Self {
        Self::ResolverUnsuccessful {
            resolver: resolver.clone(),
            code,
        }
    }

    pub fn resolver_failed(
        resolver: ResolverName,
        dep: PackageName,
        env: Option<EnvironmentName>,
        message: String,
    ) -> Self {
        Self::ResolverFailed {
            resolver,
            dep,
            message,
            env_str: match env {
                Some(env_name) => format!("environment {env_name}"),
                None => "default environment".to_string(),
            },
        }
    }
}
