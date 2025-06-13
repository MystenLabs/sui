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
    errors::{FileHandle, TheFile},
    flavor::MoveFlavor,
    jsonrpc::Endpoint,
    package::{EnvironmentName, PackageName},
};

use super::{DependencySet, UnpinnedDependencyInfo};

pub type ResolverName = String;
pub type ResolverResult<T> = Result<T, ResolverError>;

pub const RESOLVE_ARG: &str = "--resolve-deps";
pub const RESOLVE_METHOD: &str = "resolve";

/// An external dependency has the form `{ r.<res> = <data> }`. External
/// dependencies are resolved by external resolvers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "RField", into = "RField")]
pub struct ExternalDependency {
    /// The `<res>` in `{ r.<res> = <data> }`
    pub resolver: ResolverName,

    /// The `<data>` in `{ r.<res> = <data> }`
    data: toml::Value,

    /// The file containing this dependency
    containing_file: FileHandle,
}

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

/// Convenience type for serializing/deserializing external deps
#[derive(Serialize, Deserialize)]
struct RField {
    r: BTreeMap<String, toml::Value>,

    #[serde(skip, default = "TheFile::handle")]
    containing_file: FileHandle,
}

/// Requests from the package mananger to the external resolver
#[derive(Serialize, Debug)]
struct ResolveRequest<EnvironmentID: Serialize> {
    #[serde(default)]
    env: EnvironmentID,
    data: toml::Value,
    #[serde(skip)]
    containing_file: FileHandle,
}

/// Responses from the external resolver back to the package manager
#[derive(Deserialize)]
#[serde(bound = "")]
struct ResolveResponse {
    result: UnpinnedDependencyInfo,
    warnings: Vec<String>,
}

impl ExternalDependency {
    /// Replace all [ExternalDependency]s in `deps` with internal dependencies by invoking their
    /// resolvers.
    ///
    /// Note that the set of entries may be changed because external dependencies may be resolved
    /// differently for different environments - this may cause the addition of a new
    /// dep-replacement;
    /// this method may also optimize by removing unnecessary dep-replacements.
    ///
    /// Expects all environments in [deps] to also be contained in [envs]
    pub async fn resolve<EnvironmentID: Serialize + Clone>(
        deps: &mut DependencySet<UnpinnedDependencyInfo>,
        envs: &BTreeMap<EnvironmentName, EnvironmentID>,
    ) -> ResolverResult<()> {
        // iterate over [deps] to collect queries for external resolvers
        let mut requests: BTreeMap<ResolverName, DependencySet<ResolveRequest<EnvironmentID>>> =
            BTreeMap::new();

        for (env, pkg, dep) in deps.iter() {
            if let UnpinnedDependencyInfo::External(dep) = dep {
                let env_id = envs
                    .get(env)
                    .expect("all environments must be in [envs]")
                    .clone();

                requests.entry(dep.resolver.clone()).or_default().insert(
                    env.clone(),
                    pkg.clone(),
                    ResolveRequest {
                        env: env_id,
                        data: dep.data.clone(),
                        containing_file: dep.containing_file,
                    },
                );
            }
        }

        // call the resolvers
        let responses = requests
            .into_iter()
            .map(async |(resolver, reqs)| resolve_single(resolver, reqs).await);
        let responses = DependencySet::merge(try_join_all(responses).await?);

        // put the responses back in (note: insert replaces the old External deps)
        for (env, pkg, dep) in responses.into_iter() {
            deps.insert(env, pkg, dep);
        }

        Ok(())
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

impl TryFrom<RField> for ExternalDependency {
    type Error = String;

    /// Convert from [RField] (`{r.<res> = <data>}`) to [ExternalDependency] (`{ res, data }`)
    fn try_from(value: RField) -> Result<Self, Self::Error> {
        if value.r.len() != 1 {
            return Err("Externally resolved dependencies should have the form `{r.<resolver-name> = <resolver-data>}`".to_string());
        }

        let (resolver, data) = value
            .r
            .into_iter()
            .next()
            .expect("iterator of length 1 structure is nonempty");

        Ok(Self {
            resolver,
            data,
            containing_file: value.containing_file,
        })
    }
}

impl From<ExternalDependency> for RField {
    /// Translate from [ExternalDependency] `{ res, data }` to [RField] `{r.<res> = data}`
    fn from(value: ExternalDependency) -> Self {
        let ExternalDependency {
            resolver,
            data,
            containing_file: containing_dir,
        } = value;

        RField {
            containing_file: containing_dir,
            r: BTreeMap::from([(resolver, data)]),
        }
    }
}

/// Resolve the dependencies in [dep_data] with the external resolver [resolver]; requests are
/// performed for all environments in [envs]. Ensures that the returned dependency set contains no
/// externally resolved dependencies.
///
/// Assumes `requests` is nonempty
async fn resolve_single<E: Serialize>(
    resolver: ResolverName,
    requests: DependencySet<ResolveRequest<E>>,
) -> ResolverResult<DependencySet<UnpinnedDependencyInfo>> {
    let mut command = Command::new(&resolver);
    command
        .arg(RESOLVE_ARG)
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

    let (envs, pkgs, reqs): (Vec<_>, Vec<_>, Vec<_>) = requests.into_iter().multiunzip();

    // TODO: There is a potential bug here: we just use the file from the first request, rather
    // than pairing each request with its own file. This almost certainly isn't a problem in
    // practice because we probably only care about the file if external resolvers are
    // returning local dependencies, which they almost certainly shouldn't. Moreover, we're
    // currently only calling `resolve` on a batch of deps from the same manifest, so there
    // shouldn't be confusion. If this becomes a problem, we probably need to sort out requests
    // into common files, although at that point we'll probably need to replace or fix TheFile
    // anyway due to threading problems.

    let parsing_file = reqs.first().expect("nonempty input").containing_file;

    let resps = TheFile::with_existing(parsing_file, async || {
        endpoint
            .batch_call("resolve", reqs)
            .await
            .map_err(|e| ResolverError::bad_resolver(&resolver, e.to_string()))
    })
    .await;

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

    let result: DependencySet<UnpinnedDependencyInfo> = izip!(envs, pkgs, resps?).collect();

    // ensure no externally resolved responses
    for (_, _, dep) in result.iter() {
        if let UnpinnedDependencyInfo::External(_) = dep {
            return Err(ResolverError::bad_resolver(
                &resolver,
                "resolvers must return resolved dependencies",
            ));
        }
    }

    Ok(result)
}
