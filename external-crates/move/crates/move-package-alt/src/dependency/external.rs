// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ r.<res> = data }`).

use std::{collections::BTreeMap, fmt::Debug, path::Path};

use anyhow::bail;
use futures::future::try_join_all;
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Serialize,
};
use serde_spanned::Spanned;
use tracing::warn;

use crate::{
    errors::{
        self, Located, ManifestError, ManifestErrorKind, PackageError, PackageResult, ResolverError,
    },
    flavor::MoveFlavor,
    package::{EnvironmentName, PackageName},
};

use super::external_protocol::{Query, QueryID, QueryResult, Request};

use super::{pin, DependencySet, ManifestDependencyInfo, PinnedDependencyInfo};

type ResolverName = String;

/// An external dependency has the form `{ r.<res> = <data> }`. External
/// dependencies are resolved by external resolvers.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(try_from = "RField", into = "RField")]
pub struct ExternalDependency {
    /// The `<res>` in `{ r.<res> = <data> }`
    resolver: ResolverName,

    /// the `<data>` in `{ r.<res> = <data> }`
    data: toml::Value,
}

/// Convenience type for serializing/deserializing external deps
#[derive(Serialize, Deserialize)]
struct RField {
    r: BTreeMap<String, toml::Value>,
}

impl ExternalDependency {
    /// Invoke the external binaries to resolve all [deps] in all [envs]; deserialize their outputs
    /// as dependencies.
    ///
    /// Note that the return value may not have entries for all of the environments in [envs]; some
    /// may be removed if they are identical to the default resolutions.
    pub async fn resolve<F: MoveFlavor>(
        deps: DependencySet<ExternalDependency>,
        envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
    ) -> PackageResult<DependencySet<ManifestDependencyInfo<F>>> {
        // split by resolver
        let mut sorted: BTreeMap<ResolverName, DependencySet<toml::Value>> = BTreeMap::new();
        for (env, package_name, dep) in deps.into_iter() {
            sorted
                .entry(dep.resolver)
                .or_default()
                .insert(env, package_name, dep.data);
        }

        // run the resolvers
        let resolved = sorted
            .into_iter()
            .map(move |(resolver, deps)| resolve_single::<F>(resolver, deps, envs));

        let resolved_all = try_join_all(resolved).await?;

        Ok(DependencySet::merge(resolved_all))
    }
}

impl TryFrom<RField> for ExternalDependency {
    type Error = anyhow::Error;

    fn try_from(value: RField) -> Result<Self, Self::Error> {
        if value.r.len() != 1 {
            bail!("Externally resolved dependencies must have exactly one resolver field")
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
        Self {
            r: BTreeMap::from([(value.resolver, value.data)]),
        }
    }
}

/// Resolve the dependencies in [dep_data] with the external resolver [resolver]; requests are
/// performed for all environments in [envs]
async fn resolve_single<F: MoveFlavor>(
    resolver: ResolverName,
    dep_data: DependencySet<toml::Value>,
    envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
) -> PackageResult<DependencySet<ManifestDependencyInfo<F>>> {
    // TODO: method is too long
    let mut request = Request::new(F::name());

    // request default resolution
    let mut default_reqs: BTreeMap<QueryID, Query> = dep_data
        .default_deps()
        .iter()
        .map(|(pkg_name, data)| query::<F>(None, pkg_name.clone(), data.clone()))
        .collect();
    request.queries.append(&mut default_reqs);

    // request env-specific resolutions
    for (env_name, env_id) in envs {
        let mut env_reqs = dep_data
            .deps_for_env(env_name)
            .into_iter()
            .map(|(pkg_name, data)| {
                query::<F>(
                    Some((env_name.clone(), env_id.clone())),
                    pkg_name,
                    data.clone(),
                )
            })
            .collect();
        request.queries.append(&mut env_reqs);
    }

    // invoke the resolver
    let mut response = request.execute(&resolver).await?;

    // build the result
    let resolved: Result<DependencySet<ManifestDependencyInfo<F>>, _> = dep_data
        .into_iter()
        .map(|(env, pkg_name, _)| {
            let query_id = query_id(&env, &pkg_name);
            let result = response
                .responses
                .remove(&query_id)
                .expect("Request::execute returns the same keys as its input");
            let resolved = match result {
                QueryResult::Error { error } => Err(ResolverError::resolver_failed(
                    resolver.clone(),
                    pkg_name,
                    env,
                    error,
                )),
                QueryResult::Success { warnings, resolved } => {
                    // TODO: use diagnostics here
                    for warning in warnings {
                        warn!("{resolver}: {warning}");
                    }

                    if let Ok(result) = ManifestDependencyInfo::<F>::deserialize(resolved) {
                        Ok((env, pkg_name, result))
                    } else {
                        Err(ResolverError::bad_resolver(
                            &resolver,
                            "incorrectly formatted dependency",
                        ))
                    }
                }
            };
            resolved
        })
        .collect();

    Ok(resolved?)
}

/// Generate a unique identifier corresponding to [env] and [pkg]
fn query_id(env: &Option<EnvironmentName>, pkg: &PackageName) -> String {
    format!("({env:?}, {pkg})")
}

/// Output a query for [data] in environment [env]; [pkg] is used to generate the query name
fn query<F: MoveFlavor>(
    env: Option<(EnvironmentName, F::EnvironmentID)>,
    pkg: PackageName,
    data: toml::Value,
) -> (QueryID, Query) {
    let (env_name, env_id) = env.unzip();

    (
        query_id(&env_name, &pkg),
        Query {
            argument: data,
            environment_id: env_id.map(|it| it.to_string()),
        },
    )
}
