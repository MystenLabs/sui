// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ r.<res> = data }`).

use std::{
    collections::BTreeMap,
    fmt::Debug,
    iter::once,
    ops::Range,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::bail;
use futures::future::{join_all, try_join_all};
use itertools::{Itertools, izip};
use serde::{
    Deserialize, Serialize,
    de::{MapAccess, Visitor},
};
use serde_spanned::Spanned;
use tokio::{io::AsyncReadExt, process::Command};
use tracing::{debug, info, warn};

use crate::{
    errors::{
        self, FileHandle, Located, ManifestError, ManifestErrorKind, PackageError, PackageResult,
        ResolverError,
    },
    flavor::MoveFlavor,
    jsonrpc::Endpoint,
    package::{EnvironmentName, PackageName},
};

use super::{DependencySet, ManifestDependencyInfo, PinnedDependencyInfo, pin};

pub type ResolverName = String;

pub const RESOLVE_ARG: &str = "--resolve-deps";
pub const RESOLVE_METHOD: &str = "resolve";

/// An external dependency has the form `{ r.<res> = <data> }`. External
/// dependencies are resolved by external resolvers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "RField", into = "RField")]
pub struct ExternalDependency {
    /// The `<res>` in `{ r.<res> = <data> }`
    pub resolver: ResolverName,

    /// the `<data>` in `{ r.<res> = <data> }`
    data: toml::Value,
}

/// Convenience type for serializing/deserializing external deps
#[derive(Serialize, Deserialize)]
struct RField {
    r: BTreeMap<String, toml::Value>,
}

/// Requests from the package mananger to the external resolver
#[derive(Serialize, Debug)]
struct ResolveRequest<F: MoveFlavor> {
    #[serde(default)]
    env: Option<F::EnvironmentID>,
    data: toml::Value,
}

/// Responses from the external resolver back to the package manager
#[derive(Deserialize)]
#[serde(bound = "")]
struct ResolveResponse<F: MoveFlavor> {
    result: ManifestDependencyInfo<F>,
    warnings: Vec<String>,
}

impl ExternalDependency {
    /// Replace all [ExternalDependency]s in `deps` with internal dependencies by invoking their
    /// resolvers.
    ///
    /// Note that the set of entries may be changed because external dependencies may be resolved
    /// differently for different environments - this may cause the addition of a new dep-override;
    /// this method may also optimize by removing unnecessary dep-overrides.
    ///
    /// Expects all environments in [deps] to also be contained in [envs]
    pub async fn resolve<F: MoveFlavor>(
        deps: &mut DependencySet<ManifestDependencyInfo<F>>,
        envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
    ) -> PackageResult<()> {
        // we explode [deps] first so that we know exactly which deps are needed for each env.
        deps.explode(envs.keys().cloned());

        // iterate over [deps] to collect queries for external resolvers
        let mut requests: BTreeMap<ResolverName, DependencySet<ResolveRequest<F>>> =
            BTreeMap::new();

        for (env, pkg, dep) in deps.iter() {
            if let ManifestDependencyInfo::External::<F>(dep) = dep {
                let env_id = env.map(|id| {
                    envs.get(id)
                        .expect("all environments must be in [envs]")
                        .clone()
                });
                requests.entry(dep.resolver.clone()).or_default().insert(
                    env.cloned(),
                    pkg.clone(),
                    ResolveRequest {
                        env: env_id,
                        data: dep.data.clone(),
                    },
                );
            }
        }

        // call the resolvers
        let responses = DependencySet::merge(
            try_join_all(
                requests
                    .into_iter()
                    .map(async |(resolver, reqs)| resolve_single(resolver, reqs).await),
            )
            .await?,
        );

        // put the responses back in (note: insert replaces the old External deps)
        for (env, pkg, dep) in responses.into_iter() {
            deps.insert(env, pkg, dep);
        }

        debug!("done resolving");

        Ok(())
    }
}

impl TryFrom<RField> for ExternalDependency {
    type Error = PackageError;

    fn try_from(value: RField) -> Result<Self, Self::Error> {
        debug!("try_from: {:?}", value.r);
        if value.r.len() != 1 {
            return Err(PackageError::Generic("TODO".to_string()));
            //            return Err(PackageError::Manifest(ManifestError {
            //                kind: ManifestErrorKind::BadExternalDependency,
            //                span: Some(value.r.span()),
            //                handle: value.r.file(),
            //            }));
        }

        let (resolver, data) = value
            .r
            .into_iter()
            .next()
            .expect("iterator of length 1 structure is nonempty");

        Ok(Self { resolver, data })
    }
}

impl From<ExternalDependency> for RField {
    fn from(value: ExternalDependency) -> Self {
        let ExternalDependency { resolver, data } = value;

        RField {
            r: BTreeMap::from([(resolver, data)]),
        }
    }
}

/// Resolve the dependencies in [dep_data] with the external resolver [resolver]; requests are
/// performed for all environments in [envs]. Ensures that the returned dependency set contains no
/// externally resolved dependencies.
async fn resolve_single<F: MoveFlavor>(
    resolver: ResolverName,
    requests: DependencySet<ResolveRequest<F>>,
) -> PackageResult<DependencySet<ManifestDependencyInfo<F>>> {
    let mut child = Command::new(&resolver)
        .arg(RESOLVE_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ResolverError::io_error(&resolver, e))?;

    let mut endpoint = Endpoint::new(
        child.stdout.take().expect("stdout is available"),
        child.stdin.take().expect("stdin is available"),
    );

    let (envs, pkgs, reqs): (Vec<_>, Vec<_>, Vec<_>) = requests.into_iter().multiunzip();

    debug!(
        "requests for {resolver}:\n{}",
        serde_json::to_string_pretty(&reqs).unwrap_or("serialization error".to_string())
    );

    // TODO
    let resps = endpoint
        .batch_call("resolve", reqs)
        .await
        .map_err(|e| ResolverError::bad_resolver(&resolver, e.to_string()));

    // dump standard error
    let output = child.wait_with_output().await?;

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
        return Err(ResolverError::nonzero_exit(&resolver, output.status).into());
    }

    let result: DependencySet<ManifestDependencyInfo<F>> = izip!(envs, pkgs, resps?).collect();

    // ensure no externally resolved responses
    for (_, _, dep) in result.iter() {
        if let ManifestDependencyInfo::External(_) = dep {
            return Err(ResolverError::bad_resolver(
                &resolver,
                "resolvers must return resolved dependencies",
            )
            .into());
        }
    }

    Ok(result)
}
