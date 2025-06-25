// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ r.<res> = data }`).

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fmt::Debug,
    ops::Range,
    path::PathBuf,
    process::{ExitStatus, Stdio},
};

use futures::future::try_join_all;
use itertools::{Itertools, izip};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, info};

use crate::{
    dependency::{PinnedDependencyInfo, combine::Combined},
    errors::{FileHandle, TheFile},
    flavor::MoveFlavor,
    jsonrpc::Endpoint,
    package::{EnvironmentID, EnvironmentName, PackageName},
    schema::{
        EXTERNAL_RESOLVE_ARG, ManifestDependencyInfo, ResolveRequest, ResolveResponse,
        ResolverDependencyInfo,
    },
};

use super::{CombinedDependency, Dependency, DependencySet};

/// A [Dependency<Resolved>] is like a [Dependency<Combined>] except that it no longer has
/// externally resolved dependencies
type Resolved = ResolverDependencyInfo;

pub type ResolverName = String;
pub type ResolverResult<T> = Result<T, ResolverError>;

/// A [ResolvedDependency] is like a [CombinedDependency] except that it no longer has
/// externally resolved dependencies
pub struct ResolvedDependency(pub(super) Dependency<Resolved>);

#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("I/O Error when running external resolver `{resolver}`: {source}")]
    IoError {
        resolver: ResolverName,

        #[source]
        source: std::io::Error,
    },

    /// This indicates that the resolver was faulty
    #[error("`{resolver}` did not follow the external resolver protocol ({message})")]
    BadResolver {
        resolver: ResolverName,
        message: String,
    },

    /// This indicates that the resolver returned a non-successful exit code
    #[error("`{resolver}` returned error code: {code}")]
    ResolverUnsuccessful {
        resolver: ResolverName,
        code: ExitStatus,
    },

    /// This indicates that the resolver executed successfully but returned an error
    #[error("`{resolver}` couldn't resolve `{dep}` in environment `{env_str}`: {message}")]
    ResolverFailed {
        resolver: ResolverName,
        dep: PackageName,
        env_str: String,
        message: String,
    },
}

impl ResolvedDependency {
    /// Replace all external dependencies in `deps` with internal dependencies by invoking their
    /// resolvers. Requires all environments in `deps` to be contained in `envs`
    pub async fn resolve(
        deps: DependencySet<CombinedDependency>,
        envs: &BTreeMap<EnvironmentName, EnvironmentID>,
    ) -> ResolverResult<DependencySet<ResolvedDependency>> {
        // iterate over [deps] to collect queries for external resolvers
        let mut requests: BTreeMap<ResolverName, DependencySet<ResolveRequest>> = BTreeMap::new();

        for (env, pkg, dep) in deps.iter() {
            if let Combined::External(ext) = &dep.0.dep_info {
                requests.entry(ext.resolver.clone()).or_default().insert(
                    env.clone(),
                    pkg.clone(),
                    ResolveRequest {
                        env: envs[dep.0.use_environment()].clone(),
                        data: ext.data.clone(),
                    },
                );
            }
        }

        // call the resolvers
        let responses = requests
            .into_iter()
            .map(async |(resolver, reqs)| resolve_single(resolver, reqs).await);
        let mut responses = DependencySet::merge(try_join_all(responses).await?);

        // build the output
        let mut result = DependencySet::new();
        for (env, pkg, dep) in deps.into_iter() {
            let ext = responses.remove(&env, &pkg);
            result.insert(
                env,
                pkg,
                ResolvedDependency(dep.0.map(|info| match info {
                    Combined::Local(loc) => Resolved::Local(loc),
                    Combined::Git(git) => Resolved::Git(git),
                    Combined::OnChain(onchain) => Resolved::OnChain(onchain),
                    Combined::External(_) => {
                        ext.expect("resolve_single outputs same keys as input")
                    }
                })),
            );
        }
        assert!(responses.is_empty());

        Ok(result)
    }
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

/// Resolve the dependencies in `requests` with the external resolver `resolver`
async fn resolve_single(
    resolver: ResolverName,
    requests: DependencySet<ResolveRequest>,
) -> ResolverResult<DependencySet<Resolved>> {
    let (envs, pkgs, reqs): (Vec<_>, Vec<_>, Vec<_>) = requests.into_iter().multiunzip();

    let resps = call_resolver(resolver, reqs);

    let result: DependencySet<ResolveResponse> = izip!(envs, pkgs, resps.await?).collect();

    Ok(result
        .into_iter()
        .map(|(env, pkg, resp)| (env, pkg, resp.0))
        .collect())
}

/// Invoke the `resolver` process and feed it `reqs`; parse the output and log as appropriate
async fn call_resolver(
    resolver: ResolverName,
    reqs: Vec<ResolveRequest>,
) -> ResolverResult<Vec<ResolveResponse>> {
    let mut command = Command::new(&resolver);
    command
        .arg(EXTERNAL_RESOLVE_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    debug!(
        "running external resolver `{} {}`",
        command.as_std().get_program().to_string_lossy(),
        command
            .as_std()
            .get_args()
            .map(OsStr::to_string_lossy)
            .join(" ")
    );

    let mut child = command
        .spawn()
        .map_err(|e| ResolverError::io_error(&resolver, e))?;

    let mut endpoint = Endpoint::new(
        child.stdout.take().expect("stdout is available"),
        child.stdin.take().expect("stdin is available"),
    );

    let resps = endpoint.batch_call("resolve", reqs).await.map_err(|e| {
        debug!("deserialization error: {e:?}");
        ResolverError::bad_resolver(&resolver, e.to_string())
    });

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| ResolverError::io_error(&resolver, e))?;

    // dump standard error
    if !output.stderr.is_empty() {
        info!(
            "Output from {resolver}:\n{}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .map(|l| format!("  │ {l}\n"))
                .join("")
        )
    }

    if !output.status.success() {
        return Err(ResolverError::nonzero_exit(&resolver, output.status));
    }

    Ok(resps?.collect())
}
